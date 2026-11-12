//! Number locals as registers (roadmap 2.7): a local assigned and read only whole lives in a
//! register, with a `phi` where paths join, and a local whose address is needed keeps its slot.

use super::{Database, build, end, module, parameter, primitive, run, span};
use mir::{
    Argument, BinaryOperator, BlockId, Body, Callee, Local, LocalKind, Operand, Rvalue, Statement,
    StatementKind, TerminatorKind,
};
use types::{Integer, Mode, Primitive};

/// An assignment of `rvalue` to the whole local `local`.
fn set(local: Local, rvalue: Rvalue<'_>) -> Statement<'_> {
    Statement {
        kind: StatementKind::Assign(local.into(), rvalue),
        span: span(),
    }
}

/// `def total(n: i64) -> i64`: adds `0, 1, …, n - 1` in a loop, with the sum and the counter
/// in locals assigned in two blocks each.
fn total(database: &Database) -> Body<'_> {
    let i64 = primitive(database, Primitive::Integer(Integer::I64));
    let bool = primitive(database, Primitive::Bool);
    let mut body = Body::new(i64, span());
    let n = body.push_local(i64, parameter("n", Mode::Borrow), span());
    let sum = body.push_local(i64, LocalKind::Temporary, span());
    let i = body.push_local(i64, LocalKind::Temporary, span());
    let more = body.push_local(bool, LocalKind::Temporary, span());
    let constant = |value: i128| Operand::Constant(mir::Constant::integer(i64, value));
    // bb0: sum = 0; i = 0; goto bb1
    let start = body.push_block(end(TerminatorKind::Goto(BlockId::new(1))));
    body.blocks[start.index()].statements = vec![
        set(sum, Rvalue::Use(constant(0))),
        set(i, Rvalue::Use(constant(0))),
    ];
    // bb1: more = i < n; branch more, bb2, bb3
    let test = body.push_block(end(TerminatorKind::Branch {
        condition: Operand::Copy(more.into()),
        then: BlockId::new(2),
        otherwise: BlockId::new(3),
    }));
    body.blocks[test.index()].statements = vec![set(
        more,
        Rvalue::Binary(
            BinaryOperator::Less,
            Operand::Copy(i.into()),
            Operand::Copy(n.into()),
        ),
    )];
    // bb2: sum = sum + i; i = i + 1; goto bb1
    let step = body.push_block(end(TerminatorKind::Goto(BlockId::new(1))));
    body.blocks[step.index()].statements = vec![
        set(
            sum,
            Rvalue::Binary(
                BinaryOperator::Add,
                Operand::Copy(sum.into()),
                Operand::Copy(i.into()),
            ),
        ),
        set(
            i,
            Rvalue::Binary(BinaryOperator::Add, Operand::Copy(i.into()), constant(1)),
        ),
    ];
    // bb3: return sum
    let done = body.push_block(end(TerminatorKind::Return));
    body.blocks[done.index()].statements =
        vec![set(Local::RETURN, Rvalue::Use(Operand::Copy(sum.into())))];
    body
}

/// The loop's locals need no stack slot: the sum and the counter meet in `phi`s at the loop's
/// test, and the program computes what the slots would.
#[test]
fn loop_locals_are_registers() {
    let database = Database::default();
    let text = module(&database, &[("total", total(&database))]);
    assert!(!text.contains("alloca"), "{text}");
    assert_eq!(text.matches(" = phi i64 ").count(), 2, "{text}");
    let program = build(
        "registers-loop",
        &text,
        "#include <stdio.h>\n#include <stdint.h>\nint64_t total(int64_t);\n\
         int main(void) { printf(\"%lld %lld\\n\", (long long)total(10), (long long)total(0)); }\n",
    );
    assert_eq!(run(&program, &[]).unwrap(), "45 0\n");
}

/// A local passed `InOut` needs an address, so it keeps its slot, and the function is still
/// right: `def twice() -> i64: x = 21; scale(x); return x`.
#[test]
fn a_local_whose_address_is_needed_keeps_its_slot() {
    let database = Database::default();
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let none = types::Type::new(&database, types::TypeKind::None);
    let mut body = Body::new(i64, span());
    let x = body.push_local(i64, LocalKind::Temporary, span());
    let nothing = body.push_local(none, LocalKind::Temporary, span());
    let call = body.push_block(end(TerminatorKind::Call {
        callee: Callee::Function {
            item: super::item(&database, "scale"),
            type_arguments: Vec::new(),
        },
        arguments: vec![Argument::InOut(x.into())],
        destination: nothing.into(),
        target: BlockId::new(1),
        error: None,
    }));
    body.blocks[call.index()].statements = vec![set(
        x,
        Rvalue::Use(Operand::Constant(mir::Constant::integer(i64, 21))),
    )];
    let done = body.push_block(end(TerminatorKind::Return));
    body.blocks[done.index()].statements =
        vec![set(Local::RETURN, Rvalue::Use(Operand::Copy(x.into())))];
    let text = module(&database, &[("twice", body)]);
    assert!(text.contains("%_1 = alloca i64"), "{text}");
    let program = build(
        "registers-slot",
        &text,
        "#include <stdio.h>\n#include <stdint.h>\nint64_t twice(void);\n\
         void scale(int64_t *x) { *x *= 2; }\n\
         int main(void) { printf(\"%lld\\n\", (long long)twice()); }\n",
    );
    assert_eq!(run(&program, &[]).unwrap(), "42\n");
}
