//! Tests of one-pass sums (decision 0041): `(a * b).sum()` writes each element into a buffer
//! of one pairwise leaf and never stores the product, unless something else names it.

use super::{Database, expanded, leaf, span};
use mir::{
    Argument, BinaryOperator, BlockId, Body, Builtin, Callee, Elementwise, Local, LocalKind,
    Operand, Place, Rvalue, Statement, StatementKind, Terminator, TerminatorKind,
};
use types::{Float, Integer, Mode, Primitive, Type, TypeKind};

/// A body returning `(a * b).sum()` as lowering writes it: the product into the temporary
/// `_3`, a call of `sum` on it, and a drop of it after.
fn summing(database: &Database) -> Body<'_> {
    let f64 = Type::new(database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    let vector = Type::new(
        database,
        TypeKind::Array {
            element: f64,
            rank: 1,
        },
    );
    let mut body = Body::new(f64, span());
    let parameters: Vec<Local> = ["a", "b"]
        .iter()
        .map(|name| {
            body.push_local(
                vector,
                LocalKind::Parameter {
                    name: (*name).into(),
                    mode: Mode::Borrow,
                },
                span(),
            )
        })
        .collect();
    let product = body.push_local(vector, LocalKind::Temporary, span());
    let statement = |kind| Statement { kind, span: span() };
    let entry = body.push_block(Terminator {
        kind: TerminatorKind::Call {
            callee: Callee::Builtin(Builtin::Sum),
            arguments: vec![Argument::Borrow(Place::from(product))],
            destination: Place::from(Local::RETURN),
            target: BlockId::new(1),
            error: None,
        },
        span: span(),
    });
    body.blocks[entry.index()]
        .statements
        .push(statement(StatementKind::Assign(
            Place::from(product),
            Rvalue::Elementwise(Box::new(Elementwise::Binary(
                BinaryOperator::Multiply,
                Box::new(leaf(Operand::Copy(parameters[0].into()))),
                Box::new(leaf(Operand::Copy(parameters[1].into()))),
            ))),
        )));
    let exit = body.push_block(Terminator {
        kind: TerminatorKind::Return,
        span: span(),
    });
    body.blocks[exit.index()].statements =
        vec![statement(StatementKind::Drop(Place::from(product)))];
    body
}

/// The product is never stored: its elements go into the leaf buffer, which is summed one
/// leaf at a time, and the runtime combines the leaf sums into the return value.
#[test]
fn a_summed_expression_is_never_stored() {
    let database = Database::default();
    let text = expanded(&database, summing(&database));
    assert!(!text.contains("drop(_3)"), "{text}");
    assert!(!text.contains("_3 ="), "{text}");
    assert!(text.contains("call pairwise leaves("), "{text}");
    assert!(text.contains("Multiply(copy _1["), "{text}");
    assert!(text.contains("_0 = call pairwise combine("), "{text}");
}

/// A product that something else also reads is stored and summed as before.
#[test]
fn a_product_read_elsewhere_is_stored() {
    let database = Database::default();
    let i64 = Type::new(
        &database,
        TypeKind::Primitive(Primitive::Integer(Integer::I64)),
    );
    let mut body = summing(&database);
    let length = body.push_local(i64, LocalKind::Temporary, span());
    body.blocks[1].statements.insert(
        0,
        Statement {
            kind: StatementKind::Assign(
                Place::from(length),
                Rvalue::Length(Place::from(Local::new(3)), 0),
            ),
            span: span(),
        },
    );
    let text = expanded(&database, body);
    assert!(text.contains("drop(_3)"), "{text}");
    assert!(text.contains("_0 = call sum(_3)"), "{text}");
    assert!(!text.contains("pairwise"), "{text}");
}
