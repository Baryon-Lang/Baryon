//! The driver: the application that wires the compiler's stages together.
//!
//! It owns the concrete database, which implements each stage query of [`database::Db`] by
//! calling the implementation crate for it, inside a `#[salsa::tracked]` function so the result
//! is cached. Today the stages are the `parser`, the `syntax_check`, the `resolver` and the
//! `checker`, which also answers [`types::TypeDb`]'s queries. The `documentation` generator
//! reads the parser's trees for [`Session::document`], and `lowering` turns a checked function
//! into the mid-level IR for [`Session::mir`]. Swapping an implementation means
//! changing the one function here that calls it. A [`Session`] holds that database with one
//! package loaded into it.
//!
//! The `baryon` executable (`src/main.rs`) is a thin command line over this library:
//! `baryon check <directory>` loads every source file under the directory, checks the package, and
//! prints its diagnostics; `baryon doc <directory> <output>` writes its documentation pages.
//!
//! # Example
//!
//! ```
//! use driver::Session;
//!
//! let mut session = Session::new();
//! let file = session.add_file("main.byn", "STEPS: Final = 100\n")?;
//! assert!(session.check().is_empty());
//!
//! // An edit: only this file is parsed again.
//! session.set_text(file, "STEPS: Final = \n")?;
//! let problems = session.check();
//! assert_eq!(problems.len(), 1);
//! assert!(diagnostics::render(&problems[0], &session).to_string().contains("main.byn:1:16"));
//! # Ok::<(), source::SourceError>(())
//! ```

mod compile;
mod lowered;

use database::{Db, File, Package, ParsedFile, Resolution};
use diagnostics::Diagnostic;
use documentation::ModuleDocumentation;
use salsa::Setter;
use source::{FileId, SourceError, SourceFile, Sources};
use std::fmt;
use types::{Checked, Item, TypeDb, TypedBody};

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
    /// such as `physics/nbody.byn`; it decides the module name.
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
    /// let file = session.add_file("heat.byn", "ALPHA: Final = 0.1\n")?;
    /// assert_eq!(session.file(session.file_id(file)).name(), "heat.byn");
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

    /// The documentation of every module in the package, in the order the files were added,
    /// read by the `documentation` crate; or, when the package has module naming errors or
    /// syntax errors, those problems instead, since a module without a name or a tree has no
    /// page. Type errors do not stop documentation.
    ///
    /// ```
    /// use driver::Session;
    ///
    /// let mut session = Session::new();
    /// session.add_file("ode/rk4.byn", "\"Runge-Kutta integrators.\"\n")?;
    /// let modules = session.document().unwrap();
    /// assert_eq!(modules[0].name(), "ode.rk4");
    /// assert_eq!(modules[0].summary(), Some("Runge-Kutta integrators."));
    ///
    /// session.add_file("broken.byn", "def f(:\n")?;
    /// assert!(session.document().is_err());
    /// # Ok::<(), source::SourceError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns the module naming errors and syntax errors, in the order [`check`](Self::check)
    /// reports them, when there are any.
    pub fn document(&self) -> Result<Vec<ModuleDocumentation>, Vec<Diagnostic>> {
        let db = &self.database;
        let mut problems = database::modules(db, self.package).diagnostics().to_vec();
        let mut modules = Vec::new();
        for &file in &self.files {
            problems.extend_from_slice(db.parse(file).diagnostics());
            if let Ok(name) = database::module_name(db, file) {
                let text = file.source(db).text();
                let tree = db.parse(file).tree();
                modules.push(documentation::document(tree, text, &name.to_string()));
            }
        }
        if problems.is_empty() {
            Ok(modules)
        } else {
            Err(problems)
        }
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

    fn body<'db>(&'db self, item: Item<'db>) -> &'db TypedBody<'db> {
        body(self, item)
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

/// The body query: runs the `checker` on one function or method body. Cached until the
/// function's file, its names, or a declaration it reads changes. `check` and lowering both
/// read it, so each body is checked once per revision.
#[salsa::tracked(returns(ref))]
fn body<'db>(db: &'db dyn TypeDb, item: Item<'db>) -> TypedBody<'db> {
    checker::body(db, item)
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
mod tests;
