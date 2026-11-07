//! The standard traits that bound a type parameter (language reference, "Types", "Traits"):
//! `Number`, `Float`, `VectorSpace` and `Copy`. How user code declares its own traits is
//! roadmap decision D.9; until then these four are the only bounds.
//!
//! A bound is read from the syntax tree, `T: Number` in `def f[T: Number](x: T)`, rather than
//! stored in the type, so a `Type` stays a small interned value and declarations need no
//! interface change.

use crate::outline::outline;
use database::File;
use syntax::{NodeId, Role, SyntaxKind};

/// A trait a type parameter may be bounded by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Trait {
    /// `Number`: every number type.
    Number,
    /// `Float`: `f32` and `f64`.
    Float,
    /// `VectorSpace`: `f64` and arrays of `f64`.
    VectorSpace,
    /// `Copy`: the copy types.
    Copy,
}

impl Trait {
    /// The trait a builtin name means, if it is one.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "Number" => Self::Number,
            "Float" => Self::Float,
            "VectorSpace" => Self::VectorSpace,
            "Copy" => Self::Copy,
            _ => return None,
        })
    }

    /// The trait's name as a program writes it.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Number => "Number",
            Self::Float => "Float",
            Self::VectorSpace => "VectorSpace",
            Self::Copy => "Copy",
        }
    }

    /// Whether the trait allows arithmetic and ordering with number literals.
    pub(crate) fn is_numeric(self) -> bool {
        matches!(self, Self::Number | Self::Float)
    }
}

/// The bound of the type parameter whose `Name` node is `name` in `file`: the trait after `:`
/// in `def f[T: Number]` or `class Pair[T: Number]`.
pub(crate) fn bound(db: &dyn types::TypeDb, file: File, name: NodeId) -> Option<Trait> {
    let tree = db.parse(file).tree();
    let text = file.source(db).text();
    outline(db, file).entries().iter().find_map(|entry| {
        let parameters = tree.child(entry.statement, Role::TypeParameters)?;
        let variable = tree
            .children_with(parameters, Role::Item)
            .find(|&variable| tree.child(variable, Role::Name) == Some(name))?;
        let bound = tree.child(variable, Role::Bound)?;
        (tree.kind(bound) == SyntaxKind::Name)
            .then(|| Trait::from_name(tree.text(bound, text)))
            .flatten()
    })
}
