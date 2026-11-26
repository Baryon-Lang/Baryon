//! List literals: one of constants copies its elements from a table of them, so a long literal
//! does not make a basic block as long as itself (roadmap 1.9.74); any other stores each one.
//! The driver's build tests run both kinds.

use super::{Database, assign, end, parameter, primitive, span};
use mir::{Body, Constant, Local, Operand, Rvalue, StatementKind, TerminatorKind};
use types::{Integer, Mode, Primitive, Type, TypeKind};

/// `def f(x: i64) -> i64: xs = [<elements>]; return x`, each element a constant, or `x` itself
/// where `elements` holds `None`.
fn listed<'db>(database: &'db Database, elements: &[Option<i128>]) -> Body<'db> {
    let int = primitive(database, Primitive::Integer(Integer::I64));
    let list = Type::new(database, TypeKind::List(int));
    let mut body = Body::new(int, span());
    let x = body.push_local(int, parameter("x", Mode::Borrow), span());
    let xs = body.push_local(list, mir::LocalKind::Temporary, span());
    let operands = elements
        .iter()
        .map(|element| match element {
            Some(value) => Operand::Constant(Constant::integer(int, *value)),
            None => Operand::Copy(x.into()),
        })
        .collect();
    let block = body.push_block(end(TerminatorKind::Return));
    body.blocks[block.index()].statements = vec![
        assign(xs, Rvalue::Aggregate(list, operands)),
        assign(Local::RETURN, Rvalue::Use(Operand::Copy(x.into()))),
        mir::Statement {
            kind: StatementKind::Drop(xs.into()),
            span: span(),
        },
    ];
    body
}

/// A literal of constants is one table, written once however many literals hold the same
/// elements, and one copy of its 24 bytes; no element is stored on its own.
#[test]
fn a_literal_of_constants_copies_a_table() {
    let database = Database::default();
    let elements = [Some(3), Some(-1), Some(4)];
    let text = super::module(
        &database,
        &[
            ("first", listed(&database, &elements)),
            ("second", listed(&database, &elements)),
        ],
    );
    assert_eq!(
        text.matches("= private unnamed_addr constant [3 x i64] [i64 3, i64 -1, i64 4]\n")
            .count(),
        1,
        "{text}"
    );
    assert_eq!(text.matches(", i64 24, i1 false)\n").count(), 2, "{text}");
    assert!(!text.contains("store i64 3,"), "{text}");
}

/// A literal with an element that is not a constant stores each element, and makes no table.
#[test]
fn a_literal_with_a_variable_stores_each_element() {
    let database = Database::default();
    let text = super::module(&database, &[("mixed", listed(&database, &[Some(3), None]))]);
    assert!(text.contains("store i64 3, ptr "), "{text}");
    assert!(text.contains("store i64 %a1, ptr "), "{text}");
    assert!(!text.contains("@baryon.table."), "{text}");
}
