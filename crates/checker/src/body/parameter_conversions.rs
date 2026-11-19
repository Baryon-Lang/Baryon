//! A conversion to a type parameter, `T(x)`, where `T` is bounded by `Number` or `Float`
//! (decision 0066, roadmap 1.9.59).
//!
//! `T(len(xs))` in `def mean[T: Float](xs: Array[T, 1]) -> T` converts the count to `T`. The
//! checker gives the call the type `T` and records it as a conversion to `T`; lowering writes
//! the conversion it writes for `f64(n)`, and monomorphization makes it the concrete
//! conversion for each type argument, so `T(n)` with `T` an `i8` panics where `i8(n)` would.
//! The value converted is a real number, an integer or a float of any width, or a `T`: no
//! conversion goes from a complex number, a `bool` or text to a real one.
//!
//! ```text
//! error: `T(...)` converts a real number, found `complex128`
//! ```
//!
//! The reverse, a number conversion of a `T`'s value such as `f64(x)`, is not built yet
//! (roadmap 1.9.69): under `Number` a `complex128` type argument would reach the backend, so
//! taking it needs a rule for that case first (idea I.72).

use super::Body;
use super::calls::Arguments;
use super::relations;
use crate::traits::Trait;
use syntax::NodeId;
use types::{Call, Callee, Key, Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// The type of `name(arguments)` at `node`, where `name` is the type parameter whose
    /// binding is `binding`, when its bound is `Number` or `Float`: the parameter itself, or an
    /// error type for a call that does not convert one real number. `None` under any other
    /// bound, or none, where calling a type parameter stays an error.
    pub(super) fn parameter_conversion(
        &mut self,
        node: NodeId,
        name: &str,
        binding: NodeId,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        let parameter = Type::new(
            self.db,
            TypeKind::Parameter {
                file: self.lowerer.file,
                name: binding,
            },
        );
        if !matches!(self.trait_of(parameter), Some(Trait::Number | Trait::Float)) {
            return None;
        }
        let ([argument], []) = (
            arguments.positional.as_slice(),
            arguments.keywords.as_slice(),
        ) else {
            self.untyped(arguments);
            let count = arguments.positional.len() + arguments.keywords.len();
            let message = format!("`{name}(...)` converts one value, found {count}");
            return Some(
                self.lowerer
                    .error(node, &message, "wrong number of arguments", None),
            );
        };
        let ty = self.expression(*argument, None);
        let real = ty == parameter
            || relations::is_open(self.db, ty)
            || matches!(
                ty.kind(self.db),
                TypeKind::Primitive(Primitive::Integer(_) | Primitive::Float(_))
            );
        if !real {
            let message = format!(
                "`{name}(...)` converts a real number, found `{}`",
                self.show(ty)
            );
            return Some(self.lowerer.error(
                *argument,
                &message,
                "not a real number",
                Some("an integer or a float of any width converts to a type parameter"),
            ));
        }
        let call = Call {
            callee: Callee::Conversion(parameter),
            type_arguments: Vec::new(),
            arguments: vec![Some(Key(*argument))],
        };
        self.typed.calls.insert(Key(node), call);
        Some(parameter)
    }

    /// Reports `name(argument)`, a conversion to a number type, when `ty`, the argument's type,
    /// is a type parameter: `f64(x)` with `x: T` is not built yet (roadmap 1.9.69, idea I.72).
    /// `None` for any other argument, which the caller checks as before.
    pub(super) fn conversion_of_parameter(
        &mut self,
        argument: NodeId,
        name: &str,
        ty: Type<'db>,
    ) -> Option<Type<'db>> {
        if !matches!(ty.kind(self.db), TypeKind::Parameter { .. }) {
            return None;
        }
        let message = format!(
            "`{name}(...)` cannot convert a value of type `{}` yet",
            self.show(ty)
        );
        Some(self.lowerer.error(
            argument,
            &message,
            "a type parameter's value",
            Some("convert the other value to the type parameter instead, as in `x * T(k)`"),
        ))
    }
}
