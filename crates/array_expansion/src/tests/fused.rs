//! Tests of expanding a whole-array expression that holds fused subexpressions: each one's
//! leaves are checked where its statement was, before the enclosing expression's, and the whole
//! subexpression counts as one array of the shape its leaves broadcast to.

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

/// The text form of `fused(a * b) + c` once expanded, for the test below.
const FUSED_EXPANDED: &str = "\
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
    let _10: i64;
    let _11: i64;
    let _12: bool;
    let _13: bool;
    let _14: bool;
    let _15: bool;
    let _16: i64;
    let _17: bool;
    let _18: i64;
    let _19: i64;
    let _20: bool;
    let _21: i64;
    let _22: i64;
    let _23: i64;
    let _24: i64;
    let _25: bool;
    let _26: bool;
    let _27: bool;
    let _28: bool;
    let _29: i64;
    let _30: bool;
    let _31: i64;
    let _32: Array[f64, 1];
    let _33: bool;
    let _34: bool;
    let _35: bool;
    let _36: i64;
    let _37: bool;
    let _38: f64;
    let _39: f64;
    let _40: i64;
    let _41: bool;
    let _42: i64;
    let _43: i64;
    let _44: i64;
    let _45: f64;
    let _46: i64;
    let _47: f64;

    bb0: {
        _5 = len(_1, 0);
        _6 = len(_2, 0);
        _7 = Equal(copy _5, 1_i64);
        _8 = i64(copy _7);
        _9 = Subtract(copy _6, copy _5);
        _10 = Multiply(copy _9, copy _8);
        _11 = Add(copy _5, copy _10);
        _12 = Equal(copy _6, copy _11);
        _13 = Equal(copy _6, 1_i64);
        _14 = BitOr(copy _12, copy _13);
        assert(copy _14, same length in dimension 0: copy _6 == copy _11) -> bb1;
    }

    bb1: {
        _15 = Equal(copy _5, copy _11);
        _16 = i64(copy _15);
        _17 = Equal(copy _6, copy _11);
        _18 = i64(copy _17);
        _19 = len(_3, 0);
        _20 = Equal(copy _11, 1_i64);
        _21 = i64(copy _20);
        _22 = Subtract(copy _19, copy _11);
        _23 = Multiply(copy _22, copy _21);
        _24 = Add(copy _11, copy _23);
        _25 = Equal(copy _19, copy _24);
        _26 = Equal(copy _19, 1_i64);
        _27 = BitOr(copy _25, copy _26);
        assert(copy _27, same length in dimension 0: copy _19 == copy _24) -> bb2;
    }

    bb2: {
        _28 = Equal(copy _11, copy _24);
        _29 = i64(copy _28);
        _30 = Equal(copy _19, copy _24);
        _31 = i64(copy _30);
        _32 = new Array[f64, 1](copy _24);
        _33 = BitAnd(copy _15, copy _17);
        _34 = BitAnd(copy _33, copy _28);
        _35 = BitAnd(copy _34, copy _30);
        branch copy _35 -> [true: bb3, false: bb4];
    }

    bb3: {
        _36 = 0_i64;
        goto -> bb6;
    }

    bb4: {
        _40 = 0_i64;
        goto -> bb9;
    }

    bb5: {
        _0 = move _32;
        return;
    }

    bb6: {
        _37 = Less(copy _36, copy _24);
        branch copy _37 -> [true: bb7, false: bb8];
    }

    bb7: {
        _38 = Multiply(copy _1[_36], copy _2[_36]);
        _39 = Add(copy _38, copy _3[_36]);
        _32[_36] = copy _39;
        _36 = Add(copy _36, 1_i64);
        goto -> bb6;
    }

    bb8: {
        goto -> bb5;
    }

    bb9: {
        _41 = Less(copy _40, copy _24);
        branch copy _41 -> [true: bb10, false: bb11];
    }

    bb10: {
        _42 = Multiply(copy _40, copy _29);
        _43 = Multiply(copy _42, copy _16);
        _44 = Multiply(copy _42, copy _18);
        _45 = Multiply(copy _1[_43], copy _2[_44]);
        _46 = Multiply(copy _40, copy _31);
        _47 = Add(copy _45, copy _3[_46]);
        _32[_40] = copy _47;
        _40 = Add(copy _40, 1_i64);
        goto -> bb9;
    }

    bb11: {
        goto -> bb5;
    }
}
";

/// `fused(a * b) + c`, as `t = a * b` then `t + c` fuse: `b` is checked against the length `a`
/// and `b` broadcast to where `t = a * b` was, then `c` against the length `t` and `c` broadcast
/// to, where the statement is. One
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

    assert_eq!(expanded(&database, body), FUSED_EXPANDED);
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
