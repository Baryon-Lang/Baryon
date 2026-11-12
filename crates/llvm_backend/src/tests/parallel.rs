//! `prange` loops: the parent packs its shared places into a context, a chunk function runs
//! each chunk's iterations through the outlined body with private reduction copies, and the
//! parent combines the partials in the fixed pairwise tree. The C stand-in for the runtime
//! splits a loop into at most five chunks and runs them last first, so a result that depended
//! on the order chunks run in would show.

use super::{Database, assign, build, end, module, parameter, primitive, run, span};
use mir::{Argument, BinaryOperator, Body, Constant, ConstantValue, Local, LocalKind, Operand};
use mir::{BlockId, Reduction, ReductionOperator, Rvalue, TerminatorKind};
use types::{Float, Integer, Mode, Primitive, Type, TypeKind};

/// `baryon_rt_chunk_count` and `baryon_rt_parallel_for` as the runtime defines them (decision
/// 0021), but with at most five chunks, enough for three levels of the combining tree, run on
/// one thread from the last to the first.
const POOL: &str = r"
    #include <stdint.h>
    typedef void (*Chunk)(void *, int64_t, int64_t, int64_t);
    int64_t baryon_rt_chunk_count(int64_t n) { return n <= 0 ? 0 : n < 5 ? n : 5; }
    void baryon_rt_parallel_for(int64_t n, Chunk body, void *context) {
        int64_t chunks = baryon_rt_chunk_count(n);
        for (int64_t c = chunks - 1; c >= 0; c--) {
            body(context, c, n * c / chunks, n * (c + 1) / chunks);
        }
    }
";

/// What the C stand-in for `baryon_rt_deallocate` prints when a loop of five chunks frees its
/// six partials. The partials live on the heap, not the stack: `clang -O2` moved the read of the
/// combined result past `llvm.stackrestore` when they were a stack array, so it read memory
/// already released.
const FREED_SIX: &str = "freed 48 bytes aligned to 8\n";

/// See [`FREED_SIX`]: a loop of no chunks still has partial 0.
const FREED_ONE: &str = "freed 8 bytes aligned to 8\n";

/// The constant `value` of type `ty`.
fn constant(ty: Type<'_>, value: ConstantValue) -> Operand<'_> {
    Operand::Constant(Constant { ty, value })
}

/// `def total(n: i64, step: i64) -> i64`: `total = 0`, then `total += step` in a `prange` of
/// `n` iterations, with `step` a borrowed argument and `total` an `Add` reduction.
fn total(database: &Database) -> Body<'_> {
    let i64 = primitive(database, Primitive::Integer(Integer::I64));
    let none = Type::new(database, TypeKind::None);
    let mut outlined = Body::new(none, span());
    outlined.push_local(i64, parameter("iteration", Mode::Owned), span());
    let step = outlined.push_local(i64, parameter("step", Mode::Borrow), span());
    let sum = outlined.push_local(i64, parameter("total", Mode::InOut), span());
    let only = outlined.push_block(end(TerminatorKind::Return));
    outlined.blocks[only.index()].statements = vec![assign(
        sum,
        Rvalue::Binary(
            BinaryOperator::Add,
            Operand::Copy(sum.into()),
            Operand::Copy(step.into()),
        ),
    )];
    let mut body = Body::new(i64, span());
    let n = body.push_local(i64, parameter("n", Mode::Borrow), span());
    let step = body.push_local(i64, parameter("step", Mode::Borrow), span());
    let sum = body.push_local(i64, LocalKind::Temporary, span());
    let first = body.push_block(end(TerminatorKind::ParallelFor {
        body: 0,
        count: Operand::Copy(n.into()),
        arguments: vec![Argument::Borrow(step.into())],
        reductions: vec![Reduction {
            place: sum.into(),
            operator: ReductionOperator::Add,
        }],
        accumulations: Vec::new(),
        target: BlockId::new(1),
    }));
    body.blocks[first.index()].statements = vec![assign(
        sum,
        Rvalue::Use(constant(i64, ConstantValue::Integer(0))),
    )];
    let last = body.push_block(end(TerminatorKind::Return));
    body.blocks[last.index()].statements = vec![assign(
        Local::RETURN,
        Rvalue::Use(Operand::Copy(sum.into())),
    )];
    body.outlined.push(outlined);
    body
}

/// The module [`total`] compiles to.
const TOTAL: &str = "\
define i64 @total(i64 %a1, i64 %a2) {
start:
  %_2 = alloca i64
  %_3 = alloca i64
  store i64 %a2, ptr %_2
  br label %bb0
bb0:
  store i64 0, ptr %_3
  %t0 = call ptr @llvm.stacksave()
  %t1 = alloca [2 x ptr]
  %t2 = getelementptr inbounds [2 x ptr], ptr %t1, i64 0, i64 0
  store ptr %_2, ptr %t2
  %t3 = call i64 @baryon_rt_chunk_count(i64 %a1)
  %t4 = add i64 %t3, 1
  %t5 = call ptr @baryon_rt_allocate_array(i64 %t4, i64 8, i64 8, ptr @baryon.loc.1)
  store i64 0, ptr %t5
  %t6 = getelementptr inbounds [2 x ptr], ptr %t1, i64 0, i64 1
  store ptr %t5, ptr %t6
  call void @baryon_rt_parallel_for(i64 %a1, ptr @total.chunk0, ptr %t1)
  %t7 = alloca i64
  %t8 = alloca i64
  store i64 1, ptr %t7
  br label %c0
c0:
  %t9 = load i64, ptr %t7
  %t10 = icmp slt i64 %t9, %t3
  br i1 %t10, label %c1, label %c5
c1:
  %t11 = shl i64 %t9, 1
  store i64 0, ptr %t8
  br label %c2
c2:
  %t12 = load i64, ptr %t8
  %t13 = add i64 %t12, %t9
  %t14 = icmp slt i64 %t13, %t3
  br i1 %t14, label %c3, label %c4
c3:
  %t15 = getelementptr inbounds i64, ptr %t5, i64 %t12
  %t16 = getelementptr inbounds i64, ptr %t5, i64 %t13
  %t17 = load i64, ptr %t15
  %t18 = load i64, ptr %t16
  %t19 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %t17, i64 %t18)
  %t20 = extractvalue { i64, i1 } %t19, 1
  br i1 %t20, label %p0, label %c6
c6:
  %t21 = extractvalue { i64, i1 } %t19, 0
  store i64 %t21, ptr %t15
  %t22 = add i64 %t12, %t11
  store i64 %t22, ptr %t8
  br label %c2
c4:
  store i64 %t11, ptr %t7
  br label %c0
c5:
  %t23 = load i64, ptr %t5
  %t24 = load i64, ptr %_3
  %t25 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %t24, i64 %t23)
  %t26 = extractvalue { i64, i1 } %t25, 1
  br i1 %t26, label %p1, label %c7
c7:
  %t27 = extractvalue { i64, i1 } %t25, 0
  store i64 %t27, ptr %_3
  %t28 = mul i64 %t4, 8
  call void @baryon_rt_deallocate(ptr %t5, i64 %t28, i64 8)
  call void @llvm.stackrestore(ptr %t0)
  br label %bb1
bb1:
  %t29 = load i64, ptr %_3
  ret i64 %t29
p0:
  call void @baryon_rt_panic(ptr @baryon.str.2, i64 23, ptr @baryon.loc.1)
  unreachable
p1:
  call void @baryon_rt_panic(ptr @baryon.str.2, i64 23, ptr @baryon.loc.1)
  unreachable
}

define internal void @total.chunk0(ptr %context, i64 %chunk, i64 %first, i64 %last) {
entry:
  %s0 = getelementptr inbounds [2 x ptr], ptr %context, i64 0, i64 0
  %p0 = load ptr, ptr %s0
  %v0 = load i64, ptr %p0
  %s1 = getelementptr inbounds [2 x ptr], ptr %context, i64 0, i64 1
  %p1 = load ptr, ptr %s1
  %r0 = alloca i64
  store i64 0, ptr %r0
  br label %test
test:
  %iteration = phi i64 [ %first, %entry ], [ %next, %step ]
  %more = icmp slt i64 %iteration, %last
  br i1 %more, label %step, label %done
step:
  call void @total.body0(i64 %iteration, i64 %v0, ptr %r0)
  %next = add i64 %iteration, 1
  br label %test
done:
  %f0 = load i64, ptr %r0
  %e0 = getelementptr inbounds i64, ptr %p1, i64 %chunk
  store i64 %f0, ptr %e0
  ret void
}

define internal void @total.body0(i64 %a1, i64 %a2, ptr noalias align 8 dereferenceable(8) %a3) {
start:
  br label %bb0
bb0:
  %t0 = load i64, ptr %a3
  %t1 = add i64 %t0, %a2
  store i64 %t1, ptr %a3
  ret void
}

@baryon.str.0 = private unnamed_addr constant [8 x i8] c\"heat.byn\"
@baryon.loc.1 = private unnamed_addr constant { ptr, i64, i32, i32 } { ptr @baryon.str.0, i64 8, i32 1, i32 1 }
@baryon.str.2 = private unnamed_addr constant [23 x i8] c\"integer overflow in `+`\"

declare noalias nonnull ptr @baryon_rt_allocate_array(i64, i64, i64, ptr) nounwind
declare i64 @baryon_rt_chunk_count(i64) nounwind
declare void @baryon_rt_deallocate(ptr, i64, i64) nounwind
declare void @baryon_rt_panic(ptr, i64, ptr) cold noreturn nounwind
declare void @baryon_rt_parallel_for(i64, ptr, ptr)
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
declare void @llvm.stackrestore(ptr)
declare ptr @llvm.stacksave()
";

/// The parent stores a pointer to `step` and to the partials in the context, runs the loop,
/// sums the partials pairwise, stride 1, 2, 4, …, with an overflow check, and adds the result into
/// `total`; the chunk function loads `step` once and passes each iteration a private `total`.
#[test]
fn a_prange_loop_runs_chunks_and_combines_partials_in_order() {
    let database = Database::default();
    assert_eq!(module(&database, &[("total", total(&database))]), TOTAL);
}

/// The loop sums `step` once per iteration across the chunks; a loop of no iterations leaves
/// `total` as it was, and a sum that overflows only when the partials combine still panics.
#[test]
fn a_reduction_gives_the_serial_result_and_checks_the_combine() {
    let database = Database::default();
    let text = module(&database, &[("total", total(&database))]);
    let main = r#"
        #include <stdint.h>
        #include <stdio.h>
        #include <stdlib.h>
        int64_t total(int64_t, int64_t);
        int main(int argc, char **argv) {
            (void)argc;
            printf("%lld\n", (long long)total(atoll(argv[1]), atoll(argv[2])));
            return 0;
        }
    "#;
    let executable = build("parallel-total", &text, &format!("{main}{POOL}"));
    assert_eq!(
        run(&executable, &["10", "3"]),
        Ok(format!("{FREED_SIX}30\n").to_owned())
    );
    assert_eq!(run(&executable, &["0", "3"]), Ok(format!("{FREED_ONE}0\n")));
    assert_eq!(
        run(&executable, &["3", "4611686018427387904"]),
        Err("panic: integer overflow in `+`\n  --> heat.byn:1:1\n".to_owned())
    );
}

/// `low = min(low, (i - 4.5) ** 2)` over a `prange`, written as a branch, reduces with `Min`
/// from `inf`: the smallest value over the chunks, or `low` itself for an empty loop.
#[test]
fn a_min_reduction_starts_at_infinity() {
    let database = Database::default();
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let bool = primitive(&database, Primitive::Bool);
    let none = Type::new(&database, TypeKind::None);
    let mut outlined = Body::new(none, span());
    let iteration = outlined.push_local(i64, parameter("iteration", Mode::Owned), span());
    let low = outlined.push_local(f64, parameter("low", Mode::InOut), span());
    let value = outlined.push_local(f64, LocalKind::Temporary, span());
    let smaller = outlined.push_local(bool, LocalKind::Temporary, span());
    let first = outlined.push_block(end(TerminatorKind::Branch {
        condition: Operand::Copy(smaller.into()),
        then: BlockId::new(1),
        otherwise: BlockId::new(2),
    }));
    let half = constant(f64, ConstantValue::Float(4.5_f64.to_bits()));
    outlined.blocks[first.index()].statements = vec![
        assign(value, Rvalue::Convert(Operand::Copy(iteration.into()), f64)),
        assign(
            value,
            Rvalue::Binary(BinaryOperator::Subtract, Operand::Copy(value.into()), half),
        ),
        assign(
            value,
            Rvalue::Binary(
                BinaryOperator::Multiply,
                Operand::Copy(value.into()),
                Operand::Copy(value.into()),
            ),
        ),
        assign(
            smaller,
            Rvalue::Binary(
                BinaryOperator::Less,
                Operand::Copy(value.into()),
                Operand::Copy(low.into()),
            ),
        ),
    ];
    let take = outlined.push_block(end(TerminatorKind::Goto(BlockId::new(2))));
    outlined.blocks[take.index()].statements =
        vec![assign(low, Rvalue::Use(Operand::Copy(value.into())))];
    outlined.push_block(end(TerminatorKind::Return));
    let mut body = Body::new(f64, span());
    let n = body.push_local(i64, parameter("n", Mode::Borrow), span());
    let start = body.push_local(f64, parameter("start", Mode::Borrow), span());
    let low = body.push_local(f64, LocalKind::Temporary, span());
    let first = body.push_block(end(TerminatorKind::ParallelFor {
        body: 0,
        count: Operand::Copy(n.into()),
        arguments: Vec::new(),
        reductions: vec![Reduction {
            place: low.into(),
            operator: ReductionOperator::Min,
        }],
        accumulations: Vec::new(),
        target: BlockId::new(1),
    }));
    body.blocks[first.index()].statements =
        vec![assign(low, Rvalue::Use(Operand::Copy(start.into())))];
    let last = body.push_block(end(TerminatorKind::Return));
    body.blocks[last.index()].statements = vec![assign(
        Local::RETURN,
        Rvalue::Use(Operand::Copy(low.into())),
    )];
    body.outlined.push(outlined);
    let text = module(&database, &[("lowest", body)]);
    assert!(text.contains("store double 0x7FF0000000000000, ptr %r0"));
    let main = r#"
        #include <stdint.h>
        #include <stdio.h>
        #include <stdlib.h>
        double lowest(int64_t, double);
        int main(int argc, char **argv) {
            (void)argc;
            printf("%g\n", lowest(atoll(argv[1]), atof(argv[2])));
            return 0;
        }
    "#;
    let executable = build("parallel-min", &text, &format!("{main}{POOL}"));
    assert_eq!(
        run(&executable, &["10", "100"]),
        Ok(format!("{FREED_SIX}0.25\n"))
    );
    assert_eq!(
        run(&executable, &["10", "0.1"]),
        Ok(format!("{FREED_SIX}0.1\n"))
    );
    assert_eq!(run(&executable, &["0", "7"]), Ok(format!("{FREED_ONE}7\n")));
}
