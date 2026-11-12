//! Tests of calls of built-in operations in the text form: by name, `math` functions by their
//! Python names, and `format` with its pieces.

use super::{Database, assign, end, primitive, span};
use crate::{
    Argument, BlockId, Body, Builtin, Callee, Constant, Local, LocalKind, MathFunction, Operand,
    Rvalue, TerminatorKind,
};
use types::{Float, Mode, Primitive, Type, TypeKind};

/// A call of the built-in `print` names it, with no item and no type arguments.
#[test]
fn builtins_are_called_by_name() {
    let database = Database::default();
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let none = Type::new(&database, TypeKind::None);
    let mut body = Body::new(none, span());
    let x = body.push_local(
        f64,
        LocalKind::Parameter {
            name: "x".into(),
            mode: Mode::Borrow,
        },
        span(),
    );
    let printed = body.push_local(none, LocalKind::Temporary, span());
    body.push_block(end(TerminatorKind::Call {
        callee: Callee::Builtin(Builtin::Print),
        arguments: vec![
            Argument::Borrow(x.into()),
            Argument::Owned(Operand::Constant(Constant::float(f64, 2.0))),
        ],
        destination: printed.into(),
        target: BlockId::new(1),
        error: None,
    }));
    body.push_block(end(TerminatorKind::Return));

    assert_eq!(body.validate(&database), Vec::new());
    assert!(
        body.display(&database)
            .to_string()
            .contains("_2 = call print(_1, owned 2.0_f64) -> [return: bb1];"),
        "{}",
        body.display(&database)
    );
}

/// `x ** 3.0` on floats is a call of the math built-in `pow`, written by its `math` name.
#[test]
fn math_builtins_are_called_by_their_math_names() {
    let database = Database::default();
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let mut body = Body::new(f64, span());
    let x = body.push_local(
        f64,
        LocalKind::Parameter {
            name: "x".into(),
            mode: Mode::Borrow,
        },
        span(),
    );
    body.push_block(end(TerminatorKind::Call {
        callee: Callee::Builtin(Builtin::Math(MathFunction::Power)),
        arguments: vec![
            Argument::Owned(Operand::Copy(x.into())),
            Argument::Owned(Operand::Constant(Constant::float(f64, 3.0))),
        ],
        destination: Local::RETURN.into(),
        target: BlockId::new(1),
        error: None,
    }));
    body.push_block(end(TerminatorKind::Return));

    assert_eq!(body.validate(&database), Vec::new());
    assert!(
        body.display(&database)
            .to_string()
            .contains("_0 = call math.pow(owned copy _1, owned 3.0_f64) -> [return: bb1];"),
        "{}",
        body.display(&database)
    );
}

/// An f-string's pieces are written in order, each value with its specification.
#[test]
fn format_writes_its_pieces() {
    let database = Database::default();
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let str = Type::new(&database, TypeKind::Str);
    let mut body = Body::new(str, span());
    let energy = body.push_local(
        f64,
        LocalKind::Parameter {
            name: "energy".into(),
            mode: Mode::Borrow,
        },
        span(),
    );
    let pieces = vec![
        crate::Piece::Text("energy ".to_owned()),
        crate::Piece::Value(energy.into(), types::FormatSpec::parse(">10.4f").unwrap()),
    ];
    body.push_block(end(TerminatorKind::Return));
    body.blocks[0].statements = vec![assign(Local::RETURN, Rvalue::Format(pieces))];
    assert_eq!(body.validate(&database), Vec::new());
    assert!(
        body.display(&database)
            .to_string()
            .contains("_0 = format(\"energy \", {_1: >10.4f});"),
        "{}",
        body.display(&database)
    );
}
