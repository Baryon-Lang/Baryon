//! The driver: the application that wires the compiler's stages together.
//!
//! It owns the concrete database, which implements each stage query of [`database::Db`] by
//! calling the implementation crate for it, inside a `#[salsa::tracked]` function so the result
//! is cached. Today that is one stage, the `parser`. Swapping an implementation means changing
//! the one function here that calls it. A [`Session`] holds that database with one package
//! loaded into it.
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
//! let file = session.add_file("main.npl", "steps = 100\n")?;
//! assert!(session.check().is_empty());
//!
//! // An edit: only this file is parsed again.
//! session.set_text(file, "steps = \n")?;
//! let problems = session.check();
//! assert_eq!(problems.len(), 1);
//! assert!(diagnostics::render(&problems[0], &session).to_string().contains("main.npl:1:9"));
//! # Ok::<(), source::SourceError>(())
//! ```

use database::{Db, File, Package, ParsedFile};
use diagnostics::Diagnostic;
use salsa::Setter;
use source::{FileId, SourceError, SourceFile, Sources};
use std::fmt;

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

    /// Every problem in the package: module naming errors first, then each file's syntax
    /// errors, in the order the files were added. Cached: after an edit, only the edited file
    /// is parsed again.
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

/// The check query behind [`Session::check`]. Its result compares equal after an edit that
/// changes no diagnostic, so anything that reads it later stays cached.
#[salsa::tracked(returns(ref))]
fn check(db: &dyn Db, package: Package) -> Vec<Diagnostic> {
    let mut problems = database::modules(db, package).diagnostics().to_vec();
    for &file in package.files(db) {
        problems.extend_from_slice(db.parse(file).diagnostics());
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

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
        let first = session.add_file("first.npl", "x = 1\n").unwrap();
        session.add_file("second.npl", "y = 2\n").unwrap();
        assert!(session.check().is_empty());
        assert_eq!(parses(&log), 2);

        session.set_text(first, "x = 3\n").unwrap();
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
}
