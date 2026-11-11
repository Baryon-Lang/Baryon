//! The standard streams (decision 0036, part 6): a stream's `File`, `print` to standard error
//! and to a file, the flush of `print`, and `input`, each a call of the runtime.

use super::{Database, end, module, primitive, span};
use mir::{Argument, BlockId, Body, Builtin, Callee, Constant, ConstantValue, ErrorEdge};
use mir::{FileMethod, LocalKind, Operand, TerminatorKind};
use types::{Integer, Primitive, StandardType, Type, TypeKind};

/// `err = sys.stderr`, `print(7, file=sys.stderr)`, `print(7, "\n", file=err)`, a flush of
/// `print`, then `name = input("n: ")`. A print to standard error names stream 2 and cannot
/// fail; a print to a file prints to stream 3 and then hands the text to the file, which can
/// fail; `input` raises through its error edge. `i64` stands for the error types, since the
/// backend only passes the error local's address.
#[test]
fn streams_print_targets_and_input_are_runtime_calls() {
    let database = Database::default();
    let none = Type::new(&database, TypeKind::None);
    let str = Type::new(&database, TypeKind::Str);
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let file = Type::new(&database, TypeKind::Standard(StandardType::File));
    let raised = Type::new(
        &database,
        TypeKind::Fallible {
            value: none,
            errors: vec![i64],
        },
    );
    let mut body = Body::new(none, span());
    let stream = body.push_local(file, LocalKind::Temporary, span());
    let name = body.push_local(str, LocalKind::Temporary, span());
    let error = body.push_local(raised, LocalKind::Temporary, span());
    let text = |text: &str| {
        Argument::Owned(Operand::Constant(Constant {
            ty: str,
            value: ConstantValue::Str(text.to_owned()),
        }))
    };
    let number = |value| Argument::Owned(Operand::Constant(Constant::integer(i64, value)));
    let edge = || {
        Some(ErrorEdge {
            error,
            target: BlockId::new(5),
        })
    };
    let calls = [
        (Builtin::StandardStream, vec![number(2)], stream, None),
        (Builtin::PrintError, vec![number(7)], name, None),
        (
            Builtin::File(FileMethod::Print),
            vec![Argument::Borrow(stream.into()), number(7), text("\n")],
            name,
            edge(),
        ),
        (Builtin::FlushOutput, Vec::new(), name, None),
        (Builtin::Input, vec![text("n: ")], name, edge()),
    ];
    for (index, (builtin, arguments, destination, error)) in calls.into_iter().enumerate() {
        body.push_block(end(TerminatorKind::Call {
            callee: Callee::Builtin(builtin),
            arguments,
            destination: destination.into(),
            target: BlockId::new(index + 1),
            error,
        }));
    }
    body.push_block(end(TerminatorKind::Return));
    assert_eq!(
        module(&database, &[("streams", body)]),
        "\
define void @streams() {
start:
  %_1 = alloca ptr
  %_2 = alloca { ptr, i64 }
  %_3 = alloca { i8, [1 x i64] }
  br label %bb0
bb0:
  %t0 = trunc i64 2 to i8
  %t1 = call ptr @baryon_rt_file_standard(i8 zeroext %t0)
  store ptr %t1, ptr %_1
  br label %bb1
bb1:
  call void @baryon_rt_print_i64(i32 2, i64 7)
  br label %bb2
bb2:
  call void @baryon_rt_print_i64(i32 3, i64 7)
  call void @baryon_rt_print_str(i32 3, ptr @baryon.str.0, i64 1)
  %t2 = load ptr, ptr %_1
  %t3 = call zeroext i1 @baryon_rt_file_print(ptr %t2, ptr %_3)
  br i1 %t3, label %bb5, label %bb3
bb3:
  call void @baryon_rt_flush()
  br label %bb4
bb4:
  %t4 = extractvalue { ptr, i64 } { ptr @baryon.str.1, i64 -9223372036854775805 }, 0
  %t5 = extractvalue { ptr, i64 } { ptr @baryon.str.1, i64 -9223372036854775805 }, 1
  %t6 = and i64 %t5, 9223372036854775807
  %t7 = call zeroext i1 @baryon_rt_input(ptr %t4, i64 %t6, ptr %_2, ptr %_3)
  br i1 %t7, label %bb5, label %bb5
bb5:
  ret void
}

@baryon.str.0 = private unnamed_addr constant [1 x i8] c\"\\0A\"
@baryon.str.1 = private unnamed_addr constant [3 x i8] c\"n: \"

declare zeroext i1 @baryon_rt_file_print(ptr, ptr) nounwind
declare ptr @baryon_rt_file_standard(i8 zeroext) nounwind
declare void @baryon_rt_flush() nounwind
declare zeroext i1 @baryon_rt_input(ptr, i64, ptr, ptr) nounwind
declare void @baryon_rt_print_i64(i32, i64) nounwind
declare void @baryon_rt_print_str(i32, ptr, i64) nounwind
"
    );
}
