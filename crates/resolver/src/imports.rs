//! Resolving imports: finding the module, package or definition an import names.

use crate::items::{Binding, Import, ImportTarget, module_items};
use database::{Db, Definition, File, ModuleName, Modules, STANDARD_MODULES};
use diagnostics::Diagnostic;
use syntax::{NodeId, SyntaxTree};

/// How many imports one name may pass through before resolution gives up: `from a import x`
/// where `a` itself imports `x` from `b`, and so on. Deep enough for any real chain of
/// re-exports, and it stops a cycle of imports from looping.
const MAXIMUM_CHAIN: usize = 16;

/// Resolves imports against one package's modules.
pub(crate) struct Importer<'a> {
    /// The database, for reading other modules' top-level names.
    db: &'a dyn Db,
    /// The package's module map.
    modules: &'a Modules,
    /// Whether the importing file is a standard library file, which may also import the
    /// library's internal files, such as `artist` and `axis` under `byplot` (decision 0046).
    in_library: bool,
}

impl<'a> Importer<'a> {
    /// An importer for a package with the given modules, for a file that is a standard library
    /// file when `in_library` is set.
    pub(crate) fn new(db: &'a dyn Db, modules: &'a Modules, in_library: bool) -> Self {
        Self {
            db,
            modules,
            in_library,
        }
    }

    /// What `import`, written in `tree`, refers to.
    ///
    /// # Errors
    ///
    /// Fails, with an error pointing into `tree`, when the module or the member does not
    /// exist, the member is private, or the import is relative.
    pub(crate) fn resolve(
        &self,
        tree: &SyntaxTree,
        import: &Import,
    ) -> Result<Definition, Diagnostic> {
        self.resolve_target(&import.target, 0).map_err(|failure| {
            let node = match (&import.target, failure.at) {
                (ImportTarget::Member { module_node, .. }, At::Module) => *module_node,
                (ImportTarget::Member { member_node, .. }, At::Member) => *member_node,
                _ => import.node,
            };
            let mut diagnostic =
                Diagnostic::error(failure.message).with_primary(tree.span(node), failure.label);
            if let Some(text) = failure.note {
                diagnostic = diagnostic.with_note(text);
            }
            diagnostic
        })
    }

    /// What `target` refers to, after passing through `depth` imports already.
    fn resolve_target(&self, target: &ImportTarget, depth: usize) -> Result<Definition, Failure> {
        match target {
            ImportTarget::Module { path, written } => {
                self.module(written, At::Module)?;
                self.module(path, At::Module)
            }
            ImportTarget::Member { module, member, .. } => self.member(module, member, depth),
            ImportTarget::Relative => Err(Failure {
                message: "relative imports are not supported".to_owned(),
                label: "imported relative to this module",
                note: Some(
                    "write the module's full name from the package root, such as \
                     `from physics.nbody import step`"
                        .to_owned(),
                ),
                at: At::Whole,
            }),
        }
    }

    /// The module or package `path` names, a standard library module or one of the program's.
    fn module(&self, path: &str, at: At) -> Result<Definition, Failure> {
        if let Some(module) = standard(path).filter(|module| module.contains('.')) {
            return Ok(Definition::Standard {
                module,
                member: None,
            });
        }
        let mut components = path.split('.');
        if let Some(module) = standard(components.next().unwrap_or_default()) {
            self.unshadowed(module, at)?;
            return if components.next().is_none() {
                Ok(Definition::Standard {
                    module,
                    member: None,
                })
            } else {
                Err(Failure::new(
                    format!("the standard library module `{module}` has no submodules"),
                    "no such module",
                    at,
                ))
            };
        }
        ModuleName::from_dotted(path)
            .filter(|name| self.exists(name))
            .map(Definition::Module)
            .ok_or_else(|| Failure::new(format!("no module named `{path}`"), "no such module", at))
    }

    /// Whether the package has a module or a package called `name`.
    fn exists(&self, name: &ModuleName) -> bool {
        self.modules.get(name).is_some()
            || self.modules.iter().any(|(module, _)| {
                module.components().len() > name.components().len()
                    && module.components().starts_with(name.components())
            })
    }

    /// What `member` of `module` is: a submodule, or a top-level name of the module's file.
    fn member(&self, module: &str, member: &str, depth: usize) -> Result<Definition, Failure> {
        if let Some(standard) = standard(module) {
            self.unshadowed(standard, At::Module)?;
            return Ok(self.standard_member_at(standard, member, depth));
        }
        if self.in_library && self.modules.library(module).is_some() {
            return self.library_at(module, member, depth).ok_or_else(|| {
                Failure::new(
                    format!("module `{module}` has no `{member}`"),
                    "not defined there",
                    At::Member,
                )
            });
        }
        if let Some(submodule) = standard(&format!("{module}.{member}")) {
            return Ok(Definition::Standard {
                module: submodule,
                member: None,
            });
        }
        let Definition::Module(name) = self.module(module, At::Module)? else {
            return Err(Failure::new(
                format!("the standard library module `{module}` has no submodules"),
                "no such module",
                At::Module,
            ));
        };
        if member.starts_with('_') {
            return Err(Failure {
                message: format!("`{member}` is private to module `{module}`"),
                label: "private name",
                note: Some(
                    "a name starting with `_` can only be used in its own module".to_owned(),
                ),
                at: At::Member,
            });
        }
        if let Some(submodule) = ModuleName::from_dotted(&format!("{module}.{member}"))
            .filter(|submodule| self.exists(submodule))
        {
            return Ok(Definition::Module(submodule));
        }
        let missing = || {
            Failure::new(
                format!("module `{module}` has no `{member}`"),
                "not defined there",
                At::Member,
            )
        };
        let file = self.modules.get(&name).ok_or_else(missing)?;
        match module_items(self.db, file)
            .get(member)
            .ok_or_else(missing)?
        {
            Binding::Defined(node) => Ok(Definition::Item(file, *node)),
            Binding::Imported(_) if depth >= MAXIMUM_CHAIN => Err(Failure::new(
                format!("`{member}` is imported in a circle, ending at module `{module}`"),
                "imported in a circle",
                At::Member,
            )),
            Binding::Imported(import) => {
                self.resolve_target(&import.target, depth + 1).map_err(|_| {
                    Failure::new(
                        format!("module `{module}` imports `{member}`, but that import fails"),
                        "its import fails",
                        At::Member,
                    )
                })
            }
        }
    }

    /// What `member` means in `module.member`, where `module` is a module of the program and
    /// `node` is the `Name` node of `member` in `tree`.
    ///
    /// # Errors
    ///
    /// Fails, pointing at `node`, when the module has no such member or it is private.
    pub(crate) fn attribute(
        &self,
        tree: &SyntaxTree,
        module: &ModuleName,
        member: &str,
        node: NodeId,
    ) -> Result<Definition, Diagnostic> {
        self.member(&module.to_string(), member, 0)
            .map_err(|failure| {
                let diagnostic =
                    Diagnostic::error(failure.message).with_primary(tree.span(node), failure.label);
                match failure.note {
                    Some(text) => diagnostic.with_note(text),
                    None => diagnostic,
                }
            })
    }

    /// Checks that the program has no module of its own named as the standard module
    /// `module`, which an import of `module` could never reach: the standard module is found
    /// first, as Python finds a built-in module before a file of the same name.
    ///
    /// # Errors
    ///
    /// Fails, naming the program's file, when it has such a module and the import is the
    /// program's own; the standard library's imports always mean the standard module.
    fn unshadowed(&self, module: &str, at: At) -> Result<(), Failure> {
        if self.in_library {
            return Ok(());
        }
        let Some(file) = ModuleName::from_dotted(module).and_then(|name| self.modules.get(&name))
        else {
            return Ok(());
        };
        Err(Failure {
            message: format!(
                "`{}` has the name of the standard library module `{module}`",
                file.source(self.db).name()
            ),
            label: "imports the standard module",
            note: Some(format!(
                "an import of `{module}` always finds the standard module; rename the file"
            )),
            at,
        })
    }

    /// What `member` of the standard module `module` is: the class or function the standard
    /// library's source file for the module defines (decision 0038), such as `os.ErrorKind`,
    /// or else a member the compiler builds in, such as `os.getenv`.
    pub(crate) fn standard_member(&self, module: &'static str, member: &str) -> Definition {
        self.standard_member_at(module, member, 0)
    }

    /// [`standard_member`](Self::standard_member), after passing through `depth` re-exports
    /// already.
    fn standard_member_at(&self, module: &'static str, member: &str, depth: usize) -> Definition {
        self.library_at(module, member, depth)
            .unwrap_or_else(|| Definition::Standard {
                module,
                member: Some(member.to_owned()),
            })
    }

    /// The definition of `name` in the standard library's source file for `module`, if the
    /// package has that file and it defines or re-exports the name at its top level. A name
    /// starting with `_` is the library's own and is never found.
    ///
    /// A plain import is never found either, so a library file's own imports, such as
    /// `from math import sqrt`, do not become names of its module. Only a re-export is followed:
    /// `from special_gamma import gamma as gamma`, the name imported under its own name with
    /// `as` (decision 0062). That lets one public module, such as `special`, keep its functions
    /// in internal files of one family each, while every name still has one home: the file
    /// that defines it.
    pub(crate) fn library(&self, module: &str, name: &str) -> Option<Definition> {
        self.library_at(module, name, 0)
    }

    /// [`library`](Self::library), after passing through `depth` re-exports already. A chain
    /// longer than [`MAXIMUM_CHAIN`] is not followed, so a circle of re-exports finds nothing.
    fn library_at(&self, module: &str, name: &str, depth: usize) -> Option<Definition> {
        if name.starts_with('_') {
            return None;
        }
        let file = self.modules.library(module)?;
        match module_items(self.db, file).get(name)? {
            Binding::Defined(node) => Some(Definition::Item(file, *node)),
            Binding::Imported(import) if depth < MAXIMUM_CHAIN && is_reexport(import) => {
                let ImportTarget::Member { module, member, .. } = &import.target else {
                    return None;
                };
                Importer::new(self.db, self.modules, true)
                    .member(module, member, depth + 1)
                    .ok()
            }
            Binding::Imported(_) => None,
        }
    }

    /// The definition of a name bound at the top level of `file`, the module being resolved.
    pub(crate) fn top_level(
        &self,
        tree: &SyntaxTree,
        file: File,
        binding: &Binding,
    ) -> Result<Definition, Diagnostic> {
        match binding {
            Binding::Defined(node) => Ok(Definition::Item(file, *node)),
            Binding::Imported(import) => self.resolve(tree, import),
        }
    }
}

/// Whether `import` is a re-export: `from module import name as name`, a member imported
/// under its own name with `as`, the form Python's type checkers read as a re-export (PEP 484).
/// The standard library follows it (decision 0062); any other import binds a name for the file
/// alone.
fn is_reexport(import: &Import) -> bool {
    matches!(
        &import.target,
        ImportTarget::Member { member, member_node, .. }
            if *member_node != import.node && *member == import.name
    )
}

/// The standard library module called `name`, if there is one.
fn standard(name: &str) -> Option<&'static str> {
    STANDARD_MODULES.into_iter().find(|&module| module == name)
}

/// Where in an import an error points.
#[derive(Clone, Copy)]
enum At {
    /// The module path after `from`, or the dotted path after `import`.
    Module,
    /// The imported member's name.
    Member,
    /// The bound name: for an error about the import as a whole.
    Whole,
}

/// Why an import failed, before it is placed in the source.
struct Failure {
    /// The diagnostic's message.
    message: String,
    /// The label under the source text.
    label: &'static str,
    /// An optional note on how to fix it.
    note: Option<String>,
    /// Which part of the import the error points at.
    at: At,
}

impl Failure {
    /// A failure with no note.
    fn new(message: String, label: &'static str, at: At) -> Self {
        Self {
            message,
            label,
            note: None,
            at,
        }
    }
}
