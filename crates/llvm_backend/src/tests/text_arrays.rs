//! The steps of a text array (`Array[str, 1]`, decision 0045): none calls the runtime. A
//! string's bytes are counted from its length word, copied with one `llvm.memcpy`, and read
//! back as a `str` that points into the buffer with the "not owned" bit set.

use super::{Database, end, module, parameter, primitive, span};
use mir::TextArrayStep;
use mir::{Argument, BlockId, Body, Builtin, Callee, Local, LocalKind, Operand, TerminatorKind};
use types::{Integer, Mode, Primitive, Type, TypeKind};

/// `def keep(name: str, buffer: InOut[Array[u8, 1]]) -> str`: the bytes of `name` are counted,
/// copied to the start of `buffer`, and returned as a `str` that borrows them from the buffer.
/// The count masks the flag off the length word; the copy gives the position after the bytes;
/// the view sets the flag, so dropping it frees nothing.
#[test]
fn a_string_is_counted_copied_into_a_buffer_and_borrowed_back() {
    let database = Database::default();
    let str = Type::new(&database, TypeKind::Str);
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let u8 = primitive(&database, Primitive::Integer(Integer::U8));
    let bytes = Type::new(
        &database,
        TypeKind::Array {
            element: u8,
            rank: 1,
        },
    );
    let mut body = Body::new(str, span());
    let name = body.push_local(str, parameter("name", Mode::Borrow), span());
    let buffer = body.push_local(bytes, parameter("buffer", Mode::InOut), span());
    let length = body.push_local(i64, LocalKind::Temporary, span());
    let next = body.push_local(i64, LocalKind::Temporary, span());
    let zero = || Argument::Owned(Operand::Constant(mir::Constant::integer(i64, 0)));
    let step = |step, arguments, destination: Local, target| {
        end(TerminatorKind::Call {
            callee: Callee::Builtin(Builtin::TextArray(step)),
            arguments,
            destination: destination.into(),
            target: BlockId::new(target),
            error: None,
        })
    };
    body.push_block(step(
        TextArrayStep::ByteLength,
        vec![Argument::Borrow(name.into())],
        length,
        1,
    ));
    body.push_block(step(
        TextArrayStep::CopyBytes,
        vec![
            Argument::Borrow(name.into()),
            Argument::InOut(buffer.into()),
            zero(),
        ],
        next,
        2,
    ));
    body.push_block(step(
        TextArrayStep::View,
        vec![
            Argument::Borrow(buffer.into()),
            zero(),
            Argument::Owned(Operand::Copy(length.into())),
        ],
        Local::RETURN,
        3,
    ));
    body.push_block(end(TerminatorKind::Return));
    assert_eq!(
        module(&database, &[("keep", body)]),
        "\
define { ptr, i64 } @keep(ptr noalias readonly align 8 dereferenceable(16) %a1, ptr noalias align 8 dereferenceable(24) %a2) {
start:
  %_0 = alloca { ptr, i64 }
  %_3 = alloca i64
  %_4 = alloca i64
  br label %bb0
bb0:
  %t0 = load { ptr, i64 }, ptr %a1
  %t1 = extractvalue { ptr, i64 } %t0, 0
  %t2 = extractvalue { ptr, i64 } %t0, 1
  %t3 = and i64 %t2, 9223372036854775807
  store i64 %t3, ptr %_3
  br label %bb1
bb1:
  %t4 = load { ptr, i64 }, ptr %a1
  %t5 = extractvalue { ptr, i64 } %t4, 0
  %t6 = extractvalue { ptr, i64 } %t4, 1
  %t7 = and i64 %t6, 9223372036854775807
  %t8 = load ptr, ptr %a2
  %t9 = getelementptr inbounds i8, ptr %t8, i64 0
  call void @llvm.memcpy.p0.p0.i64(ptr %t9, ptr %t5, i64 %t7, i1 false)
  %t10 = add i64 0, %t7
  store i64 %t10, ptr %_4
  br label %bb2
bb2:
  %t11 = load ptr, ptr %a2
  %t12 = getelementptr inbounds i8, ptr %t11, i64 0
  %t13 = load i64, ptr %_3
  %t14 = or i64 %t13, -9223372036854775808
  %t15 = insertvalue { ptr, i64 } poison, ptr %t12, 0
  %t16 = insertvalue { ptr, i64 } %t15, i64 %t14, 1
  store { ptr, i64 } %t16, ptr %_0
  br label %bb3
bb3:
  %t17 = load { ptr, i64 }, ptr %_0
  ret { ptr, i64 } %t17
}

declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)
"
    );
}
