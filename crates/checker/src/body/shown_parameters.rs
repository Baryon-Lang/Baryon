//! A value shown through a type parameter (roadmap 1.9.67): a generic function that prints or
//! formats its `T` called with a frame, an array with missing values, a text array or a
//! `Path`, or a value that holds one.
//!
//! Lowering builds a generic function's body once, where the value's type is `T`, so it cannot
//! choose the library calls that write those four kinds as text; by monomorphization the type
//! argument has become the parts the compiler stores them in, and the program would print
//! `([1. 0.], [1])` for `[1.0 None]`. Until the text is built through a type parameter (idea
//! I.71), the call is an error, as showing such a value inside a list already is.
//!
//! The test of the callee is syntactic, so reading its body never needs its checked body, which
//! a recursive call would need while it is being checked: a value shown is one whose
//! expression, a name with any attributes or indices after it, starts at a parameter whose
//! declared type names the type parameter, given to `print` or interpolated into an f-string.
//! A value that reaches them another way, through a local variable or another generic call,
//! is not seen.
//!
//! ```text
//! error: `say` shows its `T`, a frame here, which cannot be shown through a type parameter yet
//! ```

use super::Body;
use super::relations;
use crate::outline::outline;
use database::Definition;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Callee, Item, Signature, Type, TypeDb};

impl<'db> Body<'db> {
    /// Reports the call at `node` of `target`, named `callee`, when one of `type_arguments`,
    /// the types the call gives the type parameters of `signature`, is or holds a kind of
    /// value whose text needs the library and `target` shows a value of that type parameter.
    pub(super) fn shown_through_parameters(
        &mut self,
        node: NodeId,
        callee: &str,
        target: &Callee<'db>,
        signature: &Signature<'db>,
        type_arguments: &[Type<'db>],
    ) {
        let (Callee::Function(item) | Callee::Method(item)) = *target else {
            return;
        };
        for (&parameter, &argument) in signature.type_parameters.iter().zip(type_arguments) {
            let Some(kind) = self.shown_kind(argument) else {
                continue;
            };
            let shown = signature
                .parameters
                .iter()
                .filter(|declared| relations::mentions(self.db, declared.ty, parameter))
                .map(|declared| declared.name.clone())
                .collect::<Vec<_>>();
            if shown.is_empty() || !shows_parameters(self.db, item, &shown) {
                continue;
            }
            let message = format!(
                "`{callee}` shows its `{}`, {kind} here, which cannot be shown through a type \
                 parameter yet",
                self.show(parameter)
            );
            self.lowerer.error(
                node,
                &message,
                "shown through a type parameter",
                Some(
                    "show the value outside the generic function, `print(value)`; showing it \
                     through `T` is idea I.71",
                ),
            );
        }
    }

    /// What `ty` is or holds whose text the library writes, which a type parameter cannot show
    /// yet: `None` for every other type.
    fn shown_kind(&self, ty: Type<'db>) -> Option<&'static str> {
        if relations::holds_frame(self.db, ty) {
            Some("a frame")
        } else if relations::holds_optional_array(self.db, ty) {
            Some("an array with missing values")
        } else if relations::holds_text_array(self.db, ty) {
            Some("a text array")
        } else if self.holds_path(ty, 0) {
            Some("a `Path`")
        } else {
            None
        }
    }
}

/// Whether the body of the function `item` gives `print`, or an f-string, an expression that
/// starts at one of its parameters named in `parameters`.
fn shows_parameters(db: &dyn TypeDb, item: Item<'_>, parameters: &[String]) -> bool {
    let (package, file) = (item.package(db), item.file(db));
    let Some(entry) = outline(db, file).get(item.name(db)) else {
        return false;
    };
    let tree = db.parse(file).tree();
    let text = file.source(db).text();
    let resolution = db.resolve(package, file);
    let starts_at_parameter = |mut expression: NodeId| {
        while matches!(
            tree.kind(expression),
            SyntaxKind::Attribute | SyntaxKind::Subscript
        ) {
            let Some(value) = tree.child(expression, Role::Value) else {
                return false;
            };
            expression = value;
        }
        tree.kind(expression) == SyntaxKind::Name
            && matches!(
                resolution.definition(expression),
                Some(Definition::Local(_))
            )
            && parameters
                .iter()
                .any(|name| name == tree.text(expression, text))
    };
    let mut stack = vec![entry.statement];
    while let Some(node) = stack.pop() {
        let shown: Vec<NodeId> = match tree.kind(node) {
            SyntaxKind::Interpolation => tree.child(node, Role::Value).into_iter().collect(),
            SyntaxKind::Call
                if tree.child(node, Role::Function).is_some_and(|function| {
                    matches!(
                        resolution.definition(function),
                        Some(Definition::Builtin("print"))
                    )
                }) =>
            {
                tree.child(node, Role::Arguments)
                    .into_iter()
                    .flat_map(|list| tree.children_with(list, Role::Item))
                    .filter(|&item| tree.kind(item) != SyntaxKind::Keyword)
                    .collect()
            }
            _ => Vec::new(),
        };
        if shown.into_iter().any(starts_at_parameter) {
            return true;
        }
        stack.extend(tree.children(node));
    }
    false
}
