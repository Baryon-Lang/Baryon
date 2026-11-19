//! Variadic type parameters (the decision 0026 amendment): `*Ts` declared in a function's type
//! parameters and unpacked as the last element of a tuple, `tuple[*Ts]`, or of a `Callable`'s
//! parameter list, `Callable[[f64, S, *Ts], S]`.
//!
//! `Ts` stands for a list of types of any length, zero included. A function has at most one,
//! and it appears only in those two places, as [`TypeKind::Unpacked`]. A call binds it from the
//! tuple passed for `args`, or from the `Callable` passed when `args` is left out, and
//! substitution splices the types in ([`types::spliced`]).

use crate::lower::Lowerer;
use database::{Db, File};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Type, TypeKind};

/// Whether the type parameter whose `Name` node is `name`, in `file`, was declared `*Ts`.
/// A `TypeVariableTuple`'s name is its first child, so it sits right after it in preorder.
pub(crate) fn is_pack(db: &dyn Db, file: File, name: NodeId) -> bool {
    let tree = db.parse(file).tree();
    name.index()
        .checked_sub(1)
        .and_then(|index| tree.nodes().get(index))
        .is_some_and(|node| node.kind() == SyntaxKind::TypeVariableTuple)
}

/// The type parameter nodes of `definition` declared `*Ts`, in order.
pub(crate) fn packs(db: &dyn Db, file: File, definition: NodeId) -> Vec<NodeId> {
    let tree = db.parse(file).tree();
    tree.child(definition, Role::TypeParameters)
        .into_iter()
        .flat_map(|parameters| tree.children_with(parameters, Role::Item))
        .filter(|&item| tree.kind(item) == SyntaxKind::TypeVariableTuple)
        .collect()
}

impl<'db> Lowerer<'db> {
    /// The types of `items`, the elements of a tuple type or a `Callable`'s parameters, where
    /// the last may be `*Ts`.
    pub(crate) fn items(&mut self, items: &[NodeId]) -> Vec<Type<'db>> {
        let last = items.len().saturating_sub(1);
        items
            .iter()
            .enumerate()
            .map(|(position, &item)| {
                if self.tree.kind(item) != SyntaxKind::Starred {
                    return self.lower(item);
                }
                if position != last {
                    return self.error(
                        item,
                        "`*Ts` comes last",
                        "not the last type",
                        Some(
                            "a variadic type parameter stands for the types left at the end: \
                              `Callable[[f64, *Ts], f64]`",
                        ),
                    );
                }
                self.unpacked(item)
            })
            .collect()
    }

    /// `*Ts` at `starred`, where `Ts` must be a variadic type parameter.
    fn unpacked(&mut self, starred: NodeId) -> Type<'db> {
        let Some(value) = self.tree.child(starred, Role::Value) else {
            return self.unknown();
        };
        let pack = if self.tree.kind(value) == SyntaxKind::Name {
            self.named(value, value, None)
        } else {
            self.lower(value)
        };
        match pack.kind(self.db) {
            TypeKind::Parameter { file, name } if is_pack(self.db, *file, *name) => {
                Type::new(self.db, TypeKind::Unpacked(pack))
            }
            TypeKind::Unknown => pack,
            _ => {
                let message = format!(
                    "`*{}` unpacks only a variadic type parameter",
                    self.tree.text(value, self.text)
                );
                self.error(
                    starred,
                    &message,
                    "not declared `*Ts`",
                    Some(
                        "declare it among the type parameters, `def solve[*Ts](...)`, then \
                          write `tuple[*Ts]` or `Callable[[f64, *Ts], f64]`",
                    ),
                )
            }
        }
    }

    /// `ty`, the type the name `node` gives, unless it is a variadic type parameter written
    /// without its star: `Ts` is a list of types, not one.
    pub(crate) fn bare_pack(&mut self, node: NodeId, ty: Type<'db>) -> Type<'db> {
        match ty.kind(self.db) {
            TypeKind::Parameter { file, name } if is_pack(self.db, *file, *name) => {
                let written = self.tree.text(node, self.text);
                let message = format!("`{written}` stands for a list of types, not one type");
                let hint = format!(
                    "unpack it at the end of a tuple or a `Callable`'s parameters: \
                     `tuple[*{written}]` or `Callable[[f64, *{written}], f64]`"
                );
                self.error(node, &message, "a variadic type parameter", Some(&hint))
            }
            _ => ty,
        }
    }
}
