//! The command-line arguments, the environment and the exit status (decision 0036): each is a
//! call of the runtime, and `sys.exit` ends its block with `unreachable`.

use super::{Database, end, module, primitive, span};
use mir::{Argument, BlockId, Body, Builtin, Callee, Constant, ConstantValue, Operand};
use mir::{Local, LocalKind, TerminatorKind};
use types::{Integer, Primitive, Type, TypeKind};

/// The calls of a body that builds each built-in's call in turn, one block each, then returns.
fn calls<'db>(body: &mut Body<'db>, calls: Vec<(Builtin, Vec<Argument<'db>>, Local)>) {
    for (index, (builtin, arguments, destination)) in calls.into_iter().enumerate() {
        body.push_block(end(TerminatorKind::Call {
            callee: Callee::Builtin(builtin),
            arguments,
            destination: destination.into(),
            target: BlockId::new(index + 1),
            error: None,
        }));
    }
    body.push_block(end(TerminatorKind::Return));
}

/// `def last() -> str`: `sys.argv[len(sys.argv) - 1]` as lowering spells it, the count and then
/// the item, each a new owned `str` from the runtime; and `os.environ.get("HOME")`, which also
/// sets a `bool`.
#[test]
fn arguments_and_the_environment_are_runtime_calls() {
    let database = Database::default();
    let str = Type::new(&database, TypeKind::Str);
    let bool = primitive(&database, Primitive::Bool);
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let mut body = Body::new(str, span());
    let count = body.push_local(i64, LocalKind::Temporary, span());
    let found = body.push_local(bool, LocalKind::Temporary, span());
    let home = body.push_local(str, LocalKind::Temporary, span());
    let name = Operand::Constant(Constant {
        ty: str,
        value: ConstantValue::Str("HOME".to_owned()),
    });
    calls(
        &mut body,
        vec![
            (Builtin::ArgumentCount, vec![], count),
            (
                Builtin::Argument,
                vec![Argument::Owned(Operand::Constant(Constant::integer(
                    i64, 0,
                )))],
                Local::RETURN,
            ),
            (
                Builtin::EnvironmentGet,
                vec![Argument::Owned(name), Argument::InOut(found.into())],
                home,
            ),
        ],
    );
    assert_eq!(
        module(&database, &[("last", body)]),
        "\
define { ptr, i64 } @last() {
start:
  %_0 = alloca { ptr, i64 }
  %_1 = alloca i64
  %_2 = alloca i1
  %_3 = alloca { ptr, i64 }
  br label %bb0
bb0:
  %t0 = call i64 @baryon_rt_argument_count()
  store i64 %t0, ptr %_1
  br label %bb1
bb1:
  %t1 = call { ptr, i64 } @baryon_rt_argument(i64 0)
  store { ptr, i64 } %t1, ptr %_0
  br label %bb2
bb2:
  %t2 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775804 }, 0
  %t3 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775804 }, 1
  %t4 = and i64 %t3, 9223372036854775807
  %t5 = call { ptr, i64 } @baryon_rt_environment_get(ptr %t2, i64 %t4, ptr %_2)
  store { ptr, i64 } %t5, ptr %_3
  br label %bb3
bb3:
  %t6 = load { ptr, i64 }, ptr %_0
  ret { ptr, i64 } %t6
}

@baryon.str.0 = private unnamed_addr constant [4 x i8] c\"HOME\"

declare { ptr, i64 } @baryon_rt_argument(i64) nounwind
declare i64 @baryon_rt_argument_count() nounwind
declare { ptr, i64 } @baryon_rt_environment_get(ptr, i64, ptr) nounwind
"
    );
}

/// `sys.exit(3)`: the runtime never returns, so the block ends with `unreachable`.
#[test]
fn exit_never_returns() {
    let database = Database::default();
    let none = Type::new(&database, TypeKind::None);
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let mut body = Body::new(none, span());
    let status = Operand::Constant(Constant::integer(i64, 3));
    calls(
        &mut body,
        vec![(Builtin::Exit, vec![Argument::Owned(status)], Local::RETURN)],
    );
    assert_eq!(
        module(&database, &[("stop", body)]),
        "\
define void @stop() {
start:
  br label %bb0
bb0:
  call void @baryon_rt_exit(i64 3, ptr @baryon.loc.1)
  unreachable
bb1:
  ret void
}

@baryon.str.0 = private unnamed_addr constant [8 x i8] c\"heat.byn\"
@baryon.loc.1 = private unnamed_addr constant { ptr, i64, i32, i32 } { ptr @baryon.str.0, i64 8, i32 1, i32 1 }

declare void @baryon_rt_exit(i64, ptr) noreturn nounwind
"
    );
}

/// `os.environ.copy()` starts from the list of names and values the runtime writes to the
/// destination.
#[test]
fn the_environment_entries_are_written_by_the_runtime() {
    let database = Database::default();
    let str = Type::new(&database, TypeKind::Str);
    let list = Type::new(&database, TypeKind::List(str));
    let mut body = Body::new(list, span());
    calls(
        &mut body,
        vec![(Builtin::EnvironmentEntries, vec![], Local::RETURN)],
    );
    assert_eq!(
        module(&database, &[("entries", body)]),
        "\
define { ptr, i64, i64 } @entries() {
start:
  %_0 = alloca { ptr, i64, i64 }
  br label %bb0
bb0:
  call void @baryon_rt_environment_entries(ptr %_0)
  br label %bb1
bb1:
  %t0 = load { ptr, i64, i64 }, ptr %_0
  ret { ptr, i64, i64 } %t0
}

declare void @baryon_rt_environment_entries(ptr) nounwind
"
    );
}
