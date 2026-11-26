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
/// length into a local at once, so only those two assignments name a state that way.
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
                Rvalue::Length(place, 0) if place.projections.is_empty() => {
                    let length = match types[place.local.index()].kind(db) {
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
                    *place = Place::from(place.local);
                }
                _ => {}
            }
        }
    }
}

/// Whether `place` is one element of a local whose type is a number: `y[i]` on an `f64` state.
fn number_element<'db>(db: &'db dyn salsa::Database, types: &[Type<'db>], place: &Place) -> bool {
    matches!(place.projections.as_slice(), [Projection::Index(indices)] if indices.len() == 1)
        && matches!(types[place.local.index()].kind(db), TypeKind::Primitive(_))
}
