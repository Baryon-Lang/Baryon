//! Text files (decision 0036): `open` and the `File` methods are runtime calls that raise as a
//! compiled function does, through the error edge's local, and a drop closes the file. Array
//! files, `Array.fromfile` and `a.tofile`, raise the same way.

use super::{Database, end, module, primitive, span};
use mir::ErrorEdge;
use mir::{Argument, ArrayFile, BlockId, Body, Builtin, Callee, Constant, ConstantValue};
use mir::{FileMethod, Local, LocalKind, Operand, Statement, StatementKind, TerminatorKind};
use types::{Integer, Primitive, StandardType, Type, TypeKind};

/// `out = open("out.txt", "w")`, then `out.write("hi")`, then the drop of `out`; either call's
/// failure goes to the last block with the error in its local. `i64` stands for `OSError`,
/// since the backend only passes the local's address.
#[test]
fn file_calls_branch_on_the_failure_flag_and_a_drop_closes() {
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
    let out = body.push_local(file, LocalKind::Temporary, span());
    let count = body.push_local(i64, LocalKind::Temporary, span());
    let error = body.push_local(raised, LocalKind::Temporary, span());
    let text = |text: &str| {
        Argument::Owned(Operand::Constant(Constant {
            ty: str,
            value: ConstantValue::Str(text.to_owned()),
        }))
    };
    let edge = || {
        Some(ErrorEdge {
            error,
            target: BlockId::new(3),
        })
    };
    body.push_block(end(TerminatorKind::Call {
        callee: Callee::Builtin(Builtin::File(FileMethod::Open)),
        arguments: vec![
            text("out.txt"),
            Argument::Owned(Operand::Constant(Constant::integer(i64, 1))),
        ],
        destination: out.into(),
        target: BlockId::new(1),
        error: edge(),
    }));
    body.push_block(end(TerminatorKind::Call {
        callee: Callee::Builtin(Builtin::File(FileMethod::Write)),
        arguments: vec![Argument::Borrow(out.into()), text("hi")],
        destination: count.into(),
        target: BlockId::new(2),
        error: edge(),
    }));
    let close = body.push_block(end(TerminatorKind::Return));
    body.blocks[close.index()].statements = vec![Statement {
        kind: StatementKind::Drop(out.into()),
        span: span(),
    }];
    body.push_block(end(TerminatorKind::Return));
    assert_eq!(
        module(&database, &[("save", body)]),
        "\
define void @save() {
start:
  %_1 = alloca ptr
  %_2 = alloca i64
  %_3 = alloca { i8, [1 x i64] }
  br label %bb0
bb0:
  %t0 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775801 }, 0
  %t1 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775801 }, 1
  %t2 = and i64 %t1, 9223372036854775807
  %t3 = trunc i64 1 to i8
  %t4 = call zeroext i1 @baryon_rt_file_open(ptr %t0, i64 %t2, i8 zeroext %t3, ptr %_1, ptr %_3)
  br i1 %t4, label %bb3, label %bb1
bb1:
  %t5 = load ptr, ptr %_1
  %t6 = extractvalue { ptr, i64 } { ptr @baryon.str.1, i64 -9223372036854775806 }, 0
  %t7 = extractvalue { ptr, i64 } { ptr @baryon.str.1, i64 -9223372036854775806 }, 1
  %t8 = and i64 %t7, 9223372036854775807
  %t9 = call zeroext i1 @baryon_rt_file_write(ptr %t5, ptr %t6, i64 %t8, ptr %_2, ptr %_3)
  br i1 %t9, label %bb3, label %bb2
bb2:
  %t10 = load ptr, ptr %_1
  call void @baryon_rt_file_drop(ptr %t10)
  ret void
bb3:
  ret void
}

@baryon.str.0 = private unnamed_addr constant [7 x i8] c\"out.txt\"
@baryon.str.1 = private unnamed_addr constant [2 x i8] c\"hi\"

declare void @baryon_rt_file_drop(ptr) nounwind
declare zeroext i1 @baryon_rt_file_open(ptr, i64, i8 zeroext, ptr, ptr) nounwind
declare zeroext i1 @baryon_rt_file_write(ptr, ptr, i64, ptr, ptr) nounwind
"
    );
}

/// `flags = Array.fromfile("flags.bin", bool, count=-1, offset=16)`, then
/// `flags.tofile("copy.bin")`; either call's failure goes to the last block. The read passes the
/// element's layout, one byte with a `bool` at offset 0, and the write the header's address.
#[test]
fn array_file_calls_pass_the_element_layout_and_the_header() {
    let database = Database::default();
    let none = Type::new(&database, TypeKind::None);
    let str = Type::new(&database, TypeKind::Str);
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let element = primitive(&database, Primitive::Bool);
    let array = Type::new(&database, TypeKind::Array { element, rank: 1 });
    let raised = Type::new(
        &database,
        TypeKind::Fallible {
            value: none,
            errors: vec![i64],
        },
    );
    let mut body = Body::new(none, span());
    let flags = body.push_local(array, LocalKind::Temporary, span());
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
            target: BlockId::new(3),
        })
    };
    body.push_block(end(TerminatorKind::Call {
        callee: Callee::Builtin(Builtin::ArrayFile(ArrayFile::FromFile)),
        arguments: vec![text("flags.bin"), number(-1), number(16)],
        destination: flags.into(),
        target: BlockId::new(1),
        error: edge(),
    }));
    body.push_block(end(TerminatorKind::Call {
        callee: Callee::Builtin(Builtin::ArrayFile(ArrayFile::ToFile)),
        arguments: vec![Argument::Borrow(flags.into()), text("copy.bin")],
        destination: Local::RETURN.into(),
        target: BlockId::new(2),
        error: edge(),
    }));
    body.push_block(end(TerminatorKind::Return));
    body.push_block(end(TerminatorKind::Return));
    assert_eq!(
        module(&database, &[("copy", body)]),
        "\
define void @copy() {
start:
  %t3 = alloca [1 x i64]
  %t5 = alloca { i64, i64, i64, ptr }
  %_1 = alloca { ptr, [1 x i64], [1 x i64] }
  %_2 = alloca { i8, [1 x i64] }
  br label %bb0
bb0:
  %t0 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775799 }, 0
  %t1 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775799 }, 1
  %t2 = and i64 %t1, 9223372036854775807
  store [1 x i64] [i64 0], ptr %t3
  %t4 = insertvalue { i64, i64, i64, ptr } { i64 1, i64 1, i64 1, ptr null }, ptr %t3, 3
  store { i64, i64, i64, ptr } %t4, ptr %t5
  %t6 = call zeroext i1 @baryon_rt_array_from_file(ptr %t0, i64 %t2, ptr %t5, i64 -1, i64 16, \
ptr %_1, ptr %_2)
  br i1 %t6, label %bb3, label %bb1
bb1:
  %t7 = extractvalue { ptr, i64 } { ptr @baryon.str.1, i64 -9223372036854775800 }, 0
  %t8 = extractvalue { ptr, i64 } { ptr @baryon.str.1, i64 -9223372036854775800 }, 1
  %t9 = and i64 %t8, 9223372036854775807
  %t10 = call zeroext i1 @baryon_rt_array_to_file(ptr %t7, i64 %t9, ptr %_1, i64 1, i64 1, ptr %_2)
  br i1 %t10, label %bb3, label %bb2
bb2:
  ret void
bb3:
  ret void
}

@baryon.str.0 = private unnamed_addr constant [9 x i8] c\"flags.bin\"
@baryon.str.1 = private unnamed_addr constant [8 x i8] c\"copy.bin\"

declare zeroext i1 @baryon_rt_array_from_file(ptr, i64, ptr, i64, i64, ptr, ptr) nounwind
declare zeroext i1 @baryon_rt_array_to_file(ptr, i64, ptr, i64, i64, ptr) nounwind
"
    );
}
