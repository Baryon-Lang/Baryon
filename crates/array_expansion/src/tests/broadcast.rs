//! Tests of broadcasting (roadmap 3.9): arrays of different ranks, `Fixed` arrays whose lengths
//! repeat when compiling, and expressions that cannot repeat and get the plain nest alone.

use super::{Database, body_with, expanded, leaf};
use mir::{BinaryOperator, Constant, Elementwise, Operand};
use types::{Float, Mode, Primitive, Shape, Type, TypeKind};

/// `grid + row`, a matrix and a vector: the vector lines up with the matrix's last dimension, so
/// it is indexed by the inner loop's index alone, and checked against the result's dimension 1.
#[test]
fn a_vector_lines_up_with_a_matrix_last_dimension() {
    let database = Database::default();
    let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    let array = |rank| Type::new(&database, TypeKind::Array { element: f64, rank });
    let body = body_with(
        array(2),
        &[
            ("grid", array(2), Mode::Borrow),
            ("row", array(1), Mode::Borrow),
        ],
        |locals| {
            Elementwise::Binary(
                BinaryOperator::Add,
                Box::new(leaf(Operand::Copy(locals[0].into()))),
                Box::new(leaf(Operand::Copy(locals[1].into()))),
            )
        },
    );
    let text = expanded(&database, body);
    assert!(text.contains("_5 = len(_2, 0);"), "{text}");
    assert!(
        text.contains("same length in dimension 1: copy _5 == copy _10)"),
        "{text}"
    );
    assert!(
        text.contains("new Array[f64, 2](copy _3, copy _10);"),
        "{text}"
    );
    // The plain nest, then the broadcasting one.
    assert!(
        text.contains("Add(copy _1[_20, _21], copy _2[_21]);"),
        "{text}"
    );
    assert!(
        text.contains("Add(copy _1[_25, _29], copy _2[_30]);"),
        "{text}"
    );
}

/// A `Fixed` column of shape `(2, 1)` and a `Fixed` row of shape `(3,)` make a `(2, 3)` table.
/// Every length is known when compiling: no length is read or checked, there is one nest, and
/// the column's second index and the row's missing first one are fixed at 0 and dropped.
#[test]
fn fixed_arrays_repeat_when_compiling() {
    let database = Database::default();
    let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    let fixed = |lengths: Vec<u64>| {
        Type::new(
            &database,
            TypeKind::Fixed {
                element: f64,
                shape: Shape::new(lengths).unwrap(),
            },
        )
    };
    let body = body_with(
        fixed(vec![2, 3]),
        &[
            ("column", fixed(vec![2, 1]), Mode::Borrow),
            ("row", fixed(vec![3]), Mode::Borrow),
        ],
        |locals| {
            Elementwise::Binary(
                BinaryOperator::Multiply,
                Box::new(leaf(Operand::Copy(locals[0].into()))),
                Box::new(leaf(Operand::Copy(locals[1].into()))),
            )
        },
    );
    let text = expanded(&database, body);
    assert!(!text.contains("len("), "{text}");
    assert!(!text.contains("assert("), "{text}");
    assert_eq!(text.matches("Less(").count(), 2, "{text}");
    assert!(text.contains("_3 = 0_i64;"), "{text}");
    assert!(
        text.contains("Multiply(copy _1[_5, _3], copy _2[_6]);"),
        "{text}"
    );
}

/// An expression of one array and numbers cannot repeat anything: it gets the plain nest alone,
/// with no flag and nothing multiplied.
#[test]
fn one_array_gets_the_plain_nest_alone() {
    let database = Database::default();
    let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    let array = Type::new(
        &database,
        TypeKind::Array {
            element: f64,
            rank: 2,
        },
    );
    let body = body_with(array, &[("a", array, Mode::Borrow)], |locals| {
        Elementwise::Binary(
            BinaryOperator::Multiply,
            Box::new(leaf(Operand::Copy(locals[0].into()))),
            Box::new(leaf(Operand::Constant(Constant::float(f64, 0.5)))),
        )
    });
    let text = expanded(&database, body);
    assert_eq!(text.matches("Less(").count(), 2, "{text}");
    assert!(!text.contains("i64(copy"), "{text}");
    assert!(!text.contains("assert("), "{text}");
}
