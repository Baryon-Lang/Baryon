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

/// `a * k + b` over rank-2 arrays: one nest of two loops, `b`'s lengths checked against `a`'s in
/// both dimensions, the result allocated with `a`'s lengths and moved out after the loops.
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

    assert_eq!(
        expanded(&database, body),
        "\
fn(_1: Array[f64, 2], _2: Array[f64, 2], _3: f64) -> Array[f64, 2] {
    let _0: Array[f64, 2]; // return
    let _1: Array[f64, 2]; // parameter a
    let _2: Array[f64, 2]; // parameter b
    let _3: f64; // parameter k
    let _4: i64;
    let _5: i64;
    let _6: i64;
    let _7: bool;
    let _8: i64;
    let _9: bool;
    let _10: Array[f64, 2];
    let _11: i64;
    let _12: i64;
    let _13: bool;
    let _14: bool;
    let _15: f64;
    let _16: f64;

    bb0: {
        _4 = len(_1, 0);
        _5 = len(_1, 1);
        _6 = len(_2, 0);
        _7 = Equal(copy _6, copy _4);
        assert(copy _7, same length in dimension 0: copy _6 == copy _4) -> bb1;
    }

    bb1: {
        _8 = len(_2, 1);
        _9 = Equal(copy _8, copy _5);
        assert(copy _9, same length in dimension 1: copy _8 == copy _5) -> bb2;
    }

    bb2: {
        _10 = new Array[f64, 2](copy _4, copy _5);
        _11 = 0_i64;
        goto -> bb3;
    }

    bb3: {
        _13 = Less(copy _11, copy _4);
        branch copy _13 -> [true: bb4, false: bb5];
    }

    bb4: {
        _12 = 0_i64;
        goto -> bb6;
    }

    bb5: {
        _0 = move _10;
        return;
    }

    bb6: {
        _14 = Less(copy _12, copy _5);
        branch copy _14 -> [true: bb7, false: bb8];
    }

    bb7: {
        _15 = Multiply(copy _1[_11, _12], copy _3);
        _16 = Add(copy _15, copy _2[_11, _12]);
        _10[_11, _12] = copy _16;
        _12 = Add(copy _12, 1_i64);
        goto -> bb6;
    }

    bb8: {
        _11 = Add(copy _11, 1_i64);
        goto -> bb3;
    }
}
"
    );
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
