//! Reading a generic state in an instance (card 60, an amendment to decision 0065 point 1).
//!
//! Inside a body generic in `S: VectorSpace`, lowering writes `len(y)` as
//! [`Rvalue::Length`] and `y[i]` as a copy of `y` projected by one index, the forms it writes
//! for an `Array`. Once substitution has given `S` its instance's type, [`read`] rewrites the
//! two forms an `Array` instance does not already take:
//!
//! - For a `Fixed` state, `len(y)` becomes the constant length of its first dimension, as
//!   lowering writes `len` of a `Fixed` vector. Its elements are indexed as they are.
//! - For an `f64` state, `len(y)` becomes 1 and `y[i]` the number itself. The bounds check
//!   lowering wrote before the read stays, so `y[1]` on an `f64` panics as it would on an
//!   `Array` of length 1.

use mir::{Body, Constant, Operand, Place, Projection, Rvalue, StatementKind};
use types::{Type, TypeKind};

/// Rewrites every `len(y)` and `y[i]` in `body` and its outlined bodies whose `y` this instance
/// makes a `Fixed` vector or a number. Lowering copies each element into a temporary and each
/// length into a local at once, so only those two assignments name a state that way. The state
/// may be a local, or reached from one through items of tuples and elements of lists, as
/// `states[j][i]` reads element `i` of the `j`th state a `list[S]` holds.
pub(crate) fn read<'db>(db: &'db dyn salsa::Database, body: &mut Body<'db>) {
    for outlined in &mut body.outlined {
        read(db, outlined);
    }
    let types: Vec<Type<'db>> = body.locals.iter().map(|local| local.ty).collect();
    for block in &mut body.blocks {
        for statement in &mut block.statements {
            let StatementKind::Assign(destination, rvalue) = &mut statement.kind else {
                continue;
            };
            match rvalue {
                Rvalue::Length(place, 0) => {
                    let Some(state) = projected(db, types[place.local.index()], &place.projections)
                    else {
                        continue;
                    };
                    let length = match state.kind(db) {
                        TypeKind::Primitive(_) => 1,
                        TypeKind::Fixed { shape, .. } => i128::from(shape.lengths()[0]),
                        _ => continue,
                    };
                    let ty = types[destination.local.index()];
                    *rvalue = Rvalue::Use(Operand::Constant(Constant::integer(ty, length)));
                }
                Rvalue::Use(Operand::Copy(place) | Operand::Move(place))
                    if number_element(db, &types, place) =>
                {
                    place.projections.pop();
                }
                _ => {}
            }
        }
    }
}

/// Whether `place` is one element of a number: `y[i]` on an `f64` state.
fn number_element<'db>(db: &'db dyn salsa::Database, types: &[Type<'db>], place: &Place) -> bool {
    let Some((Projection::Index(indices), outer)) = place.projections.split_last() else {
        return false;
    };
    indices.len() == 1
        && projected(db, types[place.local.index()], outer)
            .is_some_and(|ty| matches!(ty.kind(db), TypeKind::Primitive(_)))
}

/// The type of the part `projections` reach from a value of type `ty`, when every step is an
/// item of a tuple or an element of a list. `None` for any other step, which never reaches a
/// state's element read.
fn projected<'db>(
    db: &'db dyn salsa::Database,
    ty: Type<'db>,
    projections: &[Projection],
) -> Option<Type<'db>> {
    projections
        .iter()
        .try_fold(ty, |ty, projection| match (projection, ty.kind(db)) {
            (Projection::Field(field), TypeKind::Tuple(items)) => {
                items.get(usize::try_from(*field).ok()?).copied()
            }
            (Projection::Index(_), TypeKind::List(item)) => Some(*item),
            _ => None,
        })
}
