//! `x.copy()` on a type parameter once the instance has made it a struct with its own `copy`
//! method (roadmap 1.9.60, decision 0066): lowering writes [`Builtin::Copy`], since in the
//! generic body it cannot tell which copy the type has, and here the call becomes a direct call
//! of the struct's method, as a trait method's call does in [`trait_calls`](crate::trait_calls).
//! A copy of any other type stays a `Builtin::Copy`, which the backend writes.

use mir::{Body, Builtin, Callee, TerminatorKind};
use types::{Item, Type};

/// Finds a struct's own `copy` method: given a type, the method and the type arguments it is
/// compiled at, those of its class. `None` when the type is not a struct that defines `copy`
/// itself. The application builds it from the type checker.
pub type CopyMethod<'a, 'db> = dyn Fn(Type<'db>) -> Option<(Item<'db>, Vec<Type<'db>>)> + 'a;

/// Turns each [`Builtin::Copy`] in `body`, an instance's body after
/// [`instantiate`](crate::instantiate), and in its outlined `prange` bodies, whose argument is a
/// struct with its own `copy` method into a direct call of that method. The method takes `self`
/// borrowed and returns a new value, so the call keeps its argument, destination and target.
/// The struct is read from the destination's type: lowering writes a copy into a local of its
/// own, and a copy into any other place is left for the backend.
///
/// Cost: one call of `copy_method` per `Builtin::Copy` in the body.
///
/// ```
/// use mir::{Body, Terminator, TerminatorKind};
/// use source::{FileId, Span};
/// use types::{Type, TypeKind};
///
/// # #[salsa::db]
/// # #[derive(Clone, Default)]
/// # struct Database { storage: salsa::Storage<Self> }
/// # #[salsa::db]
/// # impl salsa::Database for Database {}
/// let database = Database::default();
/// let span = Span::new(FileId::from_index(0), 0, 1);
/// let none = Type::new(&database, TypeKind::None);
/// let mut body = Body::new(none, span);
/// body.push_block(Terminator { kind: TerminatorKind::Return, span });
///
/// // A body without copies is left as it is.
/// let before = body.clone();
/// monomorphize::own_copies(&mut body, &|_| None);
/// assert_eq!(body, before);
/// ```
pub fn own_copies<'db>(body: &mut Body<'db>, copy_method: &CopyMethod<'_, 'db>) {
    for outlined in &mut body.outlined {
        own_copies(outlined, copy_method);
    }
    let types: Vec<Option<Type<'db>>> = body
        .blocks
        .iter()
        .map(|block| match &block.terminator.kind {
            TerminatorKind::Call {
                callee: Callee::Builtin(Builtin::Copy),
                destination,
                ..
            } if destination.projections.is_empty() => {
                Some(body.locals[destination.local.index()].ty)
            }
            _ => None,
        })
        .collect();
    for (block, ty) in body.blocks.iter_mut().zip(types) {
        let Some((item, type_arguments)) = ty.and_then(copy_method) else {
            continue;
        };
        if let TerminatorKind::Call { callee, .. } = &mut block.terminator.kind {
            *callee = Callee::Function {
                item,
                type_arguments,
            };
        }
    }
}
