//! `*args` in an instance: each element of the spread tuple passed as an argument of its own
//! (the decision 0026 amendment).
//!
//! A generic body cannot know how many elements a `tuple[*Ts]` has, so lowering writes
//! `f(t, y, *args)` as one [`Argument::Spread`] of the tuple's local. Once substitution has given
//! that local its instance's tuple type, the count is known, and [`expand`] replaces the spread
//! with a [`Borrow`](Argument::Borrow) of each element in order. A spread of the empty tuple adds
//! nothing, so `solve_ivp(f, span, y0)` calls `f(t, y)`. No stage after this one sees a spread.

use mir::{Argument, Body, Place, Projection, TerminatorKind};
use types::TypeKind;

/// Replaces every spread argument in `body` and its outlined bodies with one borrow of each
/// element of its tuple. Lowering spreads only a whole local, whose type substitution has made a
/// concrete tuple; a spread of anything else is left for the backend to report.
pub(crate) fn expand<'db>(db: &'db dyn salsa::Database, body: &mut Body<'db>) {
    for outlined in &mut body.outlined {
        expand(db, outlined);
    }
    let types: Vec<_> = body.locals.iter().map(|local| local.ty).collect();
    for block in &mut body.blocks {
        let (TerminatorKind::Call { arguments, .. }
        | TerminatorKind::ParallelFor { arguments, .. }) = &mut block.terminator.kind
        else {
            continue;
        };
        if !arguments
            .iter()
            .any(|argument| matches!(argument, Argument::Spread(_)))
        {
            continue;
        }
        let mut expanded = Vec::with_capacity(arguments.len());
        for argument in arguments.drain(..) {
            let Argument::Spread(place) = &argument else {
                expanded.push(argument);
                continue;
            };
            let count = match types[place.local.index()].kind(db) {
                TypeKind::Tuple(items) if place.projections.is_empty() => items.len(),
                _ => {
                    expanded.push(argument);
                    continue;
                }
            };
            let count = u32::try_from(count).unwrap_or(u32::MAX);
            expanded.extend((0..count).map(|field| {
                Argument::Borrow(Place::from(place.local).project(Projection::Field(field)))
            }));
        }
        *arguments = expanded;
    }
}
