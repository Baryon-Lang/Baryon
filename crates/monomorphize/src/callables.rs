//! `Callable` values an instance passes on (decision 0026, roadmap 1.9.18): a `Callable`
//! parameter copied into a local, and one passed on to another call, each resolved to the
//! function the instance received.
//!
//! Substitution resolves a `Callable` type only when every parameter of that signature received
//! the same function. When two did not, as in `both(square, cube)` with
//! `def both(f: Callable[[f64], f64], g: Callable[[f64], f64])`, the type alone cannot say
//! which function `f` holds, so these passes follow the values instead: each parameter's local
//! already has its own function's type, and a copy gives the copy's local the same type, and
//! a `prange` loop gives its outlined body's parameter the type of the local it passes.

use mir::{Argument, Body, Callee, Operand, Place, Rvalue, StatementKind, TerminatorKind};
use types::{Item, Type, TypeKind};

/// Gives each local of `body` that is assigned only copies of one function value, as in
/// `h = g` where `g` holds `cube`, that function's own type, so a call of the local becomes a
/// direct call. A local assigned two different functions, as in `h = f if flag else g` when
/// `f` and `g` received different functions, keeps its `Callable` type: it would need a
/// function value at run time, which version 0 does not have, and the backend refuses it.
///
/// Cost: one visit of the body's statements per local resolved, at most.
pub(crate) fn copied<'db>(db: &'db dyn salsa::Database, body: &mut Body<'db>) {
    for outlined in &mut body.outlined {
        copied(db, outlined);
    }
    loop {
        // For each local still of a `Callable` type: the one function copied into it so far,
        // or `None` once two different ones are.
        let mut found: Vec<Option<Option<Type<'db>>>> = vec![None; body.locals.len()];
        for block in &body.blocks {
            for statement in &block.statements {
                let StatementKind::Assign(
                    target,
                    Rvalue::Use(Operand::Copy(source) | Operand::Move(source)),
                ) = &statement.kind
                else {
                    continue;
                };
                let Some(target) = whole(target) else {
                    continue;
                };
                if !matches!(body.locals[target].ty.kind(db), TypeKind::Callable { .. }) {
                    continue;
                }
                // A copy of a local not resolved yet leaves the target open for this round.
                let function = function_of(db, body, source);
                found[target] = match (found[target], function) {
                    (None, Some(function)) => Some(Some(function)),
                    (Some(Some(earlier)), Some(function)) if earlier == function => {
                        Some(Some(function))
                    }
                    _ => Some(None),
                };
            }
        }
        let mut changed = false;
        for (local, function) in body.locals.iter_mut().zip(found) {
            if let Some(Some(function)) = function {
                local.ty = function;
                changed = true;
            }
        }
        if !changed {
            return;
        }
    }
}

/// Gives each parameter of an outlined `prange` body that a [`ParallelFor`] passes a function
/// value, such as `f` in `for i in prange(n): out[i] = f(x[i])`, the own type of the function
/// the passed local holds, then resolves the outlined body's own copies. Substitution leaves a
/// `Callable` type alone when two parameters of that signature received different functions,
/// so the outlined body only learns which function each one holds from the loop's arguments.
///
/// Run after the enclosing body's locals are resolved by [`copied`]. The outlined body's
/// parameters are its return value's local, the iteration number, then one per argument in
/// order (the [`ParallelFor`] documentation).
///
/// Cost: one visit of the body's terminators, and of each outlined body it resolves.
///
/// [`ParallelFor`]: TerminatorKind::ParallelFor
pub(crate) fn shared<'db>(db: &'db dyn salsa::Database, body: &mut Body<'db>) {
    let mut resolved = Vec::new();
    for block in &body.blocks {
        let TerminatorKind::ParallelFor {
            body: outlined,
            arguments,
            ..
        } = &block.terminator.kind
        else {
            continue;
        };
        for (position, argument) in arguments.iter().enumerate() {
            let (Argument::Borrow(place)
            | Argument::InOut(place)
            | Argument::Owned(Operand::Copy(place) | Operand::Move(place))) = argument
            else {
                continue;
            };
            if let Some(function) = function_of(db, body, place) {
                resolved.push((*outlined as usize, position + 2, function));
            }
        }
    }
    for (outlined, local, function) in resolved {
        if let Some(local) = body
            .outlined
            .get_mut(outlined)
            .and_then(|outlined| outlined.locals.get_mut(local))
            && matches!(local.ty.kind(db), TypeKind::Callable { .. })
        {
            local.ty = function;
        }
    }
    for outlined in &mut body.outlined {
        copied(db, outlined);
    }
}

/// Replaces each type argument of a call in `body` that stands for a `Callable` parameter of
/// the callee by the own type of the function the call passes, so the callee's instance is the
/// one for that function. `callables(item)` lists the positions of `item`'s `Callable`
/// parameters among its parameters, the receiver first for a method, in order. Their type
/// arguments are the last ones of the call, one per parameter in that order (decision 0026).
///
/// Run after [`instantiate`](crate::instantiate), whose parameter locals already hold their
/// functions' types. A call that passes a function by name already has its type argument;
/// this matters for a `Callable` the instance received and passes on.
///
/// Cost: one visit of the body's calls, and one call of `callables` per call of a function.
pub fn forward_callables<'db>(
    db: &'db dyn salsa::Database,
    body: &mut Body<'db>,
    callables: &dyn Fn(Item<'db>) -> Vec<usize>,
) {
    for outlined in &mut body.outlined {
        forward_callables(db, outlined, callables);
    }
    let mut resolved = Vec::new();
    for (index, block) in body.blocks.iter().enumerate() {
        let TerminatorKind::Call {
            callee:
                Callee::Function {
                    item,
                    type_arguments,
                },
            arguments,
            ..
        } = &block.terminator.kind
        else {
            continue;
        };
        let positions = callables(*item);
        let Some(first) = type_arguments.len().checked_sub(positions.len()) else {
            continue;
        };
        for (offset, &position) in positions.iter().enumerate() {
            let function = match arguments.get(position) {
                Some(
                    Argument::Borrow(place)
                    | Argument::InOut(place)
                    | Argument::Owned(Operand::Copy(place) | Operand::Move(place)),
                ) => function_of(db, body, place),
                Some(Argument::Owned(Operand::Constant(constant))) => {
                    matches!(constant.ty.kind(db), TypeKind::Function { .. }).then_some(constant.ty)
                }
                None => None,
            };
            if let Some(function) = function {
                resolved.push((index, first + offset, function));
            }
        }
    }
    for (block, position, function) in resolved {
        if let TerminatorKind::Call {
            callee: Callee::Function { type_arguments, .. },
            ..
        } = &mut body.blocks[block].terminator.kind
        {
            type_arguments[position] = function;
        }
    }
}

/// The local `place` names when it is a whole local, with no field or index.
fn whole(place: &Place) -> Option<usize> {
    place.projections.is_empty().then_some(place.local.index())
}

/// The own type of the function `place` holds, when it is a whole local of such a type.
fn function_of<'db>(
    db: &'db dyn salsa::Database,
    body: &Body<'db>,
    place: &Place,
) -> Option<Type<'db>> {
    let ty = body.locals[whole(place)?].ty;
    matches!(ty.kind(db), TypeKind::Function { .. }).then_some(ty)
}
