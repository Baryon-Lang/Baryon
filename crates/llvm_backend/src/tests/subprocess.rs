//! Running another program (decision 0036): `subprocess.run` is one runtime call that raises as
//! a compiled function does, through the error edge's local.

use super::{Database, end, module, primitive, span};
use mir::{Argument, BlockId, Body, Builtin, Callee, Constant, ConstantValue, ErrorEdge};
use mir::{LocalKind, Operand, TerminatorKind};
use types::{Float, Integer, Primitive, Type, TypeKind};

/// The module the test below expects: the first call passes nothing it was not given, the
/// second passes all four keywords, `given` 15.
const EXPECTED: &str = "\
define void @build() {
start:
  %_1 = alloca { ptr, i64, i64 }
  %_2 = alloca { ptr, i64 }
  %_3 = alloca ptr
  %_4 = alloca i64
  %_5 = alloca { i8, [1 x i64] }
  br label %bb0
bb0:
  %t0 = call zeroext i1 @baryon_rt_subprocess_run(ptr %_1, i1 zeroext false, i8 zeroext 0, ptr null, i64 0, ptr null, i64 0, ptr null, double 0.0, ptr @baryon.loc.1, ptr %_4, ptr %_5)
  br i1 %t0, label %bb3, label %bb1
bb1:
  %t1 = extractvalue { ptr, i64 } { ptr @baryon.str.2, i64 -9223372036854775806 }, 0
  %t2 = extractvalue { ptr, i64 } { ptr @baryon.str.2, i64 -9223372036854775806 }, 1
  %t3 = and i64 %t2, 9223372036854775807
  %t4 = load { ptr, i64 }, ptr %_2
  %t5 = extractvalue { ptr, i64 } %t4, 0
  %t6 = extractvalue { ptr, i64 } %t4, 1
  %t7 = and i64 %t6, 9223372036854775807
  %t8 = load ptr, ptr %_3
  %t9 = call zeroext i1 @baryon_rt_subprocess_run(ptr %_1, i1 zeroext true, i8 zeroext 15, ptr %t1, i64 %t3, ptr %t5, i64 %t7, ptr %t8, double 0x4004000000000000, ptr @baryon.loc.1, ptr %_4, ptr %_5)
  br i1 %t9, label %bb3, label %bb2
bb2:
  ret void
bb3:
  ret void
}

@baryon.str.0 = private unnamed_addr constant [8 x i8] c\"heat.byn\"
@baryon.loc.1 = private unnamed_addr constant { ptr, i64, i32, i32 } { ptr @baryon.str.0, i64 8, i32 1, i32 1 }
@baryon.str.2 = private unnamed_addr constant [2 x i8] c\"hi\"

declare zeroext i1 @baryon_rt_subprocess_run(ptr, i1 zeroext, i8 zeroext, ptr, i64, ptr, i64, ptr, double, ptr, ptr, ptr) nounwind
";

/// `subprocess.run(args)` with every keyword left out, then
/// `subprocess.run(args, capture_output=True, input="hi", cwd=directory, env=env, timeout=2.5)`;
/// either call's failure goes to the last block with the error in its local. `i64` stands for
/// `CompletedProcess` and `OSError`, since the backend only passes their locals' addresses.
#[test]
fn run_passes_what_was_given_and_branches_on_the_failure_flag() {
    let database = Database::default();
    let none = Type::new(&database, TypeKind::None);
    let str = Type::new(&database, TypeKind::Str);
    let bool = primitive(&database, Primitive::Bool);
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let list = Type::new(&database, TypeKind::List(str));
    let dict = Type::new(
        &database,
        TypeKind::Dict {
            key: str,
            value: str,
        },
    );
    let raised = Type::new(
        &database,
        TypeKind::Fallible {
            value: none,
            errors: vec![i64],
        },
    );
    let mut body = Body::new(none, span());
    let args = body.push_local(list, LocalKind::Temporary, span());
    let directory = body.push_local(str, LocalKind::Temporary, span());
    let env = body.push_local(dict, LocalKind::Temporary, span());
    let done = body.push_local(i64, LocalKind::Temporary, span());
    let error = body.push_local(raised, LocalKind::Temporary, span());
    let constant = |ty, value| Argument::Owned(Operand::Constant(Constant { ty, value }));
    let left_out = || constant(none, ConstantValue::None);
    let call = |arguments, target| {
        end(TerminatorKind::Call {
            callee: Callee::Builtin(Builtin::Run),
            arguments,
            destination: done.into(),
            target: BlockId::new(target),
            error: Some(ErrorEdge {
                error,
                target: BlockId::new(3),
            }),
        })
    };
    body.push_block(call(
        vec![
            Argument::Borrow(args.into()),
            constant(bool, ConstantValue::Bool(false)),
            left_out(),
            left_out(),
            left_out(),
            left_out(),
        ],
        1,
    ));
    body.push_block(call(
        vec![
            Argument::Borrow(args.into()),
            constant(bool, ConstantValue::Bool(true)),
            constant(str, ConstantValue::Str("hi".to_owned())),
            Argument::Borrow(directory.into()),
            Argument::Borrow(env.into()),
            Argument::Owned(Operand::Constant(Constant::float(f64, 2.5))),
        ],
        2,
    ));
    body.push_block(end(TerminatorKind::Return));
    body.push_block(end(TerminatorKind::Return));
    assert_eq!(module(&database, &[("build", body)]), EXPECTED);
}
