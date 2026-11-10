//! The names a module or class body defines, read straight from the syntax tree, for the
//! questions that list or find definitions.
//!
//! The checker keeps an outline of its own, but it is private to the checker; this one is the
//! few lines the editor needs, and reads only the `syntax` interface.

use syntax::{NodeId, Role, SyntaxKind, SyntaxTree};

/// What kind of statement defines a name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    /// `def name(...)`: a function, or a method in a class body.
    Function,
    /// `class Name(...)`.
    Class,
    /// `name: T = value` or `name = value`: a constant or type alias at the top level, a field
    /// or an enum member in a class body.
    Assignment,
}

/// One name a body defines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Defined {
    /// The `Name` node that is defined.
    pub(super) name: NodeId,
    /// The name as written.
    pub(super) text: String,
    /// What defines it.
    pub(super) kind: Kind,
}

/// The names that the statements of `parent`'s body define, in source order: the module's
/// when `parent` is the root, a class's when it is a class definition. Assignments to anything
/// but one plain name, such as `a, b = 1, 2`, define nothing here.
pub(super) fn definitions(tree: &SyntaxTree, text: &str, parent: NodeId) -> Vec<Defined> {
    tree.children_with(parent, Role::Body)
        .filter_map(|statement| {
            let (name, kind) = match tree.kind(statement) {
                SyntaxKind::FunctionDefinition => {
                    (tree.child(statement, Role::Name)?, Kind::Function)
                }
                SyntaxKind::ClassDefinition => (tree.child(statement, Role::Name)?, Kind::Class),
                SyntaxKind::AnnotatedAssign | SyntaxKind::Assign => {
                    let mut targets = tree.children_with(statement, Role::Target);
                    let target = targets.next()?;
                    if targets.next().is_some() || tree.kind(target) != SyntaxKind::Name {
                        return None;
                    }
                    (target, Kind::Assignment)
                }
                _ => return None,
            };
            Some(Defined {
                name,
                text: tree.text(name, text).to_owned(),
                kind,
            })
        })
        .collect()
}

/// The names the imports at the top of a module bind, with the `Name` node each is spelled by:
/// `nb` for `import physics.nbody as nb`, `physics` for `import physics.nbody`, and `d` for
/// `from a import c as d`.
pub(super) fn imports(tree: &SyntaxTree, text: &str) -> Vec<(NodeId, String)> {
    let root = tree.root();
    tree.children_with(root, Role::Body)
        .filter(|&statement| {
            matches!(
                tree.kind(statement),
                SyntaxKind::Import | SyntaxKind::ImportFrom
            )
        })
        .flat_map(|statement| {
            tree.children(statement)
                .filter(|&alias| tree.kind(alias) == SyntaxKind::Alias)
                .filter_map(move |alias| {
                    if let Some(name) = tree.child(alias, Role::Alias) {
                        return Some((name, tree.text(name, text).to_owned()));
                    }
                    let name = tree.child(alias, Role::Name)?;
                    let written = tree.text(name, text);
                    let bound = match tree.kind(statement) {
                        SyntaxKind::Import => written.split('.').next().unwrap_or(written),
                        _ => written,
                    };
                    Some((name, bound.to_owned()))
                })
        })
        .collect()
}

/// The statement at the top level, or in a top-level class's body, that defines `name`.
pub(super) fn statement_of(tree: &SyntaxTree, name: NodeId) -> Option<NodeId> {
    let defines = |statement: NodeId| {
        tree.child(statement, Role::Name) == Some(name)
            || tree
                .children_with(statement, Role::Target)
                .any(|target| target == name)
    };
    tree.children_with(tree.root(), Role::Body)
        .find_map(|statement| {
            if defines(statement) {
                return Some(statement);
            }
            (tree.kind(statement) == SyntaxKind::ClassDefinition)
                .then(|| {
                    tree.children_with(statement, Role::Body)
                        .find(|&member| defines(member))
                })
                .flatten()
        })
}

/// A class definition's header as one line, such as `class Particle(Struct, Copy)`, without
/// its decorators.
pub(super) fn class_header(tree: &SyntaxTree, text: &str, class: NodeId) -> String {
    let start = tree
        .children_with(class, Role::Decorator)
        .last()
        .map_or(tree.node(class).start(), |decorator| {
            tree.node(decorator).end()
        });
    let end = tree
        .children(class)
        .filter(|&child| {
            matches!(
                tree.node(child).role(),
                Role::Name | Role::TypeParameters | Role::Arguments
            )
        })
        .map(|child| tree.node(child).end())
        .max()
        .unwrap_or(start);
    text.get(start as usize..end as usize)
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
