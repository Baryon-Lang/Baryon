//! `print`: one runtime call per argument; the separator and line end lowering passes become
//! byte constants.

use super::{
    Database, Module, end, no_names, no_structs, parameter, primitive, sources, span, symbol_of,
};
use mir::{Argument, BlockId, Body, Builtin, Callee, Constant, ConstantValue, Operand};
use mir::{LocalKind, TerminatorKind};
use types::{Float, Integer, Mode, Primitive, Type, TypeKind};

/// `def show(x: u8): print("x =", x)`: the literal, the separator and the line end print through
/// `baryon_rt_print_str`, and the `u8` widens to `u64`.
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
            text(str, " "),
            Argument::Borrow(x.into()),
            text(str, "\n"),
        ],
        destination: printed.into(),
        target: BlockId::new(1),
        error: None,
    }));
    body.push_block(end(TerminatorKind::Return));
    let symbol = symbol_of(&database);
    let sources = sources();
    let mut module = Module::new(&database, &sources, &no_structs, &no_names, &symbol);
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
  call void @baryon_rt_print_str(i32 1, ptr @baryon.str.0, i64 3)
  call void @baryon_rt_print_str(i32 1, ptr @baryon.str.1, i64 1)
  %t0 = load i8, ptr %_1
  %t1 = zext i8 %t0 to i64
  call void @baryon_rt_print_u64(i32 1, i64 %t1)
  call void @baryon_rt_print_str(i32 1, ptr @baryon.str.2, i64 1)
  br label %bb1
bb1:
  ret void
}

@baryon.str.0 = private unnamed_addr constant [3 x i8] c\"x =\"
@baryon.str.1 = private unnamed_addr constant [1 x i8] c\" \"
@baryon.str.2 = private unnamed_addr constant [1 x i8] c\"\\0A\"

declare void @baryon_rt_print_str(i32, ptr, i64) nounwind
declare void @baryon_rt_print_u64(i32, i64) nounwind
"
    );
}

/// `print(a)` of an `Array[f64, 1]`: one runtime call with the header's pointer and the
/// addresses of its lengths and strides, from a copy of the header in a stack slot, and 10, the
/// runtime's number for `f64`.
#[test]
fn arrays_print_through_one_runtime_call() {
    let database = Database::default();
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let array = Type::new(
        &database,
        TypeKind::Array {
            element: f64,
            rank: 1,
        },
    );
    let body = print_one(&database, array);
    let symbol = symbol_of(&database);
    let sources = sources();
    let mut module = Module::new(&database, &sources, &no_structs, &no_names, &symbol);
    module.define("show", &body, &[]).unwrap();
    assert_eq!(
        module.finish(),
        "\
define void @show() {
start:
  %t1 = alloca { ptr, [1 x i64], [1 x i64] }
  %_2 = alloca { ptr, [1 x i64], [1 x i64] }
  br label %bb0
bb0:
  %t0 = load { ptr, [1 x i64], [1 x i64] }, ptr %_2
  store { ptr, [1 x i64], [1 x i64] } %t0, ptr %t1
  %t2 = extractvalue { ptr, [1 x i64], [1 x i64] } %t0, 0
  %t3 = getelementptr inbounds { ptr, [1 x i64], [1 x i64] }, ptr %t1, i32 0, i32 1, i32 0
  %t4 = getelementptr inbounds { ptr, [1 x i64], [1 x i64] }, ptr %t1, i32 0, i32 2, i32 0
  call void @baryon_rt_print_array(i32 1, i32 10, ptr %t2, i32 1, ptr %t3, ptr %t4)
  br label %bb1
bb1:
  ret void
}

declare void @baryon_rt_print_array(i32, i32, ptr, i32, ptr, ptr) nounwind
"
    );
}

/// A value `print` cannot write yet, an array of `str`, is named in the error.
#[test]
fn unprintable_values_are_named() {
    let database = Database::default();
    let str = Type::new(&database, TypeKind::Str);
    let array = Type::new(
        &database,
        TypeKind::Array {
            element: str,
            rank: 1,
        },
    );
    let body = print_one(&database, array);
    let symbol = symbol_of(&database);
    let sources = sources();
    let mut module = Module::new(&database, &sources, &no_structs, &no_names, &symbol);
    assert_eq!(
        module
            .define("show", &body, &[])
            .map_err(|error| error.to_string()),
        Err(
            "the LLVM backend does not compile printing values of type `Array[str, 1]` yet"
                .to_owned()
        )
    );
}

/// A body that prints one local of type `ty`, borrowed.
fn print_one<'db>(database: &'db Database, ty: Type<'db>) -> Body<'db> {
    let none = Type::new(database, TypeKind::None);
    let mut body = Body::new(none, span());
    let printed = body.push_local(none, LocalKind::Temporary, span());
    let values = body.push_local(ty, LocalKind::Temporary, span());
    body.push_block(end(TerminatorKind::Call {
        callee: Callee::Builtin(Builtin::Print),
        arguments: vec![Argument::Borrow(values.into())],
        destination: printed.into(),
        target: BlockId::new(1),
        error: None,
    }));
    body.push_block(end(TerminatorKind::Return));
    body
}

/// The `str` constant `text`, passed `Owned`, as lowering passes `print`'s separator and line
/// end.
fn text<'db>(str: Type<'db>, text: &str) -> Argument<'db> {
    Argument::Owned(Operand::Constant(Constant {
        ty: str,
        value: ConstantValue::Str(text.to_owned()),
    }))
}
