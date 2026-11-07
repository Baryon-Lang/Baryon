//! `print`: one runtime call per argument, the separator and line end as byte constants.

use super::{Database, Module, end, no_structs, parameter, primitive, sources, span, symbol_of};
use mir::{Argument, BlockId, Body, Builtin, Callee, Constant, ConstantValue, Operand};
use mir::{LocalKind, TerminatorKind};
use types::{Integer, Mode, Primitive, Type, TypeKind};

/// `def show(x: u8): print("x =", x)`: the literal and the separators print through
/// `npl_rt_print_str`, and the `u8` widens to `u64`.
#[test]
fn print_calls_the_runtime_per_argument() {
    let database = Database::default();
    let u8 = primitive(&database, Primitive::Integer(Integer::U8));
    let none = Type::new(&database, TypeKind::None);
    let str = Type::new(&database, TypeKind::Str);
    let mut body = Body::new(none, span());
    let x = body.push_local(u8, parameter("x", Mode::Borrow), span());
    let printed = body.push_local(none, LocalKind::Temporary, span());
    let label = Constant {
        ty: str,
        value: ConstantValue::Str("x =".to_owned()),
    };
    body.push_block(end(TerminatorKind::Call {
        callee: Callee::Builtin(Builtin::Print),
        arguments: vec![
            Argument::Owned(Operand::Constant(label)),
            Argument::Borrow(x.into()),
        ],
        destination: printed.into(),
        target: BlockId::new(1),
        error: None,
    }));
    body.push_block(end(TerminatorKind::Return));
    let symbol = symbol_of(&database);
    let sources = sources();
    let mut module = Module::new(&database, &sources, &no_structs, &symbol);
    module.define("show", &body, &[]).unwrap();
    assert_eq!(
        module.finish(),
        "\
define void @show(i8 %a1) {
start:
  %_1 = alloca i8
  store i8 %a1, ptr %_1
  br label %bb0
bb0:
  call void @npl_rt_print_str(i32 1, ptr @npl.str.0, i64 3)
  call void @npl_rt_print_str(i32 1, ptr @npl.str.1, i64 1)
  %t0 = load i8, ptr %_1
  %t1 = zext i8 %t0 to i64
  call void @npl_rt_print_u64(i32 1, i64 %t1)
  call void @npl_rt_print_str(i32 1, ptr @npl.str.2, i64 1)
  br label %bb1
bb1:
  ret void
}

@npl.str.0 = private unnamed_addr constant [3 x i8] c\"x =\"
@npl.str.1 = private unnamed_addr constant [1 x i8] c\" \"
@npl.str.2 = private unnamed_addr constant [1 x i8] c\"\\0A\"

declare void @npl_rt_print_str(i32, ptr, i64) nounwind
declare void @npl_rt_print_u64(i32, i64) nounwind
"
    );
}

/// A value `print` cannot write yet is named in the error.
#[test]
fn unprintable_values_are_named() {
    let database = Database::default();
    let none = Type::new(&database, TypeKind::None);
    let mut body = Body::new(none, span());
    let printed = body.push_local(none, LocalKind::Temporary, span());
    let unit = Type::new(&database, TypeKind::Tuple(Vec::new()));
    let empty = body.push_local(unit, LocalKind::Temporary, span());
    body.push_block(end(TerminatorKind::Call {
        callee: Callee::Builtin(Builtin::Print),
        arguments: vec![Argument::Borrow(empty.into())],
        destination: printed.into(),
        target: BlockId::new(1),
        error: None,
    }));
    body.push_block(end(TerminatorKind::Return));
    let symbol = symbol_of(&database);
    let sources = sources();
    let mut module = Module::new(&database, &sources, &no_structs, &symbol);
    assert_eq!(
        module
            .define("show", &body, &[])
            .map_err(|error| error.to_string()),
        Err("the LLVM backend does not compile printing values of type `tuple[]` yet".to_owned())
    );
}
