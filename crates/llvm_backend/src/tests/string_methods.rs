//! `str`'s methods (roadmap 1.13.12): each is one call of its runtime function, with each `str`
//! passed as its bytes and their count, the "not owned" bit masked off.

use super::{Database, end, module, parameter, span};
use mir::{Argument, BlockId, Body, Builtin, Callee, Constant, ConstantValue, Operand};
use mir::{Local, LocalKind, TerminatorKind, TextMethod};
use types::{Integer, Mode, Primitive, Type, TypeKind};

/// `def fields(line: str) -> list[str]: return line.split(",")`: the separator's length is
/// checked first, an empty one panics as Python's `ValueError` would, then the runtime writes
/// the new list straight into the return value's header.
#[test]
fn split_checks_the_separator_and_writes_the_list() {
    let database = Database::default();
    let str = Type::new(&database, TypeKind::Str);
    let list = Type::new(&database, TypeKind::List(str));
    let mut body = Body::new(list, span());
    let line = body.push_local(str, parameter("line", Mode::Borrow), span());
    let comma = Operand::Constant(Constant {
        ty: str,
        value: ConstantValue::Str(",".to_owned()),
    });
    body.push_block(end(TerminatorKind::Call {
        callee: Callee::Builtin(Builtin::Text(TextMethod::Split)),
        arguments: vec![Argument::Borrow(line.into()), Argument::Owned(comma)],
        destination: Local::RETURN.into(),
        target: BlockId::new(1),
        error: None,
    }));
    body.push_block(end(TerminatorKind::Return));
    let written = module(&database, &[("fields", body)]);
    for expected in [
        "  %t7 = icmp ne i64 %t6, 0\n",
        "  call void @baryon_rt_str_split(ptr %t1, i64 %t3, ptr %t4, i64 %t6, ptr %_0)\n",
        "declare void @baryon_rt_str_split(ptr, i64, ptr, i64, ptr) nounwind\n",
        "empty separator",
    ] {
        assert!(
            written.contains(expected),
            "{expected} missing from\n{written}"
        );
    }
}

/// `def middle(name: str, start: int, count: int) -> str: return name[start:start + count]`,
/// reduced to its call: the text, then three `i64` positions; the runtime returns a new owned
/// `str`, stored as it is.
#[test]
fn take_passes_positions_and_returns_owned_text() {
    let database = Database::default();
    let str = Type::new(&database, TypeKind::Str);
    let int = Type::new(
        &database,
        TypeKind::Primitive(Primitive::Integer(Integer::I64)),
    );
    let mut body = Body::new(str, span());
    let name = body.push_local(str, parameter("name", Mode::Borrow), span());
    let start = body.push_local(int, parameter("start", Mode::Owned), span());
    let count = body.push_local(int, LocalKind::Temporary, span());
    let one = Operand::Constant(Constant::integer(int, 1));
    body.push_block(end(TerminatorKind::Call {
        callee: Callee::Builtin(Builtin::Text(TextMethod::Take)),
        arguments: vec![
            Argument::Borrow(name.into()),
            Argument::Owned(Operand::Copy(start.into())),
            Argument::Owned(Operand::Copy(count.into())),
            Argument::Owned(one),
        ],
        destination: Local::RETURN.into(),
        target: BlockId::new(1),
        error: None,
    }));
    body.push_block(end(TerminatorKind::Return));
    let written = module(&database, &[("middle", body)]);
    for expected in [
        "call { ptr, i64 } @baryon_rt_str_take(ptr %t1, i64 %t3, i64 %t4, i64 %t5, i64 1)\n",
        "declare { ptr, i64 } @baryon_rt_str_take(ptr, i64, i64, i64, i64) nounwind\n",
    ] {
        assert!(
            written.contains(expected),
            "{expected} missing from\n{written}"
        );
    }
}
