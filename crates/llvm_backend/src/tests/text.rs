//! `str` values (decision 0022): a literal is a constant with the "not owned" bit, printing
//! masks the bit off, and a drop frees only when the bit is clear.

use super::{Database, Module, assign, end, no_structs, parameter, sources, span, symbol_of};
use mir::{Argument, BlockId, Body, Builtin, Callee, Constant, ConstantValue, Operand, Rvalue};
use mir::{LocalKind, Statement, StatementKind, TerminatorKind};
use types::{Mode, Type, TypeKind};

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
        arguments: vec![Argument::Borrow(name.into())],
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
    let mut module = Module::new(&database, &sources, &no_structs, &symbol);
    module.define("show", &body, &[]).unwrap();
    assert_eq!(
        module.finish(),
        "\
define void @show(ptr noalias readonly %a1) {
start:
  %_2 = alloca { ptr, i64 }
  br label %bb0
bb0:
  store { ptr, i64 } { ptr @npl.str.0, i64 -9223372036854775804 }, ptr %_2
  %t0 = load { ptr, i64 }, ptr %a1
  %t1 = extractvalue { ptr, i64 } %t0, 0
  %t2 = extractvalue { ptr, i64 } %t0, 1
  %t3 = and i64 %t2, 9223372036854775807
  call void @npl_rt_print_str(i32 1, ptr %t1, i64 %t3)
  call void @npl_rt_print_str(i32 1, ptr @npl.str.1, i64 1)
  br label %bb1
bb1:
  %t4 = getelementptr inbounds { ptr, i64 }, ptr %_2, i32 0, i32 1
  %t5 = load i64, ptr %t4
  %t6 = icmp sge i64 %t5, 0
  br i1 %t6, label %c0, label %c1
c0:
  %t7 = load ptr, ptr %_2
  call void @npl_rt_deallocate(ptr %t7, i64 %t5, i64 1)
  br label %c1
c1:
  ret void
}

@npl.str.0 = private unnamed_addr constant [4 x i8] c\"heat\"
@npl.str.1 = private unnamed_addr constant [1 x i8] c\"\\0A\"

declare void @npl_rt_deallocate(ptr, i64, i64) nounwind
declare void @npl_rt_print_str(i32, ptr, i64) nounwind
"
    );
}
