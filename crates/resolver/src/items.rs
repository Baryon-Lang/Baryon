//! A module's top-level names: what it defines and what it imports, before any import is
//! resolved. Other modules read this when they import from it.

use database::{Db, File};
use diagnostics::Diagnostic;
use std::collections::BTreeMap;
use syntax::{NodeId, Role, SyntaxKind, SyntaxTree};

/// Every name bound at the top level of one module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ModuleItems {
    /// Each top-level name and what binds it. When a name is bound twice, the first binding.
    names: BTreeMap<String, Binding>,
    /// Module imports that bind a name an earlier module import already binds to the same
    /// module, as `import physics.nbody` then `import physics.forces` both bind `physics`. They
    /// are not clashes, but each path written must still exist.
    repeated: Vec<Import>,
    /// One error per name bound twice, in source order.
    diagnostics: Vec<Diagnostic>,
}

impl ModuleItems {
    /// What binds `name` at the top level, if anything.
    pub(crate) fn get(&self, name: &str) -> Option<&Binding> {
        self.names.get(name)
    }

    /// Every top-level name and what binds it, sorted by name.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&str, &Binding)> {
        self.names
            .iter()
            .map(|(name, binding)| (name.as_str(), binding))
    }

    /// Module imports that bind the same name to the same module as an earlier one.
    pub(crate) fn repeated(&self) -> &[Import] {
        &self.repeated
    }

    /// One error per name bound twice, in source order.
    pub(crate) fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

/// What binds one top-level name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Binding {
    /// A function, class, constant or type alias; the id is its `Name` node.
    Defined(NodeId),
    /// An import, not yet resolved.
    Imported(Import),
}

/// One name bound by an import statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Import {
    /// The name the import binds: `c` in `import a.b as c`, `a` in `import a.b`.
    pub(crate) name: String,
    /// The node that binds the name: the alias after `as` when there is one, otherwise the
    /// imported `Name` leaf, which may be a whole dotted path.
    pub(crate) node: NodeId,
    /// What the import refers to.
    pub(crate) target: ImportTarget,
}

/// What an import refers to, as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ImportTarget {
    /// `import a.b` binds `a`, and `import a.b as c` binds `c`; both name the module `path`.
    Module {
        /// The dotted path the name refers to: `a` for `import a.b`, `a.b` for `import a.b as c`.
        path: String,
        /// The whole dotted path written, which must exist: `a.b` in both examples.
        written: String,
    },
    /// `from module import member`.
    Member {
        /// The dotted module path after `from`.
        module: String,
        /// The `Name` node of that path, where an error about the module points.
        module_node: NodeId,
        /// The imported name.
        member: String,
        /// The `Name` node of the imported name, where an error about it points.
        member_node: NodeId,
    },
    /// `from . import x`: a relative import, which is not supported.
    Relative,
}

/// The top-level names of `file`. Cached until the file's tree changes.
#[salsa::tracked(returns(ref))]
pub(crate) fn module_items(db: &dyn Db, file: File) -> ModuleItems {
    let tree = db.parse(file).tree();
    let text = file.source(db).text();
    let mut items = ModuleItems {
        names: BTreeMap::new(),
        repeated: Vec::new(),
        diagnostics: Vec::new(),
    };
    for statement in tree.children(tree.root()) {
        for (name, binding) in top_level_bindings(tree, text, statement) {
            let node = binding_node(&binding);
            if let Some(first) = items.names.get(&name) {
                if let (Binding::Imported(again), Binding::Imported(earlier)) = (&binding, first)
                    && same_module(&again.target, &earlier.target)
                {
                    items.repeated.push(again.clone());
                    continue;
                }
                let first = binding_node(first);
                items.diagnostics.push(
                    Diagnostic::error(format!("`{name}` is defined twice in this module"))
                        .with_primary(tree.span(node), "defined again here")
                        .with_secondary(tree.span(first), "first defined here")
                        .with_note("one name means one thing in a module; rename one of them"),
                );
            } else {
                items.names.insert(name, binding);
            }
        }
    }
    items
}

/// Whether two imports bind their name to the same module: `import a.b` and `import a.c` both
/// bind `a` to the package `a`.
fn same_module(first: &ImportTarget, second: &ImportTarget) -> bool {
    matches!(
        (first, second),
        (ImportTarget::Module { path: first, .. }, ImportTarget::Module { path: second, .. })
            if first == second
    )
}

/// The node that binds a name: a definition's name, or an import's bound name.
fn binding_node(binding: &Binding) -> NodeId {
    match binding {
        Binding::Defined(node) => *node,
        Binding::Imported(import) => import.node,
    }
}

/// The names one top-level statement binds, in source order.
fn top_level_bindings(tree: &SyntaxTree, text: &str, statement: NodeId) -> Vec<(String, Binding)> {
    let name_of = |node: NodeId| tree.text(node, text).to_owned();
    let defined = |node: NodeId| (name_of(node), Binding::Defined(node));
    match tree.kind(statement) {
        SyntaxKind::FunctionDefinition | SyntaxKind::ClassDefinition | SyntaxKind::TypeAlias => {
            tree.child(statement, Role::Name)
                .map(defined)
                .into_iter()
                .collect()
        }
        SyntaxKind::AnnotatedAssign | SyntaxKind::Assign => tree
            .children_with(statement, Role::Target)
            .filter(|&target| tree.kind(target) == SyntaxKind::Name)
            .map(defined)
            .collect(),
        SyntaxKind::Import | SyntaxKind::ImportFrom => imports(tree, text, statement)
            .into_iter()
            .map(|import| (import.name.clone(), Binding::Imported(import)))
            .collect(),
        _ => Vec::new(),
    }
}

/// The names an `import` or `from ... import` statement binds, in source order. Used for
/// imports at the top level and inside functions alike.
pub(crate) fn imports(tree: &SyntaxTree, text: &str, statement: NodeId) -> Vec<Import> {
    let from = (tree.kind(statement) == SyntaxKind::ImportFrom).then(|| {
        tree.child(statement, Role::Module)
            .filter(|_| !is_relative(tree, text, statement))
            .map(|module| (tree.text(module, text).to_owned(), module))
    });
    tree.children_with(statement, Role::Item)
        .filter_map(|alias| {
            let name = tree.child(alias, Role::Name)?;
            let written = tree.text(name, text).to_owned();
            let renamed = tree.child(alias, Role::Alias);
            let (node, target) = match &from {
                Some(Some((module, module_node))) => (
                    renamed.unwrap_or(name),
                    ImportTarget::Member {
                        module: module.clone(),
                        module_node: *module_node,
                        member: written,
                        member_node: name,
                    },
                ),
                Some(None) => (renamed.unwrap_or(name), ImportTarget::Relative),
                None => match renamed {
                    Some(renamed) => (
                        renamed,
                        ImportTarget::Module {
                            path: written.clone(),
                            written,
                        },
                    ),
                    None => (
                        name,
                        ImportTarget::Module {
                            path: written.split('.').next().unwrap_or_default().to_owned(),
                            written,
                        },
                    ),
                },
            };
            let bound = match &target {
                ImportTarget::Module { path, .. } if renamed.is_none() => path.clone(),
                _ => tree.text(node, text).to_owned(),
            };
            Some(Import {
                name: bound,
                node,
                target,
            })
        })
        .collect()
}

/// Whether a `from` import is relative: `from .`, `from ..physics`. The dots are not nodes, so
/// they are read from the text between `from` and the module or `import`.
fn is_relative(tree: &SyntaxTree, text: &str, statement: NodeId) -> bool {
    let span = tree.span(statement);
    text[span.start() as usize..span.end() as usize]
        .strip_prefix("from")
        .is_some_and(|rest| rest.trim_start().starts_with('.'))
}
