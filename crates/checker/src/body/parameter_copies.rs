//! `.copy()` through a type parameter (roadmap 1.9.60, decision 0066, cards 51 and 70): a
//! generic function may call `.copy()` on a value of type `T`, and a call that fixes `T` to a
//! type the compiler cannot copy there is an error at that call.
//!
//! `T` may be fixed to a type with a copy that a generic function builds: a copy type, which is
//! copied as it is, a `str`, a list of copy types or `str`s, an array of copy types, or a struct
//! that defines its own `copy` method. A type with a copy that is not built through a type
//! parameter yet, such as a `dict`, a frame, a text array, an array with missing values, a
//! `Path` or a list of lists, is an error that says so; building those is idea I.76. Any other
//! type has no copy, such as a tuple, `T | None` or a struct with no `copy` of its own, and the
//! error says that.
//!
//! The test of the callee is syntactic, as the test of a value shown through a type parameter
//! is (roadmap 1.9.67), so reading its body never needs its checked body, which a recursive
//! call would need while it is being checked: a copy is a `.copy()` with no arguments on an
//! expression that starts at a parameter and steps into it only by indexing, whose type,
//! followed through each index from the parameter's declared type, is the type parameter. A
//! value that reaches `.copy()` another way, through a local variable or another generic call,
//! is not seen here; the backend reports its type when it cannot copy it.
//!
//! ```text
//! error: `last` calls `.copy()` on `T`, and `tuple[list[f64], str]` has no copy
//! ```

use super::Body;
use super::calls::Arguments;
use super::exclusive::Passed;
use super::methods::copy_signature;
use crate::embed;
use crate::forms::is_copy;
use crate::outline::outline;
use database::Definition;
use syntax::{Flags, NodeId, Role, SyntaxKind};
use types::{Callee, Item, Signature, Type, TypeDb, TypeKind};

/// Whether a type has a copy that a type parameter can be fixed to.
enum Copyable {
    /// It has one, built through a type parameter.
    Built,
    /// It has one, not built through a type parameter yet.
    NotYet,
    /// It has none.
    Missing,
}

impl<'db> Body<'db> {
    /// `value.copy()` where the type of `value`, the receiver, is the type parameter `base`: a
    /// call of the built-in `parameter.copy`, which borrows the value and returns a new one of
    /// the same type.
    pub(super) fn parameter_copy(
        &mut self,
        node: NodeId,
        base: Type<'db>,
        receiver: Passed<'db>,
        arguments: &Arguments,
    ) -> Type<'db> {
        let builtin = Callee::Builtin("parameter.copy".to_owned());
        self.check_call(
            node,
            "copy",
            &copy_signature(base),
            Some(receiver),
            builtin,
            arguments,
        )
    }

    /// Reports the call at `node` of `target`, named `callee`, when one of `type_arguments`,
    /// the types the call gives the type parameters of `signature`, is a type that `target`
    /// cannot show or copy through its type parameter (roadmap 1.9.67 and 1.9.60).
    pub(super) fn through_parameters(
        &mut self,
        node: NodeId,
        callee: &str,
        target: &Callee<'db>,
        signature: &Signature<'db>,
        type_arguments: &[Type<'db>],
    ) {
        self.shown_through_parameters(node, callee, target, signature, type_arguments);
        self.copied_through_parameters(node, callee, target, signature, type_arguments);
    }

    /// Reports the call at `node` of `target`, named `callee`, when one of `type_arguments`,
    /// the types the call gives the type parameters of `signature`, is a type the compiler
    /// cannot copy through a type parameter and `target` calls `.copy()` on a value of that
    /// type parameter.
    fn copied_through_parameters(
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
            let copyable = if self.is_path(argument) {
                Copyable::NotYet
            } else {
                copyable(self.db, self.lowerer.package, argument)
            };
            if matches!(copyable, Copyable::Built) || !copies(self.db, item, signature, parameter) {
                continue;
            }
            let (parameter, argument) = (self.show(parameter), self.show(argument));
            if matches!(copyable, Copyable::NotYet) {
                let message = format!(
                    "`{callee}` copies its `{parameter}`, `{argument}` here, which cannot be \
                     copied through a type parameter yet"
                );
                self.lowerer.error(
                    node,
                    &message,
                    "copied through a type parameter",
                    Some("copying this type through a type parameter is idea I.76"),
                );
            } else {
                let message = format!(
                    "`{callee}` calls `.copy()` on `{parameter}`, and `{argument}` has no copy"
                );
                self.lowerer.error(
                    node,
                    &message,
                    "a type with no copy",
                    Some(
                        "a type parameter can be copied when it is a copy type, a `str`, a list \
                         or array of those, or a struct with its own `copy` method",
                    ),
                );
            }
        }
    }
}

/// Whether `ty`, a type a call fixes a type parameter to in `package`, has a copy, and whether
/// it is built through a type parameter. The caller has already sent a `Path` to
/// [`Copyable::NotYet`]. A type parameter of the caller, or an unknown type,
/// counts as built: its own instances, or an error already reported, settle it.
fn copyable<'db>(db: &'db dyn TypeDb, package: database::Package, ty: Type<'db>) -> Copyable {
    let element_copies = |element: Type<'db>| {
        !matches!(element.kind(db), TypeKind::Parameter { .. }) && is_copy(db, element)
    };
    match ty.kind(db) {
        TypeKind::Str => Copyable::Built,
        TypeKind::List(element)
            if element_copies(*element) || matches!(element.kind(db), TypeKind::Str) =>
        {
            Copyable::Built
        }
        TypeKind::Array { element, .. } if element_copies(*element) => Copyable::Built,
        _ if is_copy(db, ty) => Copyable::Built,
        TypeKind::Struct(nominal) => match embed::own(db, package, nominal, "copy") {
            Some(embed::Own::Method { .. }) => Copyable::Built,
            _ => Copyable::Missing,
        },
        TypeKind::List(_)
        | TypeKind::Array { .. }
        | TypeKind::Dict { .. }
        | TypeKind::DataFrame { .. }
        | TypeKind::TextArray
        | TypeKind::OptionalArray(_) => Copyable::NotYet,
        _ => Copyable::Missing,
    }
}

/// Whether the body of the function `item`, whose signature is `signature`, calls `.copy()` on
/// a value of the type parameter `parameter`, found as the module documentation says.
fn copies<'db>(
    db: &'db dyn TypeDb,
    item: Item<'db>,
    signature: &Signature<'db>,
    parameter: Type<'db>,
) -> bool {
    let (package, file) = (item.package(db), item.file(db));
    let Some(entry) = outline(db, file).get(item.name(db)) else {
        return false;
    };
    let tree = db.parse(file).tree();
    let text = file.source(db).text();
    let resolution = db.resolve(package, file);
    // The type of `expression`: a parameter's declared type, followed through each index.
    let type_of = |expression: NodeId| {
        let mut indices = Vec::new();
        let mut root = expression;
        while tree.kind(root) == SyntaxKind::Subscript {
            indices.push(tree.child(root, Role::Index)?);
            root = tree.child(root, Role::Value)?;
        }
        if tree.kind(root) != SyntaxKind::Name
            || !matches!(resolution.definition(root), Some(Definition::Local(_)))
        {
            return None;
        }
        let name = tree.text(root, text);
        let mut ty = signature
            .parameters
            .iter()
            .find(|declared| declared.name == name)?
            .ty;
        for &index in indices.iter().rev() {
            let count = if tree.kind(index) == SyntaxKind::Tuple
                && !tree.node(index).flags().contains(Flags::PARENTHESIZED)
            {
                tree.children_with(index, Role::Item).count()
            } else {
                1
            };
            let sliced = tree.kind(index) == SyntaxKind::Slice
                || tree
                    .children_with(index, Role::Item)
                    .any(|item| tree.kind(item) == SyntaxKind::Slice);
            if sliced {
                return None;
            }
            ty = match ty.kind(db) {
                TypeKind::List(element) if count == 1 => *element,
                TypeKind::Array { element, rank } if count == *rank as usize => *element,
                TypeKind::Dict { value, .. } if count == 1 => *value,
                _ => return None,
            };
        }
        Some(ty)
    };
    let mut stack = vec![entry.statement];
    while let Some(node) = stack.pop() {
        if tree.kind(node) == SyntaxKind::Call
            && let Some(function) = tree.child(node, Role::Function)
            && tree.kind(function) == SyntaxKind::Attribute
            && tree
                .child(function, Role::Attribute)
                .is_some_and(|attribute| tree.text(attribute, text) == "copy")
            && tree
                .child(node, Role::Arguments)
                .is_none_or(|list| tree.children(list).next().is_none())
            && let Some(value) = tree.child(function, Role::Value)
            && type_of(value) == Some(parameter)
        {
            return true;
        }
        stack.extend(tree.children(node));
    }
    false
}
