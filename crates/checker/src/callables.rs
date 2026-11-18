//! Where a `Callable` may appear in a declaration (decision 0026): as the whole type of a
//! parameter, and nowhere else in version 0.
//!
//! A `Callable` parameter is compiled once per function passed to it, so the function is known
//! when compiling and the value takes no bytes. A `Callable` kept anywhere else would need a
//! function value at run time and an indirect call, which version 0 leaves out: in a struct
//! field, in a list, tuple or other container, as an optional `Callable | None` parameter, or as
//! a return type. Each is reported here, at its annotation, so the program never reaches a
//! later stage that cannot build it:
//!
//! ```text
//! error: a struct field cannot hold a `Callable` in version 0
//! ```

use crate::lower::Lowerer;
use database::Db;
use syntax::NodeId;
use types::{Type, TypeKind};

/// Where a declaration's type was written, for the message that reports a `Callable` in it.
#[derive(Clone, Copy)]
pub(crate) enum Place {
    /// A struct field's annotation.
    Field,
    /// A parameter's annotation, where a `Callable` may be the whole type but not part of it.
    Parameter,
    /// A function's return annotation.
    Return,
}

impl<'db> Lowerer<'db> {
    /// `ty`, the type `annotation` declares at `place`, or the unknown type after reporting a
    /// `Callable` that version 0 cannot keep there. The unknown type keeps later checks from
    /// reporting the same mistake again.
    pub(crate) fn kept_callable(
        &mut self,
        annotation: NodeId,
        ty: Type<'db>,
        place: Place,
    ) -> Type<'db> {
        let whole = matches!(ty.kind(self.db), TypeKind::Callable { .. });
        let reported = match place {
            Place::Parameter if whole => holds_callable(self.db, ty, true),
            _ => holds_callable(self.db, ty, false),
        };
        if !reported {
            return ty;
        }
        let shown = ty.display(self.db).to_string();
        let (message, hint) = match place {
            Place::Field => (
                "a struct field cannot hold a `Callable` in version 0".to_owned(),
                "a `Callable` is a parameter type only (decision 0026); pass the function to each \
                 function that calls it, or give the struct a method",
            ),
            Place::Return => (
                "a function cannot return a `Callable` in version 0".to_owned(),
                "a `Callable` is a parameter type only (decision 0026); return the value the \
                 function computes, or pass the function on as an argument",
            ),
            Place::Parameter => (
                format!("a parameter of type `{shown}` is not supported in version 0"),
                "a `Callable` must be a parameter's whole type, not optional or inside another \
                 type (decision 0026); write a second function for the case without it",
            ),
        };
        self.error(
            annotation,
            &message,
            "a `Callable` kept as a value",
            Some(hint),
        )
    }
}

/// Whether `ty` holds a `Callable` that would need a function value at run time. When
/// `parameter`, `ty` is a `Callable` parameter's own type: its parameter types are parameters
/// in turn and are not searched, but a `Callable` it returns is reported.
fn holds_callable<'db>(db: &'db dyn Db, ty: Type<'db>, parameter: bool) -> bool {
    let any = |types: &[Type<'db>]| types.iter().any(|&ty| holds_callable(db, ty, false));
    match ty.kind(db) {
        TypeKind::Callable { returns, .. } if parameter => holds_callable(db, *returns, false),
        TypeKind::Callable { .. } | TypeKind::Function { .. } => true,
        TypeKind::Tuple(types) | TypeKind::Union(types) => any(types),
        TypeKind::List(element)
        | TypeKind::Optional(element)
        | TypeKind::OptionalArray(element)
        | TypeKind::Array { element, .. }
        | TypeKind::Fixed { element, .. } => holds_callable(db, *element, false),
        TypeKind::Dict { key, value } => any(&[*key, *value]),
        TypeKind::Struct(nominal) | TypeKind::Enum(nominal) => any(&nominal.arguments),
        TypeKind::Fallible { value, errors } => holds_callable(db, *value, false) || any(errors),
        TypeKind::Primitive(_)
        | TypeKind::Str
        | TypeKind::None
        | TypeKind::TextArray
        | TypeKind::DataFrame { .. }
        | TypeKind::Standard(_)
        | TypeKind::Parameter { .. }
        | TypeKind::Unknown => false,
    }
}
