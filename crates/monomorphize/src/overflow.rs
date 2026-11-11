//! Overflow checks for integer arithmetic on a type parameter (decision 0030): once an instance
//! makes `T` an integer, `+`, `-`, `*` and `-x` on it gain the check lowering writes for the same
//! arithmetic on an integer type.

use mir::{
    BinaryOperator, Body, Check, Constant, ConstantValue, LocalDeclaration, LocalKind, Operand,
    Place, Projection, Rvalue, Statement, StatementKind, Terminator, TerminatorKind, UnaryOperator,
};
use types::{Integer, Primitive, Type, TypeKind};

/// Checks for overflow the arithmetic of `body` and its outlined `prange` bodies whose type
/// parameter this instance makes an integer, as lowering checks the same arithmetic written on
/// an integer type (decision 0030). `integers` lists those parameters and their integer types.
///
/// Lowering writes `+`, `-` and `*` on a type parameter, and `-x` on one, into a local of the
/// parameter's type, so the destination names the arithmetic to check. Each becomes the form
/// lowering writes for an integer: a [`CheckedBinary`](Rvalue::CheckedBinary) into a
/// `tuple[T, bool]` local, an `Assert` that its flag is `False`, and the result, in a new block
/// that continues the rest of the old one. `-x` is `0 - x`, checked only for a signed integer,
/// as lowering does. The rewrite runs before substitution, so the new locals name the parameter
/// and substitution gives them the instance's types with the rest.
pub(crate) fn checked_arithmetic<'db>(
    db: &'db dyn salsa::Database,
    body: &mut Body<'db>,
    integers: &[(Type<'db>, Integer)],
) {
    if integers.is_empty() {
        return;
    }
    for outlined in &mut body.outlined {
        checked_arithmetic(db, outlined, integers);
    }
    let boolean = Type::new(db, TypeKind::Primitive(Primitive::Bool));
    let mut block = 0;
    while block < body.blocks.len() {
        let found =
            body.blocks[block]
                .statements
                .iter()
                .enumerate()
                .find_map(|(index, statement)| {
                    checked_operation(&body.locals, integers, &statement.kind)
                        .map(|operation| (index, operation, statement.span))
                });
        let Some((index, (place, operator, (left, right), ty), span)) = found else {
            block += 1;
            continue;
        };
        let rest = body.blocks[block].statements.split_off(index + 1);
        body.blocks[block].statements.pop();
        let pair = body.push_local(
            Type::new(db, TypeKind::Tuple(vec![ty, boolean])),
            LocalKind::Temporary,
            span,
        );
        let fits = body.push_local(boolean, LocalKind::Temporary, span);
        let flag = Operand::Copy(Place::from(pair).project(Projection::Field(1)));
        let result = Operand::Copy(Place::from(pair).project(Projection::Field(0)));
        let checks = [
            (
                Place::from(pair),
                Rvalue::CheckedBinary(operator, left, right),
            ),
            (Place::from(fits), Rvalue::Unary(UnaryOperator::Not, flag)),
        ];
        let assign = |(place, rvalue)| Statement {
            kind: StatementKind::Assign(place, rvalue),
            span,
        };
        body.blocks[block].statements.extend(checks.map(assign));
        let next = body.push_block(body.blocks[block].terminator.clone());
        body.blocks[next.index()].statements =
            std::iter::once(assign((place, Rvalue::Use(result))))
                .chain(rest)
                .collect();
        body.blocks[block].terminator = Terminator {
            kind: TerminatorKind::Assert {
                condition: Operand::Copy(Place::from(fits)),
                check: Check::NoOverflow(operator),
                target: next,
            },
            span,
        };
        block += 1;
    }
}

/// An integer `+`, `-`, `*` or `-x` of a type parameter in `integers` that `statement` computes
/// without a check: its destination, operator, operands and type. `-x` is `0 - x`.
fn checked_operation<'db>(
    locals: &[LocalDeclaration<'db>],
    integers: &[(Type<'db>, Integer)],
    statement: &StatementKind<'db>,
) -> Option<(
    Place,
    BinaryOperator,
    (Operand<'db>, Operand<'db>),
    Type<'db>,
)> {
    let StatementKind::Assign(place, rvalue) = statement else {
        return None;
    };
    if !place.projections.is_empty() {
        return None;
    }
    let ty = locals.get(place.local.index())?.ty;
    let &(_, integer) = integers.iter().find(|(parameter, _)| *parameter == ty)?;
    let (operator, operands) = match rvalue {
        Rvalue::Binary(
            operator @ (BinaryOperator::Add | BinaryOperator::Subtract | BinaryOperator::Multiply),
            left,
            right,
        ) => (*operator, (left.clone(), right.clone())),
        Rvalue::Unary(UnaryOperator::Negate, operand) if integer.is_signed() => {
            let zero = Operand::Constant(Constant {
                ty,
                value: ConstantValue::Integer(0),
            });
            (BinaryOperator::Subtract, (zero, operand.clone()))
        }
        _ => return None,
    };
    Some((place.clone(), operator, operands, ty))
}
