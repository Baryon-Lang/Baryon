//! The module database: which module each file is, and the map from module names to files.
//!
//! One file is one module, and a directory is a package that needs no `__init__` file (language
//! reference, "Syntax", "Source files"). So a file's module name
//! is its path relative to the package root, without the `.byn` extension, with `.` in place of
//! `/`: `physics/nbody.byn` is the module `physics.nbody`, inside the package `physics`.

use crate::{Db, File, Package};
use diagnostics::Diagnostic;
use std::collections::BTreeMap;
use std::fmt;

/// The extension every source file has, fixed by decision 0023.
const EXTENSION: &str = ".byn";

/// Python's reserved words. A module named after one could never be imported, because
/// `import class` is a syntax error.
const KEYWORDS: [&str; 35] = [
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

/// A module's full name, such as `physics.nbody`: the names of its packages, then its own.
///
/// Every component is a valid Python name that is not a keyword, so the module can be named
/// in an `import` statement.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleName {
    /// The components in order, outermost package first. Never empty.
    components: Vec<String>,
}

impl ModuleName {
    /// Parses a dotted name as written in an `import` statement, or returns `None` if any
    /// component is not a valid module name.
    ///
    /// ```
    /// use database::ModuleName;
    ///
    /// let name = ModuleName::from_dotted("physics.nbody").unwrap();
    /// assert_eq!(name.components(), ["physics", "nbody"]);
    /// assert_eq!(name.to_string(), "physics.nbody");
    /// assert!(ModuleName::from_dotted("physics..nbody").is_none());
    /// assert!(ModuleName::from_dotted("lambda").is_none());
    /// ```
    #[must_use]
    pub fn from_dotted(dotted: &str) -> Option<Self> {
        let components: Vec<String> = dotted.split('.').map(str::to_owned).collect();
        components
            .iter()
            .all(|component| is_module_name(component))
            .then_some(Self { components })
    }

    /// The components in order, outermost package first. Never empty.
    #[must_use]
    pub fn components(&self) -> &[String] {
        &self.components
    }
}

impl fmt::Display for ModuleName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.components.join("."))
    }
}

/// The module name of `file`, from its path, or the error that says why the path names no
/// module. Cached until the file's source is replaced.
///
/// ```
/// use database::{File, ModuleName, module_name};
/// # use database::{Db, ParsedFile};
/// use source::{FileId, SourceFile};
///
/// # #[salsa::db]
/// # #[derive(Clone, Default)]
/// # struct Database { storage: salsa::Storage<Self> }
/// # #[salsa::db]
/// # impl salsa::Database for Database {}
/// # #[salsa::db]
/// # impl Db for Database {
/// #     fn parse(&self, _: File) -> &ParsedFile { unreachable!() }
/// #     fn resolve(&self, _: database::Package, _: File) -> &database::Resolution { unreachable!() }
/// # }
/// let database = Database::default();
/// let source = SourceFile::new("fluids/heat_2d.byn", "")?;
/// let file = File::new(&database, FileId::from_index(0), source);
/// assert_eq!(
///     module_name(&database, file).as_ref().ok(),
///     ModuleName::from_dotted("fluids.heat_2d").as_ref(),
/// );
///
/// let source = SourceFile::new("heat-2d.byn", "")?;
/// let file = File::new(&database, FileId::from_index(1), source);
/// assert!(module_name(&database, file).is_err());
/// # Ok::<(), source::SourceError>(())
/// ```
///
/// # Errors
///
/// Fails when the name does not end in `.byn`, or when a directory or the file's stem is not a
/// valid module name: not a Python name, or a Python keyword.
#[salsa::tracked(returns(ref))]
pub fn module_name(db: &dyn Db, file: File) -> Result<ModuleName, Diagnostic> {
    let path = file.source(db).name();
    let Some(stem) = path.strip_suffix(EXTENSION) else {
        return Err(Diagnostic::error(format!(
            "`{path}` is not a source file: source files end in `{EXTENSION}`"
        )));
    };
    let components: Vec<String> = stem.split('/').map(str::to_owned).collect();
    match components
        .iter()
        .find(|component| !is_module_name(component))
    {
        None => Ok(ModuleName { components }),
        Some(bad) if KEYWORDS.contains(&bad.as_str()) => Err(Diagnostic::error(format!(
            "`{path}` cannot be imported: `{bad}` is a Python keyword"
        ))
        .with_note("rename the file or directory")),
        Some(bad) => Err(Diagnostic::error(format!(
            "`{path}` cannot be imported: `{bad}` is not a valid module name"
        ))
        .with_note(
            "module names follow Python's rules for names: letters, digits and underscores, \
             not starting with a digit",
        )),
    }
}

/// Every module of a package, by name, and the problems found building the map.
///
/// Built by [`modules()`]. Name resolution reads it to find the file an `import` refers to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Modules {
    /// Each importable module and its file.
    by_name: BTreeMap<ModuleName, File>,
    /// One error per file that names no module or clashes with another, in file order.
    diagnostics: Vec<Diagnostic>,
}

impl Modules {
    /// The file of the module called `name`, if the package has one.
    #[must_use]
    pub fn get(&self, name: &ModuleName) -> Option<File> {
        self.by_name.get(name).copied()
    }

    /// Every module and its file, sorted by name.
    pub fn iter(&self) -> impl Iterator<Item = (&ModuleName, File)> {
        self.by_name.iter().map(|(name, &file)| (name, file))
    }

    /// One error per file that names no module or clashes with another, in file order. Such a
    /// file is left out of the map.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

/// Maps every module name in `package` to its file. Cached until the file list or a file's
/// source changes; editing a file's text reruns it, but the result compares equal, so nothing
/// that reads the map runs again.
///
/// A file is left out, with an error, when its path names no module ([`module_name()`]), when
/// an earlier file already has its name, or when its module and a package share a name, such
/// as `physics.byn` next to a `physics/` directory: `import physics` would be ambiguous.
///
/// Cost: one pass over the package's files, `O(n log n)` in the number of files.
#[salsa::tracked(returns(ref))]
pub fn modules(db: &dyn Db, package: Package) -> Modules {
    let mut by_name = BTreeMap::new();
    let mut diagnostics = Vec::new();
    let mut named = Vec::new();
    for &file in package.files(db) {
        match module_name(db, file) {
            Ok(name) => named.push((name.clone(), file)),
            Err(diagnostic) => diagnostics.push(diagnostic.clone()),
        }
    }
    let packages: Vec<&[String]> = named
        .iter()
        .flat_map(|(name, _)| (1..name.components.len()).map(|length| &name.components[..length]))
        .collect();
    for (name, file) in &named {
        let path = file.source(db).name();
        if packages.contains(&name.components.as_slice()) {
            diagnostics.push(
                Diagnostic::error(format!(
                    "module `{name}` in `{path}` has the same name as the package `{name}`"
                ))
                .with_note("`import` could not tell them apart; rename the file or directory"),
            );
        } else if let Some(&first) = by_name.get(name) {
            let first: File = first;
            diagnostics.push(Diagnostic::error(format!(
                "`{path}` and `{}` both define the module `{name}`",
                first.source(db).name()
            )));
        } else {
            by_name.insert(name.clone(), *file);
        }
    }
    Modules {
        by_name,
        diagnostics,
    }
}

/// Whether `name` can be a module name: a Python name that is not a keyword.
///
/// Python names start with a letter or underscore and continue with letters, digits and
/// underscores, where "letter" includes non-ASCII letters such as `α`. This follows Python's
/// rule closely enough for file names; it does not apply Python's NFKC normalization.
fn is_module_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first == '_' || first.is_alphabetic())
        && characters.all(|character| character == '_' || character.is_alphanumeric())
        && !KEYWORDS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ParsedFile;
    use diagnostics::render;
    use salsa::Setter;
    use source::{FileId, SourceFile, SourceMap};

    /// A database for these tests. Module queries never parse, so `parse` is never called.
    #[salsa::db]
    #[derive(Clone, Default)]
    struct Database {
        /// Salsa's caches and inputs.
        storage: salsa::Storage<Self>,
    }

    #[salsa::db]
    impl salsa::Database for Database {}

    #[salsa::db]
    impl Db for Database {
        fn parse(&self, _: File) -> &ParsedFile {
            unreachable!("module queries never parse")
        }

        fn resolve(&self, _: Package, _: File) -> &crate::Resolution {
            unreachable!("module queries never resolve names")
        }
    }

    /// A package of empty files with the given paths.
    fn package(database: &Database, paths: &[&str]) -> Package {
        let files = paths
            .iter()
            .zip(0..)
            .map(|(path, index)| {
                let source = SourceFile::new(*path, "").unwrap();
                File::new(database, FileId::from_index(index), source)
            })
            .collect();
        Package::new(database, files)
    }

    /// Renders diagnostics without source lines; module diagnostics point at no span.
    fn rendered(diagnostics: &[Diagnostic]) -> Vec<String> {
        let sources = SourceMap::new();
        diagnostics
            .iter()
            .map(|diagnostic| render(diagnostic, &sources).to_string())
            .collect()
    }

    fn names(modules: &Modules) -> Vec<String> {
        modules.iter().map(|(name, _)| name.to_string()).collect()
    }

    #[test]
    fn paths_become_dotted_module_names() {
        let database = Database::default();
        let package = package(
            &database,
            &["main.byn", "physics/nbody.byn", "física/α.byn"],
        );
        let modules = modules(&database, package);
        assert!(modules.diagnostics().is_empty());
        assert_eq!(names(modules), ["física.α", "main", "physics.nbody"]);
        let first = package.files(&database)[1];
        let name = ModuleName::from_dotted("physics.nbody").unwrap();
        assert_eq!(modules.get(&name), Some(first));
    }

    #[test]
    fn bad_paths_name_no_module() {
        let database = Database::default();
        let package = package(
            &database,
            &[
                "notes.txt",
                "heat-2d.byn",
                "class/solver.byn",
                "2d.byn",
                ".byn",
            ],
        );
        let modules = modules(&database, package);
        assert!(names(modules).is_empty());
        assert_eq!(
            rendered(modules.diagnostics()),
            [
                "error: `notes.txt` is not a source file: source files end in `.byn`",
                "error: `heat-2d.byn` cannot be imported: `heat-2d` is not a valid module name\n \
                 = note: module names follow Python's rules for names: letters, digits and \
                 underscores, not starting with a digit",
                "error: `class/solver.byn` cannot be imported: `class` is a Python keyword\n \
                 = note: rename the file or directory",
                "error: `2d.byn` cannot be imported: `2d` is not a valid module name\n \
                 = note: module names follow Python's rules for names: letters, digits and \
                 underscores, not starting with a digit",
                "error: `.byn` cannot be imported: `` is not a valid module name\n \
                 = note: module names follow Python's rules for names: letters, digits and \
                 underscores, not starting with a digit",
            ]
        );
    }

    #[test]
    fn a_module_and_a_package_cannot_share_a_name() {
        let database = Database::default();
        let package = package(&database, &["physics.byn", "physics/nbody.byn"]);
        let modules = modules(&database, package);
        assert_eq!(names(modules), ["physics.nbody"]);
        assert_eq!(
            rendered(modules.diagnostics()),
            [
                "error: module `physics` in `physics.byn` has the same name as the package \
                 `physics`\n = note: `import` could not tell them apart; rename the file or \
                 directory"
            ]
        );
    }

    #[test]
    fn the_first_of_two_files_with_one_name_wins() {
        let database = Database::default();
        let package = package(&database, &["main.byn", "main.byn"]);
        let modules = modules(&database, package);
        assert_eq!(names(modules), ["main"]);
        assert_eq!(
            modules.get(&ModuleName::from_dotted("main").unwrap()),
            Some(package.files(&database)[0])
        );
        assert_eq!(
            rendered(modules.diagnostics()),
            ["error: `main.byn` and `main.byn` both define the module `main`"]
        );
    }

    #[test]
    fn renaming_a_file_updates_the_map() {
        let mut database = Database::default();
        let package = package(&database, &["old.byn"]);
        assert_eq!(names(modules(&database, package)), ["old"]);
        let file = package.files(&database)[0];
        file.set_source(&mut database)
            .to(SourceFile::new("new.byn", "").unwrap());
        assert_eq!(names(modules(&database, package)), ["new"]);
    }

    #[test]
    fn dotted_names_reject_empty_and_invalid_components() {
        assert!(ModuleName::from_dotted("").is_none());
        assert!(ModuleName::from_dotted("a.").is_none());
        assert!(ModuleName::from_dotted("a.b-c").is_none());
        assert!(ModuleName::from_dotted("_private.x1").is_some());
    }
}
