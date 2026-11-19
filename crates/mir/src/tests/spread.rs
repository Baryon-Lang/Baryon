//! Tests of `*args`, a tuple spread into a call's arguments in a generic body (the decision 0026
//! amendment).

use super::{Database, end, primitive, span};
use crate::{Argument, BlockId, Body, Callee, LocalKind, Operand, TerminatorKind};
use types::{Float, Mode, Primitive, Type, TypeKind};

/// `fun(t, *args)` writes the spread tuple with a star, and passes validation.
#[test]
fn a_spread_tuple_is_written_with_a_star() {
    let database = Database::default();
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let pair = Type::new(&database, TypeKind::Tuple(vec![f64, f64]));
    let callable = Type::new(
        &database,
        TypeKind::Callable {
            parameters: vec![f64, f64, f64],
            returns: f64,
        },
    );
    let mut body = Body::new(f64, span());
    let parameter = |name: &str| LocalKind::Parameter {
        name: name.into(),
        mode: Mode::Borrow,
    };
    let fun = body.push_local(callable, parameter("fun"), span());
    let t = body.push_local(f64, parameter("t"), span());
    let args = body.push_local(pair, parameter("args"), span());
    body.push_block(end(TerminatorKind::Call {
        callee: Callee::Value(Operand::Copy(fun.into())),
        arguments: vec![Argument::Borrow(t.into()), Argument::Spread(args.into())],
        destination: crate::Local::RETURN.into(),
        target: BlockId::new(1),
        error: None,
    }));
    body.push_block(end(TerminatorKind::Return));

    assert_eq!(body.validate(&database), Vec::new());
    assert!(
        body.display(&database)
            .to_string()
            .contains("_0 = call (copy _1)(_2, *_3) -> [return: bb1];"),
        "{}",
        body.display(&database)
    );
}
