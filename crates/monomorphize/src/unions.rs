//! Trait calls whose receiver is a union (decision 0032): a switch on the member the union holds
//! and one direct call per member, each on the receiver read as that member.

use crate::Implementation;
use mir::{
    Argument, Body, Callee, LocalKind, Operand, Place, Projection, Rvalue, Statement,
    StatementKind, TerminatorKind,
};
use types::{Integer, Item, Primitive, Type, TypeKind};

/// Turns the call of a trait's method `call.0` at block `index`, whose receiver is a union with
/// the members `call.1` (decision 0032), into a switch on the member the union holds and, for
/// each member, a call of that member's method on the receiver read as the member. Every call
/// keeps the original's destination, target and error edge. The call is left as it is when a
/// member does not implement the method, which the checker has already reported.
pub(crate) fn member_calls<'db>(
    db: &'db dyn salsa::Database,
    body: &mut Body<'db>,
    index: usize,
    (item, members): (Item<'db>, &[Type<'db>]),
    implementation: &Implementation<'_, 'db>,
) {
    let Some(found) = members
        .iter()
        .map(|&member| implementation(item, member))
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };
    let terminator = body.blocks[index].terminator.clone();
    let TerminatorKind::Call { arguments, .. } = &terminator.kind else {
        return;
    };
    let Some(union) = arguments.first().and_then(|first| place_of(first).cloned()) else {
        return;
    };
    let mut cases = Vec::with_capacity(found.len());
    for ((position, (method, path, method_arguments)), value) in (0_u32..).zip(found).zip(0_i128..)
    {
        let mut call = terminator.clone();
        if let TerminatorKind::Call {
            callee, arguments, ..
        } = &mut call.kind
        {
            *callee = Callee::Function {
                item: method,
                type_arguments: method_arguments,
            };
            if let Some(receiver) = arguments.first_mut().and_then(argument_place) {
                receiver.projections.push(Projection::Member(position));
                receiver
                    .projections
                    .extend(path.into_iter().map(Projection::Field));
            }
        }
        cases.push((value, body.push_block(call)));
    }
    let i64 = Type::new(db, TypeKind::Primitive(Primitive::Integer(Integer::I64)));
    let tag = body.push_local(i64, LocalKind::Temporary, terminator.span);
    let block = &mut body.blocks[index];
    block.statements.push(Statement {
        kind: StatementKind::Assign(Place::from(tag), Rvalue::MemberIndex(union)),
        span: terminator.span,
    });
    let otherwise = cases[0].1;
    block.terminator.kind = TerminatorKind::Switch {
        value: Operand::Copy(Place::from(tag)),
        cases,
        otherwise,
    };
}

/// The place a call argument names, to step into it, or `None` for a constant.
pub(crate) fn argument_place<'a>(argument: &'a mut Argument<'_>) -> Option<&'a mut Place> {
    match argument {
        Argument::Borrow(place)
        | Argument::InOut(place)
        | Argument::Owned(Operand::Copy(place) | Operand::Move(place)) => Some(place),
        Argument::Owned(Operand::Constant(_)) => None,
    }
}

/// The place a call argument names, or `None` for a constant.
fn place_of<'a>(argument: &'a Argument<'_>) -> Option<&'a Place> {
    match argument {
        Argument::Borrow(place)
        | Argument::InOut(place)
        | Argument::Owned(Operand::Copy(place) | Operand::Move(place)) => Some(place),
        Argument::Owned(Operand::Constant(_)) => None,
    }
}
