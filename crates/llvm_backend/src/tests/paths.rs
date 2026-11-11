//! Paths and directories (decision 0036): each path method is a call of the runtime; the ones
//! that can fail raise through the error edge's local, as the file calls do.

use super::{Database, end, module, primitive, span};
use mir::{Argument, BlockId, Body, Builtin, Callee, Constant, ConstantValue, ErrorEdge};
use mir::{LocalKind, Operand, PathMethod, TerminatorKind};
use types::{Integer, Primitive, Type, TypeKind};

/// `path = Path("runs") / name`, `path.suffix`, `path.is_dir()`, `os.makedirs(path,
/// exist_ok=True)`, `os.listdir(path)` and `os.getcwd()`, one block each. A failure of the last
/// three goes to the last block with the error in its local. `i64` stands for `OSError`, since
/// the backend only passes the local's address.
#[test]
fn path_methods_are_runtime_calls_and_the_fallible_ones_branch() {
    let database = Database::default();
    let body = path_calls(&database);
    assert_eq!(module(&database, &[("paths", body)]),
        "\
define void @paths() {
start:
  %_1 = alloca { ptr, i64 }
  %_2 = alloca { ptr, i64 }
  %_3 = alloca { ptr, i64 }
  %_4 = alloca i1
  %_6 = alloca { ptr, i64, i64 }
  %_7 = alloca { ptr, i64 }
  %_8 = alloca { i8, [1 x i64] }
  br label %bb0
bb0:
  %t0 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775804 }, 0
  %t1 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775804 }, 1
  %t2 = and i64 %t1, 9223372036854775807
  %t3 = load { ptr, i64 }, ptr %_1
  %t4 = extractvalue { ptr, i64 } %t3, 0
  %t5 = extractvalue { ptr, i64 } %t3, 1
  %t6 = and i64 %t5, 9223372036854775807
  %t7 = call { ptr, i64 } @baryon_rt_path_join(ptr %t0, i64 %t2, ptr %t4, i64 %t6)
  store { ptr, i64 } %t7, ptr %_2
  br label %bb1
bb1:
  %t8 = load { ptr, i64 }, ptr %_2
  %t9 = extractvalue { ptr, i64 } %t8, 0
  %t10 = extractvalue { ptr, i64 } %t8, 1
  %t11 = and i64 %t10, 9223372036854775807
  %t12 = call { ptr, i64 } @baryon_rt_path_part(ptr %t9, i64 %t11, i8 zeroext 3)
  store { ptr, i64 } %t12, ptr %_3
  br label %bb2
bb2:
  %t13 = load { ptr, i64 }, ptr %_2
  %t14 = extractvalue { ptr, i64 } %t13, 0
  %t15 = extractvalue { ptr, i64 } %t13, 1
  %t16 = and i64 %t15, 9223372036854775807
  %t17 = call zeroext i1 @baryon_rt_path_is(ptr %t14, i64 %t16, i8 zeroext 2)
  store i1 %t17, ptr %_4
  br label %bb3
bb3:
  %t18 = load { ptr, i64 }, ptr %_2
  %t19 = extractvalue { ptr, i64 } %t18, 0
  %t20 = extractvalue { ptr, i64 } %t18, 1
  %t21 = and i64 %t20, 9223372036854775807
  %t22 = call zeroext i1 @baryon_rt_path_make_directory(ptr %t19, i64 %t21, i1 zeroext true, i1 zeroext true, ptr %_8)
  br i1 %t22, label %bb7, label %bb4
bb4:
  %t23 = load { ptr, i64 }, ptr %_2
  %t24 = extractvalue { ptr, i64 } %t23, 0
  %t25 = extractvalue { ptr, i64 } %t23, 1
  %t26 = and i64 %t25, 9223372036854775807
  %t27 = call zeroext i1 @baryon_rt_path_list(ptr %t24, i64 %t26, i1 zeroext false, ptr %_6, ptr %_8)
  br i1 %t27, label %bb7, label %bb5
bb5:
  %t28 = call zeroext i1 @baryon_rt_current_directory(ptr %_7, ptr %_8)
  br i1 %t28, label %bb7, label %bb6
bb6:
  ret void
bb7:
  ret void
}

@baryon.str.0 = private unnamed_addr constant [4 x i8] c\"runs\"

declare zeroext i1 @baryon_rt_current_directory(ptr, ptr) nounwind
declare zeroext i1 @baryon_rt_path_is(ptr, i64, i8 zeroext) nounwind
declare { ptr, i64 } @baryon_rt_path_join(ptr, i64, ptr, i64) nounwind
declare zeroext i1 @baryon_rt_path_list(ptr, i64, i1 zeroext, ptr, ptr) nounwind
declare zeroext i1 @baryon_rt_path_make_directory(ptr, i64, i1 zeroext, i1 zeroext, ptr) nounwind
declare { ptr, i64 } @baryon_rt_path_part(ptr, i64, i8 zeroext) nounwind
"
    );
}

/// The body the test above compiles: the six calls, one block each, then two returns, the
/// last the target of every error edge.
fn path_calls(database: &Database) -> Body<'_> {
    let none = Type::new(database, TypeKind::None);
    let str = Type::new(database, TypeKind::Str);
    let bool = primitive(database, Primitive::Bool);
    let i64 = primitive(database, Primitive::Integer(Integer::I64));
    let list = Type::new(database, TypeKind::List(str));
    let raised = Type::new(
        database,
        TypeKind::Fallible {
            value: none,
            errors: vec![i64],
        },
    );
    let mut body = Body::new(none, span());
    let name = body.push_local(str, LocalKind::Temporary, span());
    let path = body.push_local(str, LocalKind::Temporary, span());
    let suffix = body.push_local(str, LocalKind::Temporary, span());
    let is_directory = body.push_local(bool, LocalKind::Temporary, span());
    let nothing = body.push_local(none, LocalKind::Temporary, span());
    let names = body.push_local(list, LocalKind::Temporary, span());
    let here = body.push_local(str, LocalKind::Temporary, span());
    let error = body.push_local(raised, LocalKind::Temporary, span());
    let runs = Argument::Owned(Operand::Constant(Constant {
        ty: str,
        value: ConstantValue::Str("runs".to_owned()),
    }));
    let yes = Argument::Owned(Operand::Constant(Constant {
        ty: bool,
        value: ConstantValue::Bool(true),
    }));
    let borrowed = Argument::Borrow(path.into());
    let calls = vec![
        (
            PathMethod::Join,
            vec![runs, Argument::Borrow(name.into())],
            path,
        ),
        (PathMethod::Suffix, vec![borrowed.clone()], suffix),
        (
            PathMethod::IsDirectory,
            vec![borrowed.clone()],
            is_directory,
        ),
        (
            PathMethod::MakeDirectories,
            vec![borrowed.clone(), yes],
            nothing,
        ),
        (PathMethod::ListDirectory, vec![borrowed], names),
        (PathMethod::CurrentDirectory, vec![], here),
    ];
    let count = calls.len();
    for (index, (method, arguments, destination)) in calls.into_iter().enumerate() {
        body.push_block(end(TerminatorKind::Call {
            callee: Callee::Builtin(Builtin::Path(method)),
            arguments,
            destination: destination.into(),
            target: BlockId::new(index + 1),
            error: method.can_fail().then_some(ErrorEdge {
                error,
                target: BlockId::new(count + 1),
            }),
        }));
    }
    body.push_block(end(TerminatorKind::Return));
    body.push_block(end(TerminatorKind::Return));
    body
}
