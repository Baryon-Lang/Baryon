//! A file's outline: every definition the type checker gives a declaration, and what kind of
//! definition each one is, read from the syntax alone.
//!
//! Lowering an annotation that names a class needs to know whether the class is a struct or an
//! enum and how many type parameters it takes. Reading that from the class's syntax, rather than
//! from its full declaration, means a struct whose field names another struct never waits on
//! that struct's declaration, so structs that refer to each other are not a cycle.

use database::{Db, File};
use syntax::{NodeId, Operator, Role, SyntaxKind, SyntaxTree};

/// Every definition in one file that has a declaration: top-level functions, classes, type
/// aliases and constants, and the methods of each class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Outline {
    /// One entry per definition, in source order, methods right after their class.
    entries: Vec<Entry>,
}

impl Outline {
    /// The definition whose `Name` node is `name`, if there is one.
    ///
    /// Cost: a linear search over the file's definitions, which number in the tens.
    pub(crate) fn get(&self, name: NodeId) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    /// Every definition, in source order.
    pub(crate) fn entries(&self) -> &[Entry] {
        &self.entries
    }
}

/// One definition in a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    /// The definition's `Name` node: the name after `def` or `class`, or the assigned name.
    pub(crate) name: NodeId,
    /// The statement that defines it.
    pub(crate) statement: NodeId,
    /// What kind of definition it is.
    pub(crate) kind: Kind,
    /// For a method, the `Name` node of its class.
    pub(crate) class: Option<NodeId>,
}

/// What kind of definition an [`Entry`] is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A function or method.
    Function,
    /// A class.
    Class(Class),
    /// `Name = type` at the top level.
    Alias,
    /// `Name: Final = value` at the top level.
    Constant,
    /// A `type` statement, or `Name = value` whose value cannot be a type, such as `N = 10`:
    /// both are reported by the syntax check, so the name is left unknown.
    Unsupported,
}

/// What a class is, from its bases and type parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Class {
    /// Whether it is a struct, an error type, an enum or a trait.
    pub(crate) base: Base,
    /// Whether `Copy` is among its bases.
    pub(crate) copy: bool,
    /// Whether it is decorated `@layout("C")`, which pins its field order and silences the
    /// padding warning (decision 0017). The syntax check rejects any other `@layout`.
    pub(crate) c_layout: bool,
    /// The `Name` node of each type parameter, in order.
    pub(crate) type_parameters: Vec<NodeId>,
}

/// The base that says what a class is. Which combinations are allowed is the syntax check's
/// job; a class with no recognized base counts as a struct here, so it reports nothing twice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Base {
    /// `Struct`, or no recognized base.
    Struct,
    /// `Exception`: an error type, a struct a fallible function can fail with.
    Error,
    /// `Enum`.
    Enum,
    /// `Protocol`: a trait (decision 0031).
    Protocol,
}

/// The outline of `file`. Cached until the file's tree changes.
#[salsa::tracked(returns(ref))]
pub(crate) fn outline(db: &dyn Db, file: File) -> Outline {
    let tree = db.parse(file).tree();
    let text = file.source(db).text();
    let mut entries = Vec::new();
    for statement in tree.children_with(tree.root(), Role::Body) {
        let kind = match tree.kind(statement) {
            SyntaxKind::FunctionDefinition => Kind::Function,
            SyntaxKind::ClassDefinition => Kind::Class(class(tree, text, statement)),
            SyntaxKind::Assign if typelike(tree, statement) => Kind::Alias,
            SyntaxKind::AnnotatedAssign => Kind::Constant,
            SyntaxKind::Assign | SyntaxKind::TypeAlias => Kind::Unsupported,
            _ => continue,
        };
        let role = match kind {
            Kind::Alias | Kind::Constant => Role::Target,
            _ => Role::Name,
        };
        let Some(name) = tree
            .child(statement, role)
            .filter(|&name| tree.kind(name) == SyntaxKind::Name)
        else {
            continue;
        };
        let is_class = matches!(kind, Kind::Class(_));
        entries.push(Entry {
            name,
            statement,
            kind,
            class: None,
        });
        if is_class {
            entries.extend(methods(tree, statement, name));
        }
    }
    Outline { entries }
}

/// What a class is: its base, whether it is `Copy` or `@layout("C")`, and its type parameters.
fn class(tree: &SyntaxTree, text: &str, statement: NodeId) -> Class {
    let bases: Vec<&str> = tree
        .child(statement, Role::Arguments)
        .into_iter()
        .flat_map(|arguments| tree.children_with(arguments, Role::Item))
        .filter(|&base| tree.kind(base) == SyntaxKind::Name)
        .map(|base| tree.text(base, text))
        .collect();
    let base = if bases.contains(&"Enum") {
        Base::Enum
    } else if bases.contains(&"Exception") {
        Base::Error
    } else if bases.contains(&"Protocol") {
        Base::Protocol
    } else {
        Base::Struct
    };
    let c_layout = tree
        .children_with(statement, Role::Decorator)
        .any(|decorator| {
            tree.child(decorator, Role::Value)
                .filter(|&value| tree.kind(value) == SyntaxKind::Call)
                .and_then(|call| tree.child(call, Role::Function))
                .is_some_and(|function| {
                    tree.kind(function) == SyntaxKind::Name && tree.text(function, text) == "layout"
                })
        });
    Class {
        base,
        copy: bases.contains(&"Copy"),
        c_layout,
        type_parameters: type_parameters(tree, statement),
    }
}

/// The `Name` node of each type parameter of a definition, in order.
pub(crate) fn type_parameters(tree: &SyntaxTree, definition: NodeId) -> Vec<NodeId> {
    tree.child(definition, Role::TypeParameters)
        .into_iter()
        .flat_map(|parameters| tree.children_with(parameters, Role::Item))
        .filter_map(|parameter| tree.child(parameter, Role::Name))
        .collect()
}

/// The methods of a class, in source order.
fn methods(tree: &SyntaxTree, class: NodeId, class_name: NodeId) -> Vec<Entry> {
    tree.children_with(class, Role::Body)
        .filter(|&statement| tree.kind(statement) == SyntaxKind::FunctionDefinition)
        .filter_map(|statement| {
            Some(Entry {
                name: tree.child(statement, Role::Name)?,
                statement,
                kind: Kind::Function,
                class: Some(class_name),
            })
        })
        .collect()
}

/// Whether the value of the top-level assignment `statement` is shaped like a type: a name,
/// `a.b`, `T[...]`, `None` or `A | B`. The syntax check reports any other value as a
/// module-level variable, which needs `Final`.
fn typelike(tree: &SyntaxTree, statement: NodeId) -> bool {
    tree.child(statement, Role::Value)
        .is_some_and(|value| match tree.kind(value) {
            SyntaxKind::Name | SyntaxKind::Attribute | SyntaxKind::Subscript | SyntaxKind::None => {
                true
            }
            SyntaxKind::BinaryOperation => tree.node(value).operator() == Operator::BitOr,
            _ => false,
        })
}
