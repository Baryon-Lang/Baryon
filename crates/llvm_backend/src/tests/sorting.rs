//! `sorted` and `round(x, digits)` compile to calls of the runtime (roadmap 1.13.13). The
//! runtime's own tests check what the calls compute, and the driver's build tests run them.

use super::{Database, assign, end, parameter, primitive, span};
use mir::{Body, Local, Operand, Rvalue, StatementKind, TerminatorKind};
use types::{Float, Integer, Mode, Primitive, Type, TypeKind};

/// A body returning an `i64`: builds the list `elements`, sorts it from the largest when
/// `reverse`, pops its last element into the return value and drops the list.
fn sorted_last<'db>(database: &'db Database, elements: &[i128], reverse: bool) -> Body<'db> {
    let int = primitive(database, Primitive::Integer(Integer::I64));
    let none = Type::new(database, TypeKind::None);
    let list = Type::new(database, TypeKind::List(int));
    let mut body = Body::new(int, span());
    let xs = body.push_local(list, mir::LocalKind::Temporary, span());
    let nothing = body.push_local(none, mir::LocalKind::Temporary, span());
    let number = |value: i128| Operand::Constant(mir::Constant::integer(int, value));
    let flag = Operand::Constant(mir::Constant {
        ty: primitive(database, Primitive::Bool),
        value: mir::ConstantValue::Bool(reverse),
    });
    let sort = body.push_block(end(TerminatorKind::Call {
        callee: mir::Callee::Builtin(mir::Builtin::Sort),
        arguments: vec![mir::Argument::InOut(xs.into()), mir::Argument::Owned(flag)],
        destination: nothing.into(),
        target: mir::BlockId::new(1),
        error: None,
    }));
    body.blocks[sort.index()].statements = vec![assign(
        xs,
        Rvalue::Aggregate(list, elements.iter().copied().map(number).collect()),
    )];
    body.push_block(end(TerminatorKind::Call {
        callee: mir::Callee::Builtin(mir::Builtin::Pop),
        arguments: vec![mir::Argument::InOut(xs.into())],
        destination: Local::RETURN.into(),
        target: mir::BlockId::new(2),
        error: None,
    }));
    let finish = body.push_block(end(TerminatorKind::Return));
    body.blocks[finish.index()].statements = vec![mir::Statement {
        kind: StatementKind::Drop(xs.into()),
        span: span(),
    }];
    body
}

/// `def f(x: T, digits: i64) -> T: return round(x, digits)`.
fn rounded<'db>(ty: Type<'db>, int: Type<'db>) -> Body<'db> {
    let mut body = Body::new(ty, span());
    let x = body.push_local(ty, parameter("x", Mode::Borrow), span());
    let digits = body.push_local(int, parameter("digits", Mode::Borrow), span());
    body.push_block(end(TerminatorKind::Call {
        callee: mir::Callee::Builtin(mir::Builtin::RoundDigits),
        arguments: vec![
            mir::Argument::Owned(Operand::Copy(x.into())),
            mir::Argument::Owned(Operand::Copy(digits.into())),
        ],
        destination: Local::RETURN.into(),
        target: mir::BlockId::new(1),
        error: None,
    }));
    body.push_block(end(TerminatorKind::Return));
    body
}

/// A sort passes the list's element pointer and length, the runtime's number for the element
/// type, `i64` here, and the direction; a list of `str`s goes to its own function.
#[test]
fn sort_calls_the_runtime_with_the_element_type() {
    let database = Database::default();
    let text = super::module(
        &database,
        &[("up", sorted_last(&database, &[3, 1, 2], false))],
    );
    assert!(text.contains(", i32 4, i1 zeroext false)\n"), "{text}");
    assert!(
        text.contains("declare void @baryon_rt_sort(ptr, i64, i32, i1 zeroext) nounwind"),
        "{text}"
    );
}

/// `round(x, digits)` of an `f64` calls `baryon_rt_round_f64`, and of an `i64` calls
/// `baryon_rt_round_i64` with the call's location, for its overflow panic.
#[test]
fn round_digits_calls_the_runtime_for_its_type() {
    let database = Database::default();
    let int = primitive(&database, Primitive::Integer(Integer::I64));
    let float = primitive(&database, Primitive::Float(Float::F64));
    let real = super::module(&database, &[("real", rounded(float, int))]);
    assert!(
        real.contains("= call double @baryon_rt_round_f64(double %t0, i64 %t1)\n"),
        "{real}"
    );
    assert!(real.contains("declare double @baryon_rt_round_f64(double, i64) nounwind"));
    let whole = super::module(&database, &[("whole", rounded(int, int))]);
    assert!(
        whole.contains("= call i64 @baryon_rt_round_i64(i64 %t0, i64 %t1, ptr @baryon.loc.1)\n"),
        "{whole}"
    );
    assert!(whole.contains("declare i64 @baryon_rt_round_i64(i64, i64, ptr) nounwind"));
}
