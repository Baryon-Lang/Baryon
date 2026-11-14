//! The driver: the application that wires the compiler's stages together.
//!
//! It owns the concrete database, which implements each stage query of [`database::Db`] by
//! calling the implementation crate for it, inside a `#[salsa::tracked]` function so the result
//! is cached. Today the stages are the `parser`, the `syntax_check`, the `resolver` and the
//! `checker`, which also answers [`types::TypeDb`]'s queries. The `documentation` generator
//! reads the parser's trees for [`Session::document`], and `lowering` turns a checked function
//! into the mid-level IR for [`Session::mir`]. An editor's questions about a place in the code,
//! [`Session::hover`], [`Session::definition`] and [`Session::complete`], read the same cached
//! results. Swapping an implementation means changing the one function here that calls it. A
//! [`Session`] holds that database with one package loaded into it.
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
mod editor;
mod lowered;

pub use editor::{Completion, CompletionKind, Hover, InlayHint, TestLens};

use database::{Db, File, Package, ParsedFile, Resolution};
use diagnostics::Diagnostic;
use documentation::ModuleDocumentation;
use salsa::Setter;
use source::{FileId, SourceError, SourceFile, Sources};
use std::fmt;
use types::{Checked, Item, TypeDb, TypedBody};

/// The standard library's source files (decision 0038): each file's path under
/// [`database::LIBRARY_ROOT`] and its text, compiled into the driver so every package has them.
const LIBRARY: [(&str, &str); 42] = [
    ("<std>/arrays.byn", include_str!("../library/arrays.byn")),
    ("<std>/artist.byn", include_str!("../library/artist.byn")),
    ("<std>/axis.byn", include_str!("../library/axis.byn")),
    (
        "<std>/backend_agg.byn",
        include_str!("../library/backend_agg.byn"),
    ),
    (
        "<std>/backend_bases.byn",
        include_str!("../library/backend_bases.byn"),
    ),
    (
        "<std>/backend_pdf.byn",
        include_str!("../library/backend_pdf.byn"),
    ),
    (
        "<std>/backend_svg.byn",
        include_str!("../library/backend_svg.byn"),
    ),
    (
        "<std>/builtins.byn",
        include_str!("../library/builtins.byn"),
    ),
    ("<std>/byplot.byn", include_str!("../library/byplot.byn")),
    ("<std>/bz2.byn", include_str!("../library/bz2.byn")),
    ("<std>/cm.byn", include_str!("../library/cm.byn")),
    (
        "<std>/colorbar.byn",
        include_str!("../library/colorbar.byn"),
    ),
    ("<std>/colors.byn", include_str!("../library/colors.byn")),
    (
        "<std>/compression.zstd.byn",
        include_str!("../library/compression.zstd.byn"),
    ),
    ("<std>/contour.byn", include_str!("../library/contour.byn")),
    (
        "<std>/dejavu_sans.byn",
        include_str!("../library/dejavu_sans.byn"),
    ),
    ("<std>/fft.byn", include_str!("../library/fft.byn")),
    ("<std>/figure.byn", include_str!("../library/figure.byn")),
    ("<std>/fits.byn", include_str!("../library/fits.byn")),
    ("<std>/font.byn", include_str!("../library/font.byn")),
    (
        "<std>/frame_joins.byn",
        include_str!("../library/frame_joins.byn"),
    ),
    (
        "<std>/frame_missing.byn",
        include_str!("../library/frame_missing.byn"),
    ),
    ("<std>/frames.byn", include_str!("../library/frames.byn")),
    ("<std>/gzip.byn", include_str!("../library/gzip.byn")),
    ("<std>/h5by.byn", include_str!("../library/h5by.byn")),
    ("<std>/image.byn", include_str!("../library/image.byn")),
    ("<std>/legend.byn", include_str!("../library/legend.byn")),
    ("<std>/linalg.byn", include_str!("../library/linalg.byn")),
    ("<std>/lzma.byn", include_str!("../library/lzma.byn")),
    ("<std>/math.byn", include_str!("../library/math.byn")),
    ("<std>/netCDF4.byn", include_str!("../library/netCDF4.byn")),
    ("<std>/os.byn", include_str!("../library/os.byn")),
    ("<std>/pathlib.byn", include_str!("../library/pathlib.byn")),
    (
        "<std>/pdf_file.byn",
        include_str!("../library/pdf_file.byn"),
    ),
    ("<std>/png.byn", include_str!("../library/png.byn")),
    ("<std>/raster.byn", include_str!("../library/raster.byn")),
    ("<std>/special.byn", include_str!("../library/special.byn")),
    ("<std>/stats.byn", include_str!("../library/stats.byn")),
    (
        "<std>/subprocess.byn",
        include_str!("../library/subprocess.byn"),
    ),
    ("<std>/ticker.byn", include_str!("../library/ticker.byn")),
    (
        "<std>/transforms.byn",
        include_str!("../library/transforms.byn"),
    ),
    ("<std>/zlib.byn", include_str!("../library/zlib.byn")),
];

/// The compiler's incremental database: every input and every cached query result.
#[salsa::db]
#[derive(Clone, Default)]
pub(crate) struct Database {
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
    /// The standard library's files, from [`LIBRARY`]. They take the highest file ids, counting
    /// down from `u32::MAX`, so the files a program adds keep ids from 0.
    library: Vec<File>,
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
        let library: Vec<File> = LIBRARY
            .iter()
            .zip((0..=u32::MAX).rev())
            .filter_map(|(&(name, text), index)| {
                let source = SourceFile::new(name, text).ok()?;
                Some(File::new(&database, FileId::from_index(index), source))
            })
            .collect();
        let package = Package::new(&database, library.clone());
        Self {
            database,
            files: Vec::new(),
            library,
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
        let files = self.library.iter().chain(&self.files).copied().collect();
        self.package.set_files(&mut self.database).to(files);
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
        let index = id.index() as usize;
        let file = self.files.get(index).copied().unwrap_or_else(|| {
            let from_top = u32::MAX - id.index();
            self.library[from_top as usize]
        });
        file.source(&self.database)
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

/// Which kind of build to compile: what happens when integer arithmetic overflows (decision
/// 0030). Both are optimized and carry line tables; only the overflow check differs.
///
/// ```
/// use driver::{Profile, Session};
///
/// let mut session = Session::new();
/// session.add_file("main.byn", "def main():\n    n = 7\n    print(n * 6)\n")?;
/// let debug = session.compile(None, Profile::Debug).unwrap();
/// assert!(debug.contains("integer overflow in `*`"), "{debug}");
/// let release = session.compile(None, Profile::Release).unwrap();
/// assert!(!release.contains("integer overflow"), "{release}");
/// # Ok::<(), source::SourceError>(())
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Profile {
    /// The default, as `baryon build` with no flag: integer `+`, `-`, `*` and negation that
    /// overflow panic with the operator and the source location, so a wrong answer stops the
    /// program instead of flowing into the results.
    #[default]
    Debug,
    /// `baryon build --release`: overflowing integer arithmetic wraps around in two's
    /// complement, as C and Fortran compilers in practice and Rust's release builds do, and the
    /// checks cost nothing. Every other check, such as array bounds, stays.
    Release,
}

#[cfg(test)]
mod tests;
