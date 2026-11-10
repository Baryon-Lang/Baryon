//! Complex numbers: what `complex64` and `complex128` have beyond the operators (language
//! reference, "Types", "Complex numbers").
//!
//! As in Python, a complex number `z` has the attributes `z.real` and `z.imag`, each a float of
//! its parts' width, and the method `z.conjugate()`, and `abs(z)` is its modulus, a float. The
//! two-argument conversion `complex(re, im)` builds one from its parts. A complex number is a
//! value, like a float, so its parts are read, never assigned: `z.real = 1.0` is an error, as
//! Python's `AttributeError` says. Python has no `//`, `%`, ordering, `round`, `min` or `max` of
//! complex numbers, and no conversion of one to a real number, so neither does the language.

use super::Body;
use super::calls::Arguments;
use super::exclusive::Passed;
use super::methods::copy_signature;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Callee, Complex, Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// The complex type of `ty`, if it is one.
    pub(super) fn complex(&self, ty: Type<'db>) -> Option<Complex> {
        match ty.kind(self.db) {
            TypeKind::Primitive(Primitive::Complex(complex)) => Some(*complex),
            _ => None,
        }
    }

    /// The float type of a part of the complex type `complex`.
    fn part(&self, complex: Complex) -> Type<'db> {
        Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Float(complex.part())),
        )
    }

    /// `base.name` when `base` is a complex number: `real` or `imag`, or an error for any other
    /// name. `None` when `base` is not complex.
    pub(super) fn complex_attribute(
        &mut self,
        node: NodeId,
        base: Type<'db>,
        name: &str,
    ) -> Option<Type<'db>> {
        let complex = self.complex(base)?;
        Some(match name {
            "real" | "imag" => self.part(complex),
            _ => self.no_complex_member(node, base, name, "attribute"),
        })
    }

    /// The call `value.name(arguments)` on a complex number, `function` being `value.name` and
    /// `receiver` the value: `conjugate()`, its one method, which reads the number and returns a
    /// new one of the same type, as a built-in method; any other name is an error.
    pub(super) fn complex_method(
        &mut self,
        node: NodeId,
        function: NodeId,
        receiver: Passed<'db>,
        name: &str,
        arguments: &Arguments,
    ) -> Type<'db> {
        if name == "conjugate" {
            let signature = copy_signature(receiver.ty);
            let builtin = Callee::Builtin("complex.conjugate".to_owned());
            return self.check_call(node, name, &signature, Some(receiver), builtin, arguments);
        }
        self.untyped(arguments);
        self.no_complex_member(function, receiver.ty, name, "method")
    }

    /// Reports `base.name` on a complex number that has no such attribute or method.
    fn no_complex_member(
        &mut self,
        node: NodeId,
        base: Type<'db>,
        name: &str,
        what: &str,
    ) -> Type<'db> {
        let message = format!("`{}` has no {what} `{name}`", self.show(base));
        self.lowerer.error(
            node,
            &message,
            "not defined on this type",
            Some("a complex number has `real`, `imag` and `conjugate()`"),
        )
    }

    /// Reports an assignment to `target` when it is `z.real` or `z.imag`: a complex number is
    /// a value whose parts are only read, as in Python. Call after `target` is typed.
    pub(super) fn assigned_complex_part(&mut self, target: NodeId) {
        if self.tree.kind(target) != SyntaxKind::Attribute {
            return;
        }
        let Some(value) = self.tree.child(target, Role::Value) else {
            return;
        };
        let complex = self
            .typed
            .expressions
            .get(&types::Key(value))
            .is_some_and(|&ty| self.complex(ty).is_some());
        if complex {
            self.lowerer.error(
                target,
                "the parts of a complex number cannot be assigned",
                "assigned here",
                Some("build a new number instead: `z = complex(x, z.imag)`"),
            );
        }
    }

    /// The checks a conversion `name(arguments…)` to `target` needs when a complex number is
    /// involved: `complex(re, im)` from two parts, and the error for converting a complex
    /// number to a real one, which Python rejects with `TypeError`. `None` when neither
    /// applies, so the caller checks the conversion as any other.
    pub(super) fn complex_conversion(
        &mut self,
        node: NodeId,
        name: &str,
        target: Primitive,
        arguments: &[NodeId],
        types: &[Type<'db>],
    ) -> Option<Type<'db>> {
        match (target, types) {
            (Primitive::Complex(complex), &[real, imaginary]) => {
                let parts = [(arguments[0], real), (arguments[1], imaginary)];
                Some(self.complex_from_parts(complex, &parts))
            }
            (Primitive::Complex(_), _) => None,
            (_, &[value]) if self.complex(value).is_some() => {
                let message =
                    format!("`{name}(...)` cannot convert a complex number to a real one");
                Some(self.lowerer.error(
                    node,
                    &message,
                    "a complex number",
                    Some("take a part with `z.real` or `z.imag`, or the modulus with `abs(z)`"),
                ))
            }
            _ => None,
        }
    }

    /// The type of the built-in call `name(arguments…)` when an argument is a complex number
    /// and the call means something else than for real numbers: `abs(z)` is a float, and
    /// `round`, `min` and `max` are errors, as Python's `TypeError` says, since a complex number
    /// has no nearest whole number and no order. `None` when no argument is complex.
    pub(super) fn complex_builtin(
        &mut self,
        name: &str,
        arguments: &[NodeId],
        types: &[Type<'db>],
    ) -> Option<Type<'db>> {
        let position = types.iter().position(|&ty| self.complex(ty).is_some())?;
        let complex = self.complex(types[position])?;
        let hint = match (name, types.len()) {
            ("abs", 1) => return Some(self.part(complex)),
            ("round", _) => "round each part: `round(z.real)`, `round(z.imag)`",
            ("min" | "max", _) => {
                "complex numbers have no order; compare their moduli with `abs(z)`"
            }
            _ => return None,
        };
        let message = format!("`{name}` is not defined on complex numbers");
        Some(self.lowerer.error(
            arguments[position],
            &message,
            "a complex number",
            Some(hint),
        ))
    }

    /// `complex(re, im)`, `complex64(re, im)` or `complex128(re, im)`: a complex number of type
    /// `complex` from two real numbers, each converted to the float type of its parts.
    pub(super) fn complex_from_parts(
        &mut self,
        complex: Complex,
        parts: &[(NodeId, Type<'db>)],
    ) -> Type<'db> {
        for &(part, ty) in parts {
            let real = matches!(
                ty.kind(self.db),
                TypeKind::Primitive(Primitive::Integer(_) | Primitive::Float(_))
            );
            if !real && !matches!(ty.kind(self.db), TypeKind::Unknown) {
                let message = format!(
                    "each part of a complex number is a real number, found `{}`",
                    self.show(ty)
                );
                self.lowerer
                    .error(part, &message, "not a real number", None);
            }
        }
        Type::new(self.db, TypeKind::Primitive(Primitive::Complex(complex)))
    }
}
