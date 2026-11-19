//! Tests of the sums decision 0041's amendment fuses: an integer or `bool` sum of a whole-array
//! expression, and a sum of one along an axis, are written as loops over the expression and
//! never store it.

use super::{Database, expanded, leaf, span};
use mir::{
    Argument, BinaryOperator, BlockId, Body, Builtin, Callee, Constant, Elementwise, Local,
    LocalKind, Operand, Place, Rvalue, Statement, StatementKind, Terminator, TerminatorKind,
};
use types::{Float, Integer, Mode, Primitive, Type, TypeKind};

/// A body returning `left(a, b).sum()`, or `.sum(axis)` when `axis` is given, as lowering
/// writes it for arrays of `element` of rank `rank`: the expression into the temporary `_3`, a
/// call of `Sum` or `SumAlong` on it into the return value of type `returns`, and a drop of it
/// after.
fn summing<'db>(
    database: &'db Database,
    (element, rank): (Type<'db>, u32),
    operator: BinaryOperator,
    axis: Option<i128>,
    returns: Type<'db>,
) -> Body<'db> {
    let array = Type::new(database, TypeKind::Array { element, rank });
    let stored = match operator {
        BinaryOperator::Greater => Type::new(
            database,
            TypeKind::Array {
                element: Type::new(database, TypeKind::Primitive(Primitive::Bool)),
                rank,
            },
        ),
        _ => array,
    };
    let mut body = Body::new(returns, span());
    let parameters: Vec<Local> = ["a", "b"]
        .iter()
        .map(|name| {
            let kind = LocalKind::Parameter {
                name: (*name).into(),
                mode: Mode::Borrow,
            };
            body.push_local(array, kind, span())
        })
        .collect();
    let expression = body.push_local(stored, LocalKind::Temporary, span());
    let read = Argument::Borrow(Place::from(expression));
    let i64 = Type::new(
        database,
        TypeKind::Primitive(Primitive::Integer(Integer::I64)),
    );
    let (builtin, arguments) = match axis {
        None => (Builtin::Sum, vec![read]),
        Some(axis) => {
            let axis = Argument::Owned(Operand::Constant(Constant::integer(i64, axis)));
            (Builtin::SumAlong, vec![read, axis])
        }
    };
    let statement = |kind| Statement { kind, span: span() };
    let entry = body.push_block(Terminator {
        kind: TerminatorKind::Call {
            callee: Callee::Builtin(builtin),
            arguments,
            destination: Place::from(Local::RETURN),
            target: BlockId::new(1),
            error: None,
        },
        span: span(),
    });
    body.blocks[entry.index()]
        .statements
        .push(statement(StatementKind::Assign(
            Place::from(expression),
            Rvalue::Elementwise(Box::new(Elementwise::Binary(
                operator,
                Box::new(leaf(Operand::Copy(parameters[0].into()))),
                Box::new(leaf(Operand::Copy(parameters[1].into()))),
            ))),
        )));
    let exit = body.push_block(Terminator {
        kind: TerminatorKind::Return,
        span: span(),
    });
    body.blocks[exit.index()].statements =
        vec![statement(StatementKind::Drop(Place::from(expression)))];
    body
}

/// `(a * b).sum()` of integers keeps one running total with checked adds and never stores the
/// product or calls the pairwise tree.
#[test]
fn an_integer_sum_adds_each_value_with_a_check() {
    let database = Database::default();
    let i64 = Type::new(
        &database,
        TypeKind::Primitive(Primitive::Integer(Integer::I64)),
    );
    let body = summing(&database, (i64, 1), BinaryOperator::Multiply, None, i64);
    let text = expanded(&database, body);
    assert!(!text.contains("_3 ="), "{text}");
    assert!(!text.contains("drop(_3)"), "{text}");
    assert!(!text.contains("call sum"), "{text}");
    assert!(!text.contains("pairwise"), "{text}");
    assert!(text.contains("CheckedAdd("), "{text}");
    assert!(text.contains("CheckedMultiply(copy _1["), "{text}");
}

/// `(a > b).sum()` counts the `True` values: each `bool` is converted to `i64` and added.
#[test]
fn a_bool_sum_counts_in_i64() {
    let database = Database::default();
    let i64 = Type::new(
        &database,
        TypeKind::Primitive(Primitive::Integer(Integer::I64)),
    );
    let body = summing(&database, (i64, 1), BinaryOperator::Greater, None, i64);
    let text = expanded(&database, body);
    assert!(!text.contains("_3 ="), "{text}");
    assert!(text.contains("i64(copy"), "{text}");
    assert!(text.contains("CheckedAdd("), "{text}");
}

/// A float sum along the first axis starts a new array at 0 and adds each value into its
/// place, one at a time, without the pairwise tree.
#[test]
fn a_sum_along_the_first_axis_adds_into_place() {
    let database = Database::default();
    let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    let vector = Type::new(
        &database,
        TypeKind::Array {
            element: f64,
            rank: 1,
        },
    );
    let body = summing(
        &database,
        (f64, 2),
        BinaryOperator::Subtract,
        Some(0),
        vector,
    );
    let text = expanded(&database, body);
    assert!(!text.contains("_3 ="), "{text}");
    assert!(text.contains("new Array[f64, 1]("), "{text}");
    assert!(!text.contains("new Array[f64, 2]("), "{text}");
    assert!(!text.contains("call sum"), "{text}");
    assert!(text.contains("Subtract(copy _1["), "{text}");
}

/// A float sum along the last axis writes each row into a buffer one row long and sums it in
/// the pairwise tree, so the expression is never stored whole.
#[test]
fn a_sum_along_the_last_axis_sums_one_row_at_a_time() {
    let database = Database::default();
    let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    let vector = Type::new(
        &database,
        TypeKind::Array {
            element: f64,
            rank: 1,
        },
    );
    let body = summing(
        &database,
        (f64, 2),
        BinaryOperator::Subtract,
        Some(1),
        vector,
    );
    let text = expanded(&database, body);
    assert!(!text.contains("_3 ="), "{text}");
    assert!(!text.contains("new Array[f64, 2]("), "{text}");
    assert!(text.contains("call sum("), "{text}");
}

/// An axis that is not a constant within the rank leaves the call alone, so the expression is
/// stored as before.
#[test]
fn a_sum_along_an_axis_outside_the_array_is_left_alone() {
    let database = Database::default();
    let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    let vector = Type::new(
        &database,
        TypeKind::Array {
            element: f64,
            rank: 1,
        },
    );
    let body = summing(
        &database,
        (f64, 2),
        BinaryOperator::Subtract,
        Some(2),
        vector,
    );
    let mut body = body;
    super::super::expand(&database, &mut body, &super::no_structs);
    let text = body.display(&database).to_string();
    assert!(text.contains("call sum along an axis(_3"), "{text}");
}
