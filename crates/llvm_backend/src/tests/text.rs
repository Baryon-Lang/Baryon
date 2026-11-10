//! `str` values (decision 0022): a literal is a constant with the "not owned" bit, printing
//! masks the bit off, and a drop frees only when the bit is clear.

use super::{
    Database, Module, assign, end, module, no_names, no_structs, parameter, primitive, sources,
};
use super::{span, symbol_of};
use mir::{Argument, BlockId, Body, Builtin, Callee, Constant, ConstantValue, Operand, Rvalue};
use mir::{Local, LocalKind, Piece, Statement, StatementKind, TerminatorKind};
use types::{Float, FormatSpec, Integer, Mode, Primitive, Type, TypeKind};

/// `def show(name: str): local = "heat"; print(name)`, then `local` dropped: the borrowed
/// `name` arrives as a pointer, `local` holds the literal's bytes without copying them, and its
/// drop tests the bit before freeing.
#[test]
fn literals_are_flagged_and_drops_free_only_owned_bytes() {
    let database = Database::default();
    let none = Type::new(&database, TypeKind::None);
    let str = Type::new(&database, TypeKind::Str);
    let mut body = Body::new(none, span());
    let name = body.push_local(str, parameter("name", Mode::Borrow), span());
    let local = body.push_local(str, LocalKind::Temporary, span());
    let printed = body.push_local(none, LocalKind::Temporary, span());
    let heat = Constant {
        ty: str,
        value: ConstantValue::Str("heat".to_owned()),
    };
    let first = body.push_block(end(TerminatorKind::Call {
        callee: Callee::Builtin(Builtin::Print),
        arguments: vec![
            Argument::Borrow(name.into()),
            Argument::Owned(Operand::Constant(Constant {
                ty: str,
                value: ConstantValue::Str("\n".to_owned()),
            })),
        ],
        destination: printed.into(),
        target: BlockId::new(1),
        error: None,
    }));
    body.blocks[first.index()].statements =
        vec![assign(local, Rvalue::Use(Operand::Constant(heat)))];
    let last = body.push_block(end(TerminatorKind::Return));
    body.blocks[last.index()].statements = vec![Statement {
        kind: StatementKind::Drop(local.into()),
        span: span(),
    }];
    let symbol = symbol_of(&database);
    let sources = sources();
    let mut module = Module::new(&database, &sources, &no_structs, &no_names, &symbol);
    module.define("show", &body, &[]).unwrap();
    assert_eq!(
        module.finish(),
        "\
define void @show(ptr noalias readonly %a1) {
start:
  %_2 = alloca { ptr, i64 }
  br label %bb0
bb0:
  store { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775804 }, ptr %_2
  %t0 = load { ptr, i64 }, ptr %a1
  %t1 = extractvalue { ptr, i64 } %t0, 0
  %t2 = extractvalue { ptr, i64 } %t0, 1
  %t3 = and i64 %t2, 9223372036854775807
  call void @baryon_rt_print_str(i32 1, ptr %t1, i64 %t3)
  call void @baryon_rt_print_str(i32 1, ptr @baryon.str.1, i64 1)
  br label %bb1
bb1:
  %t4 = getelementptr inbounds { ptr, i64 }, ptr %_2, i32 0, i32 1
  %t5 = load i64, ptr %t4
  %t6 = icmp sge i64 %t5, 0
  br i1 %t6, label %c0, label %c1
c0:
  %t7 = load ptr, ptr %_2
  call void @baryon_rt_deallocate(ptr %t7, i64 %t5, i64 1)
  br label %c1
c1:
  ret void
}

@baryon.str.0 = private unnamed_addr constant [4 x i8] c\"heat\"
@baryon.str.1 = private unnamed_addr constant [1 x i8] c\"\\0A\"

declare void @baryon_rt_deallocate(ptr, i64, i64) nounwind
declare void @baryon_rt_print_str(i32, ptr, i64) nounwind
"
    );
}

/// `def label(count: i64, energy: f64) -> str: return f"n={count} e={energy:.2f}"`: a builder,
/// one call per piece, a null specification for the empty one and a constant for `.2f`, then
/// the finished text stored as the result, which owns it.
#[test]
fn f_strings_call_the_runtime_builder() {
    let database = Database::default();
    let str = Type::new(&database, TypeKind::Str);
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let mut body = Body::new(str, span());
    let count = body.push_local(i64, parameter("count", Mode::Borrow), span());
    let energy = body.push_local(f64, parameter("energy", Mode::Borrow), span());
    let fixed = FormatSpec::parse(".2f").unwrap();
    let pieces = vec![
        Piece::Text("n=".to_owned()),
        Piece::Value(count.into(), FormatSpec::default()),
        Piece::Text(" e=".to_owned()),
        Piece::Value(energy.into(), fixed),
    ];
    let first = body.push_block(end(TerminatorKind::Return));
    body.blocks[first.index()].statements = vec![assign(Local::RETURN, Rvalue::Format(pieces))];
    assert_eq!(
        module(&database, &[("label", body)]),
        "\
define { ptr, i64 } @label(i64 %a1, double %a2) {
start:
  %_0 = alloca { ptr, i64 }
  %_1 = alloca i64
  %_2 = alloca double
  store i64 %a1, ptr %_1
  store double %a2, ptr %_2
  br label %bb0
bb0:
  %t0 = call ptr @baryon_rt_text_new()
  call void @baryon_rt_text_str(ptr %t0, ptr @baryon.str.0, i64 2, ptr null)
  %t1 = load i64, ptr %_1
  call void @baryon_rt_text_i64(ptr %t0, i64 %t1, ptr null)
  call void @baryon_rt_text_str(ptr %t0, ptr @baryon.str.1, i64 3, ptr null)
  %t2 = load double, ptr %_2
  call void @baryon_rt_text_f64(ptr %t0, double %t2, ptr @baryon.spec.2)
  %t3 = call { ptr, i64 } @baryon_rt_text_finish(ptr %t0)
  store { ptr, i64 } %t3, ptr %_0
  %t4 = load { ptr, i64 }, ptr %_0
  ret { ptr, i64 } %t4
}

@baryon.str.0 = private unnamed_addr constant [2 x i8] c\"n=\"
@baryon.str.1 = private unnamed_addr constant [3 x i8] c\" e=\"
@baryon.spec.2 = private unnamed_addr constant { i32, i8, i8, i8, i8, i8, i8, i64, i64 } \
{ i32 32, i8 0, i8 45, i8 0, i8 0, i8 0, i8 102, i64 -1, i64 2 }

declare void @baryon_rt_text_f64(ptr, double, ptr) nounwind
declare { ptr, i64 } @baryon_rt_text_finish(ptr) nounwind
declare void @baryon_rt_text_i64(ptr, i64, ptr) nounwind
declare ptr @baryon_rt_text_new() nounwind
declare void @baryon_rt_text_str(ptr, ptr, i64, ptr) nounwind
"
    );
}

/// `def facts(name: str) -> bool`: compares `name` with the literal `"heat"`, counts its
/// characters and asks whether `"ea"` is in it, each through the runtime with the strings'
/// byte pointers and lengths, the bit masked off; the comparison's C `int` widens to `i64`.
#[test]
fn comparisons_searches_and_lengths_call_the_runtime() {
    let database = Database::default();
    let str = Type::new(&database, TypeKind::Str);
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let bool = primitive(&database, Primitive::Bool);
    let literal = |text: &str| {
        Argument::Owned(Operand::Constant(Constant {
            ty: str,
            value: ConstantValue::Str(text.to_owned()),
        }))
    };
    let mut body = Body::new(bool, span());
    let name = body.push_local(str, parameter("name", Mode::Borrow), span());
    let order = body.push_local(i64, LocalKind::Temporary, span());
    let length = body.push_local(i64, LocalKind::Temporary, span());
    let calls = [
        (
            Builtin::CompareStr,
            vec![Argument::Borrow(name.into()), literal("heat")],
            order,
        ),
        (
            Builtin::LengthStr,
            vec![Argument::Borrow(name.into())],
            length,
        ),
        (
            Builtin::ContainsStr,
            vec![Argument::Borrow(name.into()), literal("ea")],
            Local::RETURN,
        ),
    ];
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
    assert_eq!(
        module(&database, &[("facts", body)]),
        "\
define i1 @facts(ptr noalias readonly %a1) {
start:
  %_0 = alloca i1
  %_2 = alloca i64
  %_3 = alloca i64
  br label %bb0
bb0:
  %t0 = load { ptr, i64 }, ptr %a1
  %t1 = extractvalue { ptr, i64 } %t0, 0
  %t2 = extractvalue { ptr, i64 } %t0, 1
  %t3 = and i64 %t2, 9223372036854775807
  %t4 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775804 }, 0
  %t5 = extractvalue { ptr, i64 } { ptr @baryon.str.0, i64 -9223372036854775804 }, 1
  %t6 = and i64 %t5, 9223372036854775807
  %t7 = call i32 @baryon_rt_str_compare(ptr %t1, i64 %t3, ptr %t4, i64 %t6)
  %t8 = sext i32 %t7 to i64
  store i64 %t8, ptr %_2
  br label %bb1
bb1:
  %t9 = load { ptr, i64 }, ptr %a1
  %t10 = extractvalue { ptr, i64 } %t9, 0
  %t11 = extractvalue { ptr, i64 } %t9, 1
  %t12 = and i64 %t11, 9223372036854775807
  %t13 = call i64 @baryon_rt_str_length(ptr %t10, i64 %t12)
  store i64 %t13, ptr %_3
  br label %bb2
bb2:
  %t14 = load { ptr, i64 }, ptr %a1
  %t15 = extractvalue { ptr, i64 } %t14, 0
  %t16 = extractvalue { ptr, i64 } %t14, 1
  %t17 = and i64 %t16, 9223372036854775807
  %t18 = extractvalue { ptr, i64 } { ptr @baryon.str.1, i64 -9223372036854775806 }, 0
  %t19 = extractvalue { ptr, i64 } { ptr @baryon.str.1, i64 -9223372036854775806 }, 1
  %t20 = and i64 %t19, 9223372036854775807
  %t21 = call i1 @baryon_rt_str_contains(ptr %t15, i64 %t17, ptr %t18, i64 %t20)
  store i1 %t21, ptr %_0
  br label %bb3
bb3:
  %t22 = load i1, ptr %_0
  ret i1 %t22
}

@baryon.str.0 = private unnamed_addr constant [4 x i8] c\"heat\"
@baryon.str.1 = private unnamed_addr constant [2 x i8] c\"ea\"

declare i32 @baryon_rt_str_compare(ptr, i64, ptr, i64) nounwind
declare zeroext i1 @baryon_rt_str_contains(ptr, i64, ptr, i64) nounwind
declare i64 @baryon_rt_str_length(ptr, i64) nounwind
"
    );
}

/// `def first(name: str) -> str`: the character at byte 0 of `name`. Its width comes from its
/// first byte, three comparisons with the UTF-8 lead bytes, and the result points into `name`'s
/// own bytes with the "not owned" bit set, so it is never freed; the cursor moves past it.
#[test]
fn a_character_borrows_its_bytes_and_moves_the_cursor() {
    let database = Database::default();
    let str = Type::new(&database, TypeKind::Str);
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let mut body = Body::new(str, span());
    let name = body.push_local(str, parameter("name", Mode::Borrow), span());
    let cursor = body.push_local(i64, LocalKind::Temporary, span());
    let start = Rvalue::Use(Operand::Constant(Constant::integer(i64, 0)));
    let first = end(TerminatorKind::Call {
        callee: Callee::Builtin(Builtin::CharacterStr),
        arguments: vec![
            Argument::Borrow(name.into()),
            Argument::InOut(cursor.into()),
        ],
        destination: Local::RETURN.into(),
        target: BlockId::new(1),
        error: None,
    });
    body.push_block(first);
    body.blocks[0].statements.push(assign(cursor, start));
    body.push_block(end(TerminatorKind::Return));
    assert_eq!(
        module(&database, &[("first", body)]),
        "\
define { ptr, i64 } @first(ptr noalias readonly %a1) {
start:
  %_0 = alloca { ptr, i64 }
  %_2 = alloca i64
  br label %bb0
bb0:
  store i64 0, ptr %_2
  %t0 = load { ptr, i64 }, ptr %a1
  %t1 = extractvalue { ptr, i64 } %t0, 0
  %t2 = extractvalue { ptr, i64 } %t0, 1
  %t3 = and i64 %t2, 9223372036854775807
  %t4 = load i64, ptr %_2
  %t5 = getelementptr inbounds i8, ptr %t1, i64 %t4
  %t6 = load i8, ptr %t5
  %t7 = zext i8 %t6 to i64
  %t8 = icmp uge i64 %t7, 192
  %t9 = zext i1 %t8 to i64
  %t10 = add i64 1, %t9
  %t11 = icmp uge i64 %t7, 224
  %t12 = zext i1 %t11 to i64
  %t13 = add i64 %t10, %t12
  %t14 = icmp uge i64 %t7, 240
  %t15 = zext i1 %t14 to i64
  %t16 = add i64 %t13, %t15
  %t17 = or i64 %t16, -9223372036854775808
  %t18 = insertvalue { ptr, i64 } poison, ptr %t5, 0
  %t19 = insertvalue { ptr, i64 } %t18, i64 %t17, 1
  store { ptr, i64 } %t19, ptr %_0
  %t20 = add i64 %t4, %t16
  store i64 %t20, ptr %_2
  br label %bb1
bb1:
  %t21 = load { ptr, i64 }, ptr %_0
  ret { ptr, i64 } %t21
}
"
    );
}
