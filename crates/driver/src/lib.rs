//! The driver: the application that wires the compiler's stages together.
//!
//! It owns the concrete database, which implements each stage query of [`database::Db`] by
//! calling the implementation crate for it, inside a `#[salsa::tracked]` function so the result
//! is cached. Today the stages are the `parser`, the `syntax_check`, the `resolver` and the
//! `checker`, which also answers [`types::TypeDb`]'s query. Swapping an implementation means
//! changing the one function here that calls it. A [`Session`] holds that database with one
//! package loaded into it.
//!
//! The `npl` executable (`src/main.rs`) is a thin command line over this library:
//! `npl check <directory>` loads every source file under the directory, checks the package, and
//! prints its diagnostics.
//!
//! # Example
//!
//! ```
//! use driver::Session;
//!
//! let mut session = Session::new();
//! let file = session.add_file("main.npl", "STEPS: Final = 100\n")?;
//! assert!(session.check().is_empty());
//!
//! // An edit: only this file is parsed again.
//! session.set_text(file, "STEPS: Final = \n")?;
//! let problems = session.check();
//! assert_eq!(problems.len(), 1);
//! assert!(diagnostics::render(&problems[0], &session).to_string().contains("main.npl:1:16"));
//! # Ok::<(), source::SourceError>(())
//! ```

use database::{Db, File, Package, ParsedFile, Resolution};
use diagnostics::Diagnostic;
use salsa::Setter;
use source::{FileId, SourceError, SourceFile, Sources};
use std::fmt;
use types::{Checked, Item, TypeDb};

/// The compiler's incremental database: every input and every cached query result.
#[salsa::db]
#[derive(Clone, Default)]
struct Database {
    /// Salsa's inputs and caches.
    storage: salsa::Storage<Self>,
}

/// One package open in the compiler: its source files and every cached query result.
///
/// Changing an input, with [`add_file`](Self::add_file) or [`set_text`](Self::set_text), keeps
/// every cached result that did not read it. A session implements [`Sources`], so diagnostics
/// render straight from it.
pub struct Session {
    /// The database that holds the inputs and caches.
    database: Database,
    /// Every file added, indexed by its [`FileId`].
    files: Vec<File>,
    /// The package all the files belong to.
    package: Package,
}

impl Session {
    /// An empty session: one package with no files.
    #[must_use]
    pub fn new() -> Self {
        Self::over(Database::default())
    }

    /// An empty session over the given session.
    fn over(database: Database) -> Self {
        let package = Package::new(&database, Vec::new());
        Self {
            database,
            files: Vec::new(),
            package,
        }
    }

    /// Adds a source file to the package and returns it.
    ///
    /// `name` is the file's path relative to the package root, with `/` between directories,
    /// such as `physics/nbody.npl`; it decides the module name.
    ///
    /// # Errors
    ///
    /// Fails if the text is 4 GiB or larger, or if the session already holds `u32::MAX` files.
    pub fn add_file(
        &mut self,
        name: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<File, SourceError> {
        let index = u32::try_from(self.files.len()).map_err(|_| SourceError::TooManyFiles)?;
        let source = SourceFile::new(name, text)?;
        let file = File::new(&self.database, FileId::from_index(index), source);
        self.files.push(file);
        self.package
            .set_files(&mut self.database)
            .to(self.files.clone());
        Ok(file)
    }

    /// Replaces the text of `file`, as an editor does on every keystroke. The file keeps its
    /// name and id.
    ///
    /// # Errors
    ///
    /// Fails if the text is 4 GiB or larger. The file is unchanged when it fails.
    pub fn set_text(&mut self, file: File, text: impl Into<String>) -> Result<(), SourceError> {
        let source = SourceFile::new(file.source(&self.database).name(), text)?;
        file.set_source(&mut self.database).to(source);
        Ok(())
    }

    /// The id of `file`, which the spans of its diagnostics carry. Files get ids in the order
    /// they are added, starting at 0.
    ///
    /// ```
    /// use driver::Session;
    /// use source::Sources;
    ///
    /// let mut session = Session::new();
    /// let file = session.add_file("heat.npl", "ALPHA: Final = 0.1\n")?;
    /// assert_eq!(session.file(session.file_id(file)).name(), "heat.npl");
    /// # Ok::<(), source::SourceError>(())
    /// ```
    #[must_use]
    pub fn file_id(&self, file: File) -> FileId {
        file.id(&self.database)
    }

    /// Every problem in the package: module naming errors first, then each file's problems, in
    /// the order the files were added. A file's problems are its syntax errors or, when it has
    /// none, the constructs the language does not support, the names that refer to nothing and
    /// the type errors in its declarations. Cached: after an edit, only the edited file is
    /// parsed again.
    #[must_use]
    pub fn check(&self) -> Vec<Diagnostic> {
        check(&self.database, self.package).clone()
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Session {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Session")
            .field("files", &self.files.len())
            .finish_non_exhaustive()
    }
}

impl Sources for Session {
    /// # Panics
    ///
    /// Panics if `id` was issued by a different session.
    fn file(&self, id: FileId) -> &SourceFile {
        self.files[id.index() as usize].source(&self.database)
    }
}

#[salsa::db]
impl salsa::Database for Database {}

#[salsa::db]
impl Db for Database {
    fn parse(&self, file: File) -> &ParsedFile {
        parse(self, file)
    }

    fn resolve(&self, package: Package, file: File) -> &Resolution {
        resolve(self, package, file)
    }
}

#[salsa::db]
impl TypeDb for Database {
    fn declaration<'db>(&'db self, item: Item<'db>) -> &'db Checked<'db> {
        declaration(self, item)
    }
}

/// The parse query: runs the `parser` crate on one file. Cached until the file's source
/// changes. `no_eq` because a syntax tree has no cheap equality; queries that read the tree
/// compare their own, smaller results instead.
#[salsa::tracked(returns(ref), no_eq)]
fn parse(db: &dyn Db, file: File) -> ParsedFile {
    let source = file.source(db);
    let (tree, diagnostics) = parser::parse(file.id(db), source.text()).into_parts();
    ParsedFile::new(tree, diagnostics)
}

/// The resolve query: runs the `resolver` crate on one file. Cached until the file's tree, the
/// module map, or the top-level names of a module the file imports from change.
#[salsa::tracked(returns(ref))]
fn resolve(db: &dyn Db, package: Package, file: File) -> Resolution {
    resolver::resolve(db, package, file)
}

/// The syntax check query: runs the `syntax_check` crate on one file, reporting the Python
/// constructs the language does not support. Cached until the file's source changes.
#[salsa::tracked(returns(ref))]
fn unsupported(db: &dyn Db, file: File) -> Vec<Diagnostic> {
    syntax_check::check(db.parse(file).tree(), file.source(db).text())
}

/// The declaration query: runs the `checker` on one definition. Cached until the definition's
/// file, its names, or a declaration it reads changes. An alias or constant defined in terms of
/// itself ends the cycle with [`declaration_cycle`].
#[salsa::tracked(returns(ref), cycle_result = declaration_cycle)]
fn declaration<'db>(db: &'db dyn TypeDb, item: Item<'db>) -> Checked<'db> {
    checker::declaration(db, item)
}

/// The declaration of an item whose declaration needs itself: the `checker`'s cycle error.
fn declaration_cycle<'db>(db: &'db dyn TypeDb, _: salsa::Id, item: Item<'db>) -> Checked<'db> {
    checker::cycle(db, item)
}

/// The type check query: the type errors of one file's declarations. Cached until the file or
/// a declaration it reads changes.
#[salsa::tracked(returns(ref))]
fn type_errors(db: &dyn TypeDb, package: Package, file: File) -> Vec<Diagnostic> {
    checker::check_file(db, package, file)
}

/// The check query behind [`Session::check`]. Its result compares equal after an edit that
/// changes no diagnostic, so anything that reads it later stays cached.
#[salsa::tracked(returns(ref))]
fn check(db: &dyn TypeDb, package: Package) -> Vec<Diagnostic> {
    let mut problems = database::modules(db, package).diagnostics().to_vec();
    for &file in package.files(db) {
        let syntax = db.parse(file).diagnostics();
        problems.extend_from_slice(syntax);
        // A file with syntax errors has `Missing` nodes and half-built statements, so later
        // stages would report noise; its syntax errors come first.
        if syntax.is_empty() {
            problems.extend_from_slice(unsupported(db, file));
            problems.extend_from_slice(db.resolve(package, file).diagnostics());
            problems.extend_from_slice(type_errors(db, package, file));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::Definition;
    use std::sync::{Arc, Mutex};
    use syntax::Role;
    use types::{Declaration, Mode, Value};

    /// A session that records the name of every query it runs, in order.
    fn logged() -> (Session, Arc<Mutex<Vec<String>>>) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&log);
        let storage = salsa::Storage::new(Some(Box::new(move |event: salsa::Event| {
            if let salsa::EventKind::WillExecute { database_key } = event.kind {
                sink.lock().unwrap().push(format!("{database_key:?}"));
            }
        })));
        (Session::over(Database { storage }), log)
    }

    /// Takes the queries run since the last call, keeping only the parse queries.
    fn parses(log: &Mutex<Vec<String>>) -> usize {
        let mut log = log.lock().unwrap();
        let count = log.iter().filter(|key| key.starts_with("parse(")).count();
        log.clear();
        count
    }

    #[test]
    fn an_edit_reparses_only_the_edited_file() {
        let (mut session, log) = logged();
        let first = session.add_file("first.npl", "X: Final = 1\n").unwrap();
        session.add_file("second.npl", "Y: Final = 2\n").unwrap();
        assert!(session.check().is_empty());
        assert_eq!(parses(&log), 2);

        session.set_text(first, "X: Final = 3\n").unwrap();
        assert!(session.check().is_empty());
        assert_eq!(parses(&log), 1);

        assert!(session.check().is_empty());
        assert_eq!(parses(&log), 0);
    }

    #[test]
    fn diagnostics_render_from_the_database() {
        let mut session = Session::new();
        session.add_file("physics/bad-name.npl", "").unwrap();
        session.add_file("main.npl", "x = \n").unwrap();
        let rendered: Vec<String> = session
            .check()
            .iter()
            .map(|diagnostic| diagnostics::render(diagnostic, &session).to_string())
            .collect();
        assert_eq!(
            rendered,
            [
                "error: `physics/bad-name.npl` cannot be imported: `bad-name` is not a valid \
                 module name\n = note: module names follow Python's rules for names: letters, \
                 digits and underscores, not starting with a digit",
                "error: expected an expression\n --> main.npl:1:5\n  |\n1 | x = \n  |     ^ an expression should be here",
            ]
        );
    }

    #[test]
    fn files_get_ids_in_the_order_they_are_added() {
        let mut session = Session::new();
        let first = session.add_file("a.npl", "").unwrap();
        let second = session.add_file("b.npl", "").unwrap();
        assert_eq!(first.id(&session.database), FileId::from_index(0));
        assert_eq!(second.id(&session.database), FileId::from_index(1));
        assert_eq!(session.file(second.id(&session.database)).name(), "b.npl");
    }

    #[test]
    fn every_use_of_a_name_points_to_its_definition() {
        let mut session = Session::new();
        let nbody = session
            .add_file("nbody.npl", "def step(dt: f64) -> f64:\n    return dt\n")
            .unwrap();
        let main = session
            .add_file(
                "main.npl",
                "from nbody import step\n\n\ndef main():\n    x = 1.0\n    x = step(x)\n",
            )
            .unwrap();
        let database = &session.database;
        let resolution = database.resolve(session.package, main);
        let tree = database.parse(main).tree();
        let text = main.source(database).text();
        let definition_of = |wanted: &str, occurrence: usize| {
            let node = tree
                .nodes()
                .iter()
                .enumerate()
                .filter(|(_, node)| {
                    node.kind() == syntax::SyntaxKind::Name
                        && &text[node.start() as usize..node.end() as usize] == wanted
                })
                .nth(occurrence)
                .map(|(index, _)| index)
                .unwrap();
            resolution
                .references()
                .iter()
                .find(|(id, _)| id.index() == node)
                .map(|(_, definition)| definition.clone())
                .unwrap()
        };
        let Definition::Local(first) = definition_of("x", 0) else {
            panic!("`x` is not a local");
        };
        for occurrence in 1..3 {
            assert_eq!(definition_of("x", occurrence), Definition::Local(first));
        }
        let Definition::Item(file, node) = definition_of("step", 1) else {
            panic!("`step` is not an item");
        };
        assert_eq!(file, nbody);
        let nbody_tree = database.parse(nbody).tree();
        assert_eq!(nbody_tree.text(node, nbody.source(database).text()), "step");
    }

    #[test]
    fn declarations_carry_types_and_values() {
        let mut session = Session::new();
        let program = "LATTICE: Final = 8\nN: Final = LATTICE**3\nBOX: Final = LATTICE * 1.5\n\
                       Vec3 = Fixed[f64, (3,)]\n\n\n\
                       def step(atoms: InOut[Array[Vec3, 1]], dt: f64) -> f64:\n    return dt\n";
        let main = session.add_file("main.npl", program).unwrap();
        let database = &session.database;
        let tree = database.parse(main).tree();
        let declaration = |index: usize| {
            let statement = tree.children(tree.root()).nth(index).unwrap();
            let name = tree
                .child(statement, Role::Target)
                .or_else(|| tree.child(statement, Role::Name))
                .unwrap();
            let item = Item::new(database, session.package, main, name);
            &database.declaration(item).declaration
        };
        let show = |ty: types::Type<'_>| ty.display(database).to_string();

        let Declaration::Constant { ty, value } = declaration(1) else {
            panic!("`N` is not a constant");
        };
        assert_eq!(
            (show(*ty), *value),
            ("i64".to_owned(), Some(Value::Integer(512)))
        );
        let Declaration::Constant { ty, value } = declaration(2) else {
            panic!("`BOX` is not a constant");
        };
        let twelve = Some(Value::Float(12.0_f64.to_bits()));
        assert_eq!((show(*ty), *value), ("f64".to_owned(), twelve));
        let Declaration::Alias(vec3) = declaration(3) else {
            panic!("`Vec3` is not an alias");
        };
        assert_eq!(show(*vec3), "Fixed[f64, (3,)]");
        let Declaration::Function(signature) = declaration(4) else {
            panic!("`step` is not a function");
        };
        let parameters: Vec<_> = signature
            .parameters
            .iter()
            .map(|parameter| (parameter.name.as_str(), show(parameter.ty), parameter.mode))
            .collect();
        assert_eq!(
            parameters,
            [
                (
                    "atoms",
                    "Array[Fixed[f64, (3,)], 1]".to_owned(),
                    Mode::InOut
                ),
                ("dt", "f64".to_owned(), Mode::Borrow),
            ]
        );
        assert_eq!(show(signature.returns), "f64");
    }
}
