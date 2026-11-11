//! Tests of array expansion, compared by the full text form of the expanded body.

use crate::expand;
use database::{Db, File, Package, ParsedFile, Resolution};
use mir::{
    BinaryOperator, Body, Constant, Elementwise, Local, LocalKind, Operand, Phase, Place, Rvalue,
    Statement, StatementKind, Terminator, TerminatorKind,
};
use source::{FileId, Span};
use types::{Declared, Float, Integer, Mode, Nominal, Primitive, Shape, Type, TypeKind};

/// A database with no files: expansion never reads a name.
#[salsa::db]
#[derive(Clone, Default)]
struct Database {
    /// Salsa's inputs and caches.
    storage: salsa::Storage<Self>,
}

#[salsa::db]
impl salsa::Database for Database {}

#[salsa::db]
impl Db for Database {
    fn parse(&self, _: File) -> &ParsedFile {
        unreachable!("expansion never reads a file")
    }

    fn resolve(&self, _: Package, _: File) -> &Resolution {
        unreachable!("expansion never resolves names")
    }
}

/// The span every test statement carries; the text form does not show spans.
fn span() -> Span {
    Span::new(FileId::from_index(0), 0, 1)
}

/// A callback for bodies that read no struct fields.
fn no_structs<'db>(_: &Nominal<'db>) -> Declared<'db> {
    unreachable!("the body reads no fields")
}

/// A leaf.
fn leaf(operand: Operand<'_>) -> Elementwise<'_> {
    Elementwise::Leaf(operand)
}

/// A body returning `ty` whose one block assigns `tree` to the return value, with the given
/// borrowed and owned parameters.
fn body_with<'db>(
    ty: Type<'db>,
    parameters: &[(&str, Type<'db>, Mode)],
    tree: impl FnOnce(&[Local]) -> Elementwise<'db>,
) -> Body<'db> {
    let mut body = Body::new(ty, span());
    let locals: Vec<Local> = parameters
        .iter()
        .map(|&(name, ty, mode)| {
            body.push_local(
                ty,
                LocalKind::Parameter {
                    name: name.into(),
                    mode,
                },
                span(),
            )
        })
        .collect();
    let entry = body.push_block(Terminator {
        kind: TerminatorKind::Return,
        span: span(),
    });
    body.blocks[entry.index()].statements.push(Statement {
        kind: StatementKind::Assign(
            Local::RETURN.into(),
            Rvalue::Elementwise(Box::new(tree(&locals))),
        ),
        span: span(),
    });
    body
}

/// Expands, checks the result is a valid ready body, and returns its text form.
fn expanded<'db>(database: &'db Database, mut body: Body<'db>) -> String {
    expand(database, &mut body, &no_structs);
    body.phase = Phase::Ready;
    assert_eq!(body.validate(database), Vec::new());
    body.display(database).to_string()
}

/// The text form of `a * k + b` over rank-2 arrays once expanded, for the test below.
const RANK_TWO_EXPANDED: &str = "\
fn(_1: Array[f64, 2], _2: Array[f64, 2], _3: f64) -> Array[f64, 2] {
    let _0: Array[f64, 2]; // return
    let _1: Array[f64, 2]; // parameter a
    let _2: Array[f64, 2]; // parameter b
    let _3: f64; // parameter k
    let _4: i64;
    let _5: i64;
    let _6: i64;
    let _7: i64;
    let _8: bool;
    let _9: i64;
    let _10: i64;
    let _11: i64;
    let _12: i64;
    let _13: bool;
    let _14: i64;
    let _15: i64;
    let _16: i64;
    let _17: i64;
    let _18: bool;
    let _19: bool;
    let _20: bool;
    let _21: bool;
    let _22: bool;
    let _23: bool;
    let _24: bool;
    let _25: i64;
    let _26: bool;
    let _27: i64;
    let _28: bool;
    let _29: i64;
    let _30: bool;
    let _31: i64;
    let _32: Array[f64, 2];
    let _33: bool;
    let _34: bool;
    let _35: bool;
    let _36: i64;
    let _37: i64;
    let _38: bool;
    let _39: bool;
    let _40: f64;
    let _41: f64;
    let _42: i64;
    let _43: i64;
    let _44: bool;
    let _45: bool;
    let _46: i64;
    let _47: i64;
    let _48: f64;
    let _49: i64;
    let _50: i64;
    let _51: f64;

    bb0: {
        _4 = len(_1, 0);
        _5 = len(_1, 1);
        _6 = len(_2, 0);
        _7 = len(_2, 1);
        _8 = Equal(copy _4, 1_i64);
        _9 = i64(copy _8);
        _10 = Subtract(copy _6, copy _4);
        _11 = Multiply(copy _10, copy _9);
        _12 = Add(copy _4, copy _11);
        _13 = Equal(copy _5, 1_i64);
        _14 = i64(copy _13);
        _15 = Subtract(copy _7, copy _5);
        _16 = Multiply(copy _15, copy _14);
        _17 = Add(copy _5, copy _16);
        _18 = Equal(copy _6, copy _12);
        _19 = Equal(copy _6, 1_i64);
        _20 = BitOr(copy _18, copy _19);
        assert(copy _20, same length in dimension 0: copy _6 == copy _12) -> bb1;
    }

    bb1: {
        _21 = Equal(copy _7, copy _17);
        _22 = Equal(copy _7, 1_i64);
        _23 = BitOr(copy _21, copy _22);
        assert(copy _23, same length in dimension 1: copy _7 == copy _17) -> bb2;
    }

    bb2: {
        _24 = Equal(copy _4, copy _12);
        _25 = i64(copy _24);
        _26 = Equal(copy _5, copy _17);
        _27 = i64(copy _26);
        _28 = Equal(copy _6, copy _12);
        _29 = i64(copy _28);
        _30 = Equal(copy _7, copy _17);
        _31 = i64(copy _30);
        _32 = new Array[f64, 2](copy _12, copy _17);
        _33 = BitAnd(copy _24, copy _26);
        _34 = BitAnd(copy _33, copy _28);
        _35 = BitAnd(copy _34, copy _30);
        branch copy _35 -> [true: bb3, false: bb4];
    }

    bb3: {
        _36 = 0_i64;
        goto -> bb6;
    }

    bb4: {
        _42 = 0_i64;
        goto -> bb12;
    }

    bb5: {
        _0 = move _32;
        return;
    }

    bb6: {
        _38 = Less(copy _36, copy _12);
        branch copy _38 -> [true: bb7, false: bb8];
    }

    bb7: {
        _37 = 0_i64;
        goto -> bb9;
    }

    bb8: {
        goto -> bb5;
    }

    bb9: {
        _39 = Less(copy _37, copy _17);
        branch copy _39 -> [true: bb10, false: bb11];
    }

    bb10: {
        _40 = Multiply(copy _1[_36, _37], copy _3);
        _41 = Add(copy _40, copy _2[_36, _37]);
        _32[_36, _37] = copy _41;
        _37 = Add(copy _37, 1_i64);
        goto -> bb9;
    }

    bb11: {
        _36 = Add(copy _36, 1_i64);
        goto -> bb6;
    }

    bb12: {
        _44 = Less(copy _42, copy _12);
        branch copy _44 -> [true: bb13, false: bb14];
    }

    bb13: {
        _43 = 0_i64;
        goto -> bb15;
    }

    bb14: {
        goto -> bb5;
    }

    bb15: {
        _45 = Less(copy _43, copy _17);
        branch copy _45 -> [true: bb16, false: bb17];
    }

    bb16: {
        _46 = Multiply(copy _42, copy _25);
        _47 = Multiply(copy _43, copy _27);
        _48 = Multiply(copy _1[_46, _47], copy _3);
        _49 = Multiply(copy _42, copy _29);
        _50 = Multiply(copy _43, copy _31);
        _51 = Add(copy _48, copy _2[_49, _50]);
        _32[_42, _43] = copy _51;
        _43 = Add(copy _43, 1_i64);
        goto -> bb15;
    }

    bb17: {
        _42 = Add(copy _42, 1_i64);
        goto -> bb12;
    }
}
";

/// `a * k + b` over rank-2 arrays: the lengths broadcast from `a`'s and `b`'s, `b` checked to fit
/// them in both dimensions, the result allocated with them, the plain nest of two loops and the
/// broadcasting nest, and the result moved out after the loops.
#[test]
fn rank_two_arrays_expand_to_a_checked_loop_nest() {
    let database = Database::default();
    let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    let grid = Type::new(
        &database,
        TypeKind::Array {
            element: f64,
            rank: 2,
        },
    );
    let body = body_with(
        grid,
        &[
            ("a", grid, Mode::Borrow),
            ("b", grid, Mode::Borrow),
            ("k", f64, Mode::Borrow),
        ],
        |locals| {
            Elementwise::Binary(
                BinaryOperator::Add,
                Box::new(Elementwise::Binary(
                    BinaryOperator::Multiply,
                    Box::new(leaf(Operand::Copy(locals[0].into()))),
                    Box::new(leaf(Operand::Copy(locals[2].into()))),
                )),
                Box::new(leaf(Operand::Copy(locals[1].into()))),
            )
        },
    );

    assert_eq!(expanded(&database, body), RANK_TWO_EXPANDED);
}

/// `v * 2.0` over a `Fixed[f64, (3,)]` needs no lengths read, no check and no allocation: the
/// type fixes the shape and the result is stored inline.
#[test]
fn fixed_arrays_use_their_type_lengths() {
    let database = Database::default();
    let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    let vec3 = Type::new(
        &database,
        TypeKind::Fixed {
            element: f64,
            shape: Shape::new(vec![3]).unwrap(),
        },
    );
    let body = body_with(vec3, &[("v", vec3, Mode::Borrow)], |locals| {
        Elementwise::Binary(
            BinaryOperator::Multiply,
            Box::new(leaf(Operand::Copy(locals[0].into()))),
            Box::new(leaf(Operand::Constant(Constant::float(f64, 2.0)))),
        )
    });

    let text = expanded(&database, body);
    assert!(!text.contains("len("), "{text}");
    assert!(!text.contains("new "), "{text}");
    assert!(text.contains("_4 = Less(copy _3, 3_i64);"), "{text}");
    assert!(text.contains("_2[_3] = copy _5;"), "{text}");
}

/// An array the expression moves is dropped once the loop has read it; the result is built in a
/// temporary, so the destination may be one of the operands.
#[test]
fn moved_arrays_are_dropped_after_the_loop() {
    let database = Database::default();
    let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    let array = Type::new(
        &database,
        TypeKind::Array {
            element: f64,
            rank: 1,
        },
    );
    let body = body_with(array, &[("a", array, Mode::Owned)], |locals| {
        Elementwise::Unary(
            mir::UnaryOperator::Negate,
            Box::new(leaf(Operand::Move(Place::from(locals[0])))),
        )
    });

    let text = expanded(&database, body);
    assert!(
        text.contains("        _0 = move _3;\n        drop(_1);\n        return;"),
        "{text}"
    );
}

/// `v * 2` over a `Fixed[i64, (2,)]` checks each element's product for overflow as integer `*`
/// is (decision 0030): a `CheckedMultiply` and an `Assert` inside the loop, with the wrapped
/// result stored only after the check.
#[test]
fn integer_arithmetic_is_checked_for_overflow() {
    let database = Database::default();
    let i64 = Type::new(
        &database,
        TypeKind::Primitive(Primitive::Integer(Integer::I64)),
    );
    let pair = Type::new(
        &database,
        TypeKind::Fixed {
            element: i64,
            shape: Shape::new(vec![2]).unwrap(),
        },
    );
    let body = body_with(pair, &[("v", pair, Mode::Borrow)], |locals| {
        Elementwise::Binary(
            BinaryOperator::Multiply,
            Box::new(leaf(Operand::Copy(locals[0].into()))),
            Box::new(leaf(Operand::Constant(Constant::integer(i64, 2)))),
        )
    });

    let text = expanded(&database, body);
    assert!(
        text.contains("_5 = CheckedMultiply(copy _1[_3], 2_i64);"),
        "{text}"
    );
    assert!(text.contains("_6 = Not(copy _5.1);"), "{text}");
    assert!(
        text.contains("assert(copy _6, no overflow: Multiply) -> bb4;"),
        "{text}"
    );
    assert!(text.contains("_2[_3] = copy _5.0;"), "{text}");
}

mod broadcast;
mod fused;
mod views;
