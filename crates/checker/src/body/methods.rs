//! Method calls: `value.method(arguments)` on a struct, an array, a `Fixed` vector or a
//! built-in container, checked against the method's signature like any other call.

use super::calls::Arguments;
use super::exclusive::Passed;
use super::members::Member;
use super::{Body, relations};
use database::Definition;
use syntax::{NodeId, Role};
use types::{Callee, Declaration, Parameter, Signature, Type, TypeKind};

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
            mode: types::Mode::Borrow,
            ty: base,
        };
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
                let substitute = |ty| {
                    relations::substitute(self.db, ty, nominal.file, &parameters, &type_arguments)
                };
                let signature = Signature {
                    type_parameters: signature.type_parameters.clone(),
                    parameters: signature
                        .parameters
                        .iter()
                        .map(|parameter| Parameter {
                            ty: substitute(parameter.ty),
                            ..parameter.clone()
                        })
                        .collect(),
                    returns: substitute(signature.returns),
                };
                let receiver = signature.parameters.first().map(|parameter| Passed {
                    mode: parameter.mode,
                    ..receiver
                });
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
            mode: types::Mode::Borrow,
            default: false,
        }],
        returns: base,
    }
}
