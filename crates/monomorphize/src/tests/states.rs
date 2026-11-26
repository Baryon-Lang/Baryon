//! Tests of reading a generic state (card 60): `len(y)` and `y[i]` in an instance whose state
//! is a `Fixed` vector, an `f64` or an `Array`.

use super::{Database, named, span};
use crate::instantiate;
use mir::{
    Body, Constant, Local, LocalKind, Operand, Place, Projection, Rvalue, Statement, StatementKind,
    Terminator, TerminatorKind,
};
use types::{Float, Integer, Mode, Primitive, Type, TypeKind};

/// `def second[S: VectorSpace](y: S) -> f64` that reads `len(y)` and `y[1]`, as lowering
/// writes them, instantiated at `Fixed[f64, (3,)]`, `f64` and `Array[f64, 1]`: the `Fixed`
/// length is the constant 3, the `f64` length is 1 and its element is the number itself, and
/// the `Array` keeps both reads as they are.
#[test]
fn states_are_read_at_each_instance_type() {
    let database = Database::default();
    let (file, name) = named(&database, "S");
    let s = Type::new(&database, TypeKind::Parameter { file, name });
    let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    let i64 = Type::new(
        &database,
        TypeKind::Primitive(Primitive::Integer(Integer::I64)),
    );
    let vector = Type::new(
        &database,
        TypeKind::Fixed {
            element: f64,
            shape: types::Shape::new(vec![3]).unwrap(),
        },
    );
    let array = Type::new(
        &database,
        TypeKind::Array {
            element: f64,
            rank: 1,
        },
    );

    let mut body = Body::new(f64, span());
    let parameter = LocalKind::Parameter {
        name: "y".into(),
        mode: Mode::Borrow,
    };
    let y = body.push_local(s, parameter, span());
    let length = body.push_local(i64, LocalKind::Temporary, span());
    let position = body.push_local(i64, LocalKind::Temporary, span());
    let entry = body.push_block(Terminator {
        kind: TerminatorKind::Return,
        span: span(),
    });
    let assign = |place: Local, rvalue| Statement {
        kind: StatementKind::Assign(place.into(), rvalue),
        span: span(),
    };
    let element = Place::from(y).project(Projection::Index(vec![position]));
    body.blocks[entry.index()].statements = vec![
        assign(length, Rvalue::Length(y.into(), 0)),
        assign(
            position,
            Rvalue::Use(Operand::Constant(Constant::integer(i64, 1))),
        ),
        assign(Local::RETURN, Rvalue::Use(Operand::Copy(element))),
    ];

    let shown = |state| {
        let instance = instantiate(&database, &body, &[s], &[state]);
        let text = instance.display(&database).to_string();
        let reads: Vec<&str> = text.lines().filter(|line| line.contains(" = ")).collect();
        reads.join("\n")
    };
    assert_eq!(
        shown(vector),
        "        _2 = 3_i64;\n        _3 = 1_i64;\n        _0 = copy _1[_3];"
    );
    assert_eq!(
        shown(f64),
        "        _2 = 1_i64;\n        _3 = 1_i64;\n        _0 = copy _1;"
    );
    assert_eq!(
        shown(array),
        "        _2 = len(_1, 0);\n        _3 = 1_i64;\n        _0 = copy _1[_3];"
    );
}
