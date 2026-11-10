//! Method calls: `value.method(arguments)` on a struct, an array, a `Fixed` vector, a built-in
//! container, or a type parameter bounded by a user trait, checked against the method's
//! signature like any other call.

use super::calls::Arguments;
use super::exclusive::Passed;
use super::members::Member;
use super::{Body, relations};
use crate::traits::{self, Trait, UserTrait};
use database::Definition;
use syntax::{NodeId, Role};
use types::{Callee, Declaration, Key, Mode, Parameter, Signature, Type, TypeKind};

impl<'db> Body<'db> {
    /// `value.method(arguments)` on a struct.
    pub(super) fn method_call(
        &mut self,
        node: NodeId,
        function: NodeId,
        arguments: &Arguments,
        expected: Option<Type<'db>>,
    ) -> Type<'db> {
        let (Some(value), Some(attribute)) = (
            self.tree.child(function, Role::Value),
            self.tree.child(function, Role::Attribute),
        ) else {
            return self.untyped(arguments);
        };
        let callee = self.tree.text(attribute, self.text).to_owned();
        match self.lowerer.definition(value) {
            Some(Definition::Builtin("Array")) => {
                return self.array_function(node, &callee, arguments, expected);
            }
            Some(Definition::Item(..)) => {
                // `Vec3.zeros()` on an alias of a `Fixed` type; anything else on a class has no
                // functions in version 0.
                let ty = self.lowerer.lower(value);
                return match ty.kind(self.db) {
                    TypeKind::Fixed { .. } => self.fixed_function(node, ty, &callee, arguments),
                    _ => self.untyped(arguments),
                };
            }
            _ => {}
        }
        let base = self.expression(value, None);
        if self.maybe_none(value, base) {
            self.untyped(arguments);
            return self.lowerer.unknown();
        }
        if let &TypeKind::Standard(standard) = base.kind(self.db) {
            let result = self.standard_method(node, standard, &callee, arguments);
            self.change(value);
            return result;
        }
        if let &TypeKind::Dict { key, value: held } = base.kind(self.db) {
            return self.dict_method(node, (value, base), (key, held), &callee, arguments);
        }
        if let Some(result) = self.container_method(node, base, &callee, arguments) {
            let changes = match base.kind(self.db) {
                TypeKind::List(_) => matches!(callee.as_str(), "append" | "pop"),
                TypeKind::Array { .. } | TypeKind::Fixed { .. } => callee == "fill",
                _ => false,
            };
            if changes {
                self.change(value);
            }
            return result;
        }
        // `copy` on a `str` or a struct that does not define its own: a built-in method.
        let copy = copy_signature(base);
        let receiver = Passed {
            node: value,
            mode: Mode::Borrow,
            ty: base,
        };
        if let Some(Trait::User(bound)) = self.trait_of(base) {
            return self.trait_method_call(node, function, base, bound, &receiver, arguments);
        }
        let nominal = match base.kind(self.db) {
            TypeKind::Str if callee == "copy" => {
                let builtin = Callee::Builtin("str.copy".to_owned());
                return self.check_call(node, &callee, &copy, Some(receiver), builtin, arguments);
            }
            TypeKind::Struct(nominal) => nominal.clone(),
            _ if self.has_no_members(base) => {
                return self.no_method(function, base, &callee, arguments);
            }
            _ => return self.untyped(arguments),
        };
        match self.member(function, &nominal, base, attribute) {
            Some(Member::Method {
                item,
                parameters,
                arguments: type_arguments,
            }) => {
                let Declaration::Function(signature) = &self.db.declaration(item).declaration
                else {
                    return self.untyped(arguments);
                };
                let signature = relations::substitute_signature(
                    self.db,
                    signature,
                    nominal.file,
                    &parameters,
                    &type_arguments,
                );
                let receiver = signature.parameters.first().map(|parameter| Passed {
                    mode: parameter.mode,
                    ..receiver
                });
                let promotion =
                    self.owned_promotion(function, &callee, receiver.as_ref(), arguments);
                if let Some(error) = promotion {
                    return error;
                }
                let method = Callee::Method(item);
                self.check_call(node, &callee, &signature, receiver, method, arguments)
            }
            Some(Member::Field(ty)) => self.call_value(node, &callee, ty, arguments),
            Some(Member::Copy) => {
                let builtin = Callee::Builtin("struct.copy".to_owned());
                self.check_call(node, &callee, &copy, Some(receiver), builtin, arguments)
            }
            None => self.untyped(arguments),
        }
    }

    /// Reports the call of a method reached through an embedded field, `function`, that takes
    /// its struct as `Owned`: it would move the field out and leave the outer struct with a
    /// hole. Returns the unknown type after reporting, and `None` for any other call.
    fn owned_promotion(
        &mut self,
        function: NodeId,
        callee: &str,
        receiver: Option<&Passed<'db>>,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        if receiver.is_none_or(|receiver| receiver.mode != Mode::Owned)
            || self.typed.promotion(function).is_empty()
        {
            return None;
        }
        let message = format!("`{callee}` takes its embedded struct as `Owned`");
        self.untyped(arguments);
        Some(self.lowerer.error(
            function,
            &message,
            "would move a field out of the struct",
            Some(
                "call it on the embedded field by name, `value.field.method()`, which moves the \
                 whole struct",
            ),
        ))
    }

    /// `value.method(arguments)` where `value` has the type `base`, a type parameter bounded by
    /// the user trait `bound` (decision 0031): the method `bound`, or a trait it requires,
    /// lists, with `Self` in its signature replaced by `base`. The call is recorded as calling
    /// the trait's method with `base` as its one type argument; each instance of the generic
    /// function calls the method of the type `base` stands for.
    fn trait_method_call(
        &mut self,
        node: NodeId,
        function: NodeId,
        base: Type<'db>,
        bound: UserTrait,
        receiver: &Passed<'db>,
        arguments: &Arguments,
    ) -> Type<'db> {
        let Some(attribute) = self.tree.child(function, Role::Attribute) else {
            return self.untyped(arguments);
        };
        let callee = self.tree.text(attribute, self.text);
        let package = self.lowerer.package;
        let available = traits::closure(self.db, package, [bound]);
        let Some((owner, item)) = traits::find_method(self.db, package, &available, callee) else {
            self.untyped(arguments);
            let message = format!("`{}` has no method `{callee}`", self.show(base));
            let hint = format!(
                "`{}` has only the methods its bound `{}` and the traits it requires list",
                self.show(base),
                bound.name(self.db)
            );
            return self
                .lowerer
                .error(function, &message, "not in the trait", Some(&hint));
        };
        let Declaration::Function(signature) = &self.db.declaration(item).declaration else {
            return self.untyped(arguments);
        };
        let signature =
            relations::substitute_signature(self.db, signature, owner.file, &[owner.name], &[base]);
        let receiver = signature.parameters.first().map(|parameter| Passed {
            mode: parameter.mode,
            ..*receiver
        });
        let callee = callee.to_owned();
        let method = Callee::Method(item);
        let returns = self.check_call(node, &callee, &signature, receiver, method, arguments);
        // The trait's method is generic in the implementing type: each instance substitutes
        // `base` and finds that type's method (`types::Call::type_arguments`).
        if let Some(call) = self.typed.calls.get_mut(&Key(node)) {
            call.type_arguments = vec![base];
        }
        returns
    }

    /// Checks the arguments of the call of `name` on `base`, a type with no methods, then
    /// reports the call.
    fn no_method(
        &mut self,
        function: NodeId,
        base: Type<'db>,
        name: &str,
        arguments: &Arguments,
    ) -> Type<'db> {
        self.untyped(arguments);
        self.no_member(function, base, name, true)
    }
}

/// The signature of the built-in `copy` of a `str` or a struct of type `base`: it borrows the
/// value and returns a new one of the same type.
fn copy_signature(base: Type<'_>) -> Signature<'_> {
    Signature {
        type_parameters: Vec::new(),
        parameters: vec![Parameter {
            name: "self".to_owned(),
            ty: base,
            mode: Mode::Borrow,
            default: false,
        }],
        returns: base,
    }
}
