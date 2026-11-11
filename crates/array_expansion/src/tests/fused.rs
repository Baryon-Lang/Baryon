//! Tests of expanding a whole-array expression that holds fused subexpressions: each one's
//! leaves are checked where its statement was, before the enclosing expression's, and the whole
//! subexpression counts as one array of its first leaf's shape.

use super::{Database, body_with, expanded, leaf};
use mir::{BinaryOperator, Body, Check, Elementwise, Operand, TerminatorKind};
use source::{FileId, Span};
use types::{Float, Mode, Primitive, Type, TypeKind};

/// The span of the statement at byte `start`; only the start matters to these tests.
fn at(start: u32) -> Span {
    Span::new(FileId::from_index(0), start, start + 1)
}

/// `left op right` as a tree.
fn binary<'db>(
    operator: BinaryOperator,
    left: Elementwise<'db>,
    right: Elementwise<'db>,
) -> Elementwise<'db> {
    Elementwise::Binary(operator, Box::new(left), Box::new(right))
}

/// `tree`, fused in from the statement at byte `start`.
fn fused(tree: Elementwise<'_>, start: u32) -> Elementwise<'_> {
    Elementwise::Fused(Box::new(tree), at(start))
}

/// A body returning `Array[f64, 1]` with four borrowed `Array[f64, 1]` parameters `a`, `b`, `c`
/// and `d` (locals 1 to 4), whose one statement assigns `tree` to the return value.
fn four_vectors<'db>(
    database: &'db Database,
    tree: impl FnOnce([Operand<'db>; 4]) -> Elementwise<'db>,
) -> Body<'db> {
    let f64 = Type::new(database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    let vector = Type::new(
        database,
        TypeKind::Array {
            element: f64,
            rank: 1,
        },
    );
    let parameters = [
        ("a", vector, Mode::Borrow),
        ("b", vector, Mode::Borrow),
        ("c", vector, Mode::Borrow),
        ("d", vector, Mode::Borrow),
    ];
    body_with(vector, &parameters, |locals| {
        tree([0, 1, 2, 3].map(|index| Operand::Copy(locals[index].into())))
    })
}

/// Each length check of `body`, in the order the blocks run, as the local it reads, the local it
/// compares with, and where it reports. Expansion writes blocks in the order they run.
fn length_checks(body: &Body<'_>) -> Vec<(String, String, u32)> {
    body.blocks
        .iter()
        .filter_map(|block| match &block.terminator.kind {
            TerminatorKind::Assert {
                check: Check::SameLength {
                    expected, found, ..
                },
                ..
            } => Some((
                format!("{expected:?}"),
                format!("{found:?}"),
                block.terminator.span.start(),
            )),
            _ => None,
        })
        .collect()
}

/// `fused(a * b) + c`, as `t = a * b` then `t + c` fuse: `b` is checked against `a` where
/// `t = a * b` was, then `c` against `a`, which stands for `t`, where the statement is. One
/// loop computes every element, and no array is built for `t`.
#[test]
fn a_fused_subexpression_checks_its_leaves_where_its_statement_was() {
    let database = Database::default();
    let body = four_vectors(&database, |[a, b, c, _]| {
        binary(
            BinaryOperator::Add,
            fused(binary(BinaryOperator::Multiply, leaf(a), leaf(b)), 10),
            leaf(c),
        )
    });
    let mut expanded_body = body.clone();
    crate::expand(&database, &mut expanded_body, &super::no_structs);
    let starts: Vec<u32> = length_checks(&expanded_body)
        .iter()
        .map(|check| check.2)
        .collect();
    assert_eq!(starts, [10, 0]);

    assert_eq!(
        expanded(&database, body),
        "\
fn(_1: Array[f64, 1], _2: Array[f64, 1], _3: Array[f64, 1], _4: Array[f64, 1]) -> Array[f64, 1] {
    let _0: Array[f64, 1]; // return
    let _1: Array[f64, 1]; // parameter a
    let _2: Array[f64, 1]; // parameter b
    let _3: Array[f64, 1]; // parameter c
    let _4: Array[f64, 1]; // parameter d
    let _5: i64;
    let _6: i64;
    let _7: bool;
    let _8: i64;
    let _9: i64;
    let _10: bool;
    let _11: Array[f64, 1];
    let _12: i64;
    let _13: bool;
    let _14: f64;
    let _15: f64;

    bb0: {
        _5 = len(_1, 0);
        _6 = len(_2, 0);
        _7 = Equal(copy _6, copy _5);
        assert(copy _7, same length in dimension 0: copy _6 == copy _5) -> bb1;
    }

    bb1: {
        _8 = len(_1, 0);
        _9 = len(_3, 0);
        _10 = Equal(copy _9, copy _8);
        assert(copy _10, same length in dimension 0: copy _9 == copy _8) -> bb2;
    }

    bb2: {
        _11 = new Array[f64, 1](copy _8);
        _12 = 0_i64;
        goto -> bb3;
    }

    bb3: {
        _13 = Less(copy _12, copy _8);
        branch copy _13 -> [true: bb4, false: bb5];
    }

    bb4: {
        _14 = Multiply(copy _1[_12], copy _2[_12]);
        _15 = Add(copy _14, copy _3[_12]);
        _11[_12] = copy _15;
        _12 = Add(copy _12, 1_i64);
        goto -> bb3;
    }

    bb5: {
        _0 = move _11;
        return;
    }
}
"
    );
}

/// `fused(c * d) + fused(a * b)`, where `t = a * b` ran before `u = c * d`: `t`'s leaves are
/// checked first, as the program ran them, though `u` comes first in the expression.
#[test]
fn fused_subexpressions_check_in_the_order_their_statements_ran() {
    let database = Database::default();
    let mut body = four_vectors(&database, |[a, b, c, d]| {
        binary(
            BinaryOperator::Add,
            fused(binary(BinaryOperator::Multiply, leaf(c), leaf(d)), 20),
            fused(binary(BinaryOperator::Multiply, leaf(a), leaf(b)), 10),
        )
    });
    crate::expand(&database, &mut body, &super::no_structs);

    let starts: Vec<u32> = length_checks(&body).iter().map(|check| check.2).collect();
    assert_eq!(starts, [10, 20, 0]);
}

/// `fused(fused(a * b) + c) * d`, as `t = a * b`, `u = t + c` and `u * d` fuse: three checks,
/// innermost first, and one loop.
#[test]
fn nested_fused_subexpressions_check_innermost_first() {
    let database = Database::default();
    let body = four_vectors(&database, |[a, b, c, d]| {
        let t = fused(binary(BinaryOperator::Multiply, leaf(a), leaf(b)), 10);
        let u = fused(binary(BinaryOperator::Add, t, leaf(c)), 20);
        binary(BinaryOperator::Multiply, u, leaf(d))
    });
    let mut expanded_body = body.clone();
    crate::expand(&database, &mut expanded_body, &super::no_structs);
    let starts: Vec<u32> = length_checks(&expanded_body)
        .iter()
        .map(|check| check.2)
        .collect();
    assert_eq!(starts, [10, 20, 0]);

    let text = expanded(&database, body);
    assert_eq!(text.matches("new Array").count(), 1, "{text}");
}
