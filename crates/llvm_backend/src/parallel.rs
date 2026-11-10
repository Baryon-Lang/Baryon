//! `prange` loops (decision 0018): a [`ParallelFor`](mir::TerminatorKind::ParallelFor) runs
//! its outlined body once per iteration through the runtime's thread pool, and combines each
//! reduction's per-chunk results in a fixed order.
//!
//! A function `f` with one `prange` loop becomes three LLVM functions:
//!
//! - `f` itself, which packs a pointer to each shared place into a context, calls
//!   `baryon_rt_parallel_for`, then combines the partial results of each reduction;
//! - `f.body0`, the outlined body, written like any other body, which runs one iteration;
//! - `f.chunk0`, the chunk function the runtime calls, `(context, chunk, start, end)`, which
//!   unpacks the context, gives each reduction a private copy at the operator's identity, calls
//!   `f.body0` for each iteration of the chunk in order, then stores each copy as the chunk's
//!   partial result.
//!
//! The runtime's chunks depend only on the iteration count (decision 0021), and the partials
//! combine in a fixed pairwise tree that depends only on the chunk count, never on the thread
//! count, so a float reduction gives the same bits on every run and every machine (the
//! reference, "Reproducible reductions"). The partials, one per chunk and at most 1024 per
//! reduction, live on the heap and are freed once combined. They cannot live in the stack memory
//! the loop releases when it ends: `clang -O2` may move the read of the combined result past
//! `llvm.stackrestore`, which then reads stack memory already given back.

use crate::Unsupported;
use crate::accumulate;
use crate::function::{Function, block_label};
use crate::literal::{float, global};
use crate::ty::Number;
use mir::ReductionOperator;
use mir::{Argument, BinaryOperator, BlockId, Body, Check, LocalKind, Operand, Place, Reduction};
use std::fmt::Write as _;
use types::{Layout, Mode, Type};

/// The runtime functions a `prange` loop calls, as the runtime exports them (decision 0021).
const CHUNK_COUNT: &str = "declare i64 @baryon_rt_chunk_count(i64) nounwind";

/// See [`CHUNK_COUNT`].
const PARALLEL_FOR: &str = "declare void @baryon_rt_parallel_for(i64, ptr, ptr)";

/// The intrinsics that release the context's stack memory when a loop ends, so a `prange`
/// inside a serial loop does not grow the stack on every pass.
const STACK_SAVE: &str = "declare ptr @llvm.stacksave()";

/// See [`STACK_SAVE`].
const STACK_RESTORE: &str = "declare void @llvm.stackrestore(ptr)";

/// How one argument reaches the outlined body: through its address, or loaded from it.
#[derive(Clone, Debug)]
enum Passed {
    /// As a pointer: an `InOut` argument, or a borrow of a value passed by reference.
    Pointer,
    /// As a value of this LLVM type, loaded once per chunk.
    Value(String),
}

/// One reduction of a loop, as the parent function writes it.
struct Reduced<'db> {
    /// How contributions combine.
    operator: ReductionOperator,
    /// The address of the reduced place in the parent.
    address: String,
    /// The reduced value's type.
    ty: Type<'db>,
    /// Its kind of number.
    number: Number,
    /// Its LLVM type.
    llvm: String,
    /// The operator's identity for that type, which each chunk's copy starts at.
    identity: String,
    /// The value's layout, which sizes the partials' heap memory.
    layout: Layout,
}

impl<'db> Function<'_, '_, 'db> {
    /// Writes a `prange` loop: the outlined body `index` run `count` times with `arguments`,
    /// reducing into `reductions` and accumulating into `accumulations`, then a jump to
    /// `target`. The chunk function goes into [`helpers`](Function::helpers).
    pub(crate) fn parallel_for(
        &mut self,
        index: u32,
        count: &Operand<'db>,
        arguments: &[Argument<'db>],
        reductions: &[Reduction],
        accumulations: &[Place],
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let outlined = self
            .body
            .outlined
            .get(index as usize)
            .ok_or_else(|| Unsupported::new("a `prange` loop whose body is missing"))?;
        let body_symbol = format!("{}.body{index}", self.name);
        let chunk_symbol = format!("{}.chunk{index}", self.name);
        let passed = self.passed(outlined, arguments.len())?;
        let mut reduced = Vec::new();
        for reduction in reductions {
            let (address, ty) = self.place(&reduction.place)?;
            let (number, llvm) = self.scalar(ty)?;
            let identity = identity(reduction.operator, number, &llvm)?;
            let layout = types::layout(self.types.db, ty, self.types.declared)
                .map_err(|_| self.types.unsupported(ty))?;
            reduced.push(Reduced {
                operator: reduction.operator,
                address,
                ty,
                number,
                llvm,
                identity,
                layout,
            });
        }
        let (iterations, _) = self.value(count)?;
        self.declare("llvm.stacksave", STACK_SAVE);
        self.declare("llvm.stackrestore", STACK_RESTORE);
        self.declare("baryon_rt_parallel_for", PARALLEL_FOR);
        let accumulated = self.start_accumulators(accumulations, &iterations)?;
        let saved = self.emit("call ptr @llvm.stacksave()");
        let slots = arguments.len() + reductions.len() + 2 * accumulated.len();
        let context = self.emit(&format!("alloca [{slots} x ptr]"));
        for (position, argument) in arguments.iter().enumerate() {
            let address = self.argument_address(argument)?;
            self.store_slot(&context, slots, position, &address);
        }
        let first = arguments.len() + reductions.len();
        for (index, array) in accumulated.iter().enumerate() {
            self.store_slot(&context, slots, first + 2 * index, &array.address);
            self.store_slot(&context, slots, first + 2 * index + 1, &array.accumulator);
        }
        if !accumulated.is_empty() {
            accumulate::declare_chunk_done(self);
        }
        let chunks = if reductions.is_empty() {
            None
        } else {
            self.declare("baryon_rt_chunk_count", CHUNK_COUNT);
            Some(self.emit(&format!(
                "call i64 @baryon_rt_chunk_count(i64 {iterations})"
            )))
        };
        let mut partials = Vec::new();
        let mut partial_count = None;
        if let Some(chunks) = &chunks {
            // One more than the chunks, so a loop with none still has partial 0: the identity,
            // which chunk 0 overwrites when it runs.
            let room = self.emit(&format!("add i64 {chunks}, 1"));
            for (position, reduction) in reduced.iter().enumerate() {
                let llvm = &reduction.llvm;
                let partial = self.allocate_elements(&room, reduction.layout);
                self.line(&format!(
                    "store {llvm} {}, ptr {partial}",
                    reduction.identity
                ));
                self.store_slot(&context, slots, arguments.len() + position, &partial);
                partials.push(partial);
            }
            partial_count = Some(room);
        }
        self.line(&format!(
            "call void @baryon_rt_parallel_for(i64 {iterations}, ptr {}, ptr {context})",
            global(&chunk_symbol)
        ));
        if let Some(chunks) = &chunks {
            for (reduction, partial) in reduced.iter().zip(&partials) {
                self.combine_partials(reduction, partial, chunks)?;
            }
        }
        if let Some(room) = &partial_count {
            // The combined results are stored by now, so the partials can go.
            for (reduction, partial) in reduced.iter().zip(&partials) {
                let bytes = self.emit(&format!("mul i64 {room}, {}", reduction.layout.size()));
                self.deallocate(partial, &bytes, reduction.layout);
            }
        }
        self.finish_accumulators(&accumulated);
        self.line(&format!("call void @llvm.stackrestore(ptr {saved})"));
        self.line(&format!("br label %{}", block_label(target)));
        let reductions: Vec<(String, String)> = reduced
            .into_iter()
            .map(|reduction| (reduction.llvm, reduction.identity))
            .collect();
        let headers: Vec<String> = accumulated.into_iter().map(|array| array.header).collect();
        self.helpers.push(chunk_function(
            &chunk_symbol,
            &body_symbol,
            &passed,
            &reductions,
            &headers,
        ));
        Ok(())
    }

    /// How each of the `count` argument parameters of `outlined` arrives: its parameters after
    /// the iteration number and before the reductions, in the modes the outlined body's
    /// prologue gives them.
    fn passed(&self, outlined: &Body<'db>, count: usize) -> Result<Vec<Passed>, Unsupported> {
        let mut passed = Vec::new();
        for local in outlined.locals.iter().skip(2).take(count) {
            let LocalKind::Parameter { mode, .. } = &local.kind else {
                break;
            };
            passed.push(match mode {
                Mode::InOut => Passed::Pointer,
                Mode::Borrow if !self.types.by_value(local.ty) => Passed::Pointer,
                Mode::Borrow | Mode::Owned => Passed::Value(self.types.value(local.ty)?),
            });
        }
        Ok(passed)
    }

    /// The address the context holds for `argument`: the place itself, or for an `Owned`
    /// value, a copy of it on the stack.
    fn argument_address(&mut self, argument: &Argument<'db>) -> Result<String, Unsupported> {
        match argument {
            Argument::Borrow(place) | Argument::InOut(place) => Ok(self.place(place)?.0),
            Argument::Owned(operand) => {
                let (value, ty) = self.value(operand)?;
                if !self.types.by_value(ty) {
                    return Err(Unsupported::new("a `prange` body that owns a shared value"));
                }
                let llvm = self.types.value(ty)?;
                let copy = self.emit(&format!("alloca {llvm}"));
                self.line(&format!("store {llvm} {value}, ptr {copy}"));
                Ok(copy)
            }
        }
    }

    /// Stores `address` in slot `position` of the context, an array of `slots` pointers.
    fn store_slot(&mut self, context: &str, slots: usize, position: usize, address: &str) {
        let slot = self.emit(&format!(
            "getelementptr inbounds [{slots} x ptr], ptr {context}, i64 0, i64 {position}"
        ));
        self.line(&format!("store ptr {address}, ptr {slot}"));
    }

    /// Combines the `chunks` partial results of `reduction` at `partial` in the fixed pairwise
    /// tree, then combines the result into the reduced place.
    ///
    /// The tree (the reference, "Reproducible reductions"): for a stride of 1, 2, 4, … while it
    /// is below the chunk count, each partial `i` at a multiple of twice the stride becomes
    /// `p[i] ⊕ p[i + stride]` when `i + stride` is a chunk. Partial 0 then holds the result, from
    /// at most 11 levels for 1024 chunks, so a float sum's rounding error grows with the tree's
    /// depth rather than the chunk count. The tree depends only on the chunk count.
    fn combine_partials(
        &mut self,
        reduction: &Reduced<'db>,
        partial: &str,
        chunks: &str,
    ) -> Result<(), Unsupported> {
        let Reduced {
            operator,
            address,
            ty,
            number,
            llvm,
            ..
        } = reduction;
        let (operator, number, ty) = (*operator, *number, *ty);
        let stride_slot = self.emit("alloca i64");
        let index_slot = self.emit("alloca i64");
        self.line(&format!("store i64 1, ptr {stride_slot}"));
        let [level, pairs, pair, combine, next_level, done] =
            std::array::from_fn(|_| self.fresh_block());
        self.line(&format!("br label %{level}"));
        self.line_label(&level);
        let stride = self.emit(&format!("load i64, ptr {stride_slot}"));
        let deeper = self.emit(&format!("icmp slt i64 {stride}, {chunks}"));
        self.line(&format!("br i1 {deeper}, label %{pairs}, label %{done}"));
        self.line_label(&pairs);
        let twice = self.emit(&format!("shl i64 {stride}, 1"));
        self.line(&format!("store i64 0, ptr {index_slot}"));
        self.line(&format!("br label %{pair}"));
        self.line_label(&pair);
        let left = self.emit(&format!("load i64, ptr {index_slot}"));
        let right = self.emit(&format!("add i64 {left}, {stride}"));
        let present = self.emit(&format!("icmp slt i64 {right}, {chunks}"));
        self.line(&format!(
            "br i1 {present}, label %{combine}, label %{next_level}"
        ));
        self.line_label(&combine);
        let left_address = self.emit(&format!(
            "getelementptr inbounds {llvm}, ptr {partial}, i64 {left}"
        ));
        let right_address = self.emit(&format!(
            "getelementptr inbounds {llvm}, ptr {partial}, i64 {right}"
        ));
        let a = self.emit(&format!("load {llvm}, ptr {left_address}"));
        let b = self.emit(&format!("load {llvm}, ptr {right_address}"));
        let combined = self.combine(operator, number, llvm, &a, &b)?;
        self.line(&format!("store {llvm} {combined}, ptr {left_address}"));
        let following = self.emit(&format!("add i64 {left}, {twice}"));
        self.line(&format!("store i64 {following}, ptr {index_slot}"));
        self.line(&format!("br label %{pair}"));
        self.line_label(&next_level);
        self.line(&format!("store i64 {twice}, ptr {stride_slot}"));
        self.line(&format!("br label %{level}"));
        self.line_label(&done);
        let reduced = self.emit(&format!("load {llvm}, ptr {partial}"));
        let before = self
            .load(address, ty)?
            .ok_or_else(|| self.types.unsupported(ty))?;
        let after = self.combine(operator, number, llvm, &before, &reduced)?;
        self.line(&format!("store {llvm} {after}, ptr {address}"));
        Ok(())
    }

    /// `a` combined with `b` by `operator`. An integer sum or product that overflows panics, as
    /// the same arithmetic in the loop body does; `min` and `max` follow Python, taking `b`
    /// only when it is strictly smaller or larger.
    fn combine(
        &mut self,
        operator: ReductionOperator,
        number: Number,
        llvm: &str,
        a: &str,
        b: &str,
    ) -> Result<String, Unsupported> {
        let pick = |comparison: &str| format!("{comparison} {llvm} {b}, {a}");
        Ok(match (operator, number) {
            (ReductionOperator::Add, Number::Float(_)) => {
                self.emit(&format!("fadd {llvm} {a}, {b}"))
            }
            (ReductionOperator::Multiply, Number::Float(_)) => {
                self.emit(&format!("fmul {llvm} {a}, {b}"))
            }
            (ReductionOperator::Add, Number::Integer { signed, .. }) => {
                self.checked(BinaryOperator::Add, signed, llvm, a, b)
            }
            (ReductionOperator::Multiply, Number::Integer { signed, .. }) => {
                self.checked(BinaryOperator::Multiply, signed, llvm, a, b)
            }
            (ReductionOperator::Min | ReductionOperator::Max, _) => {
                let comparison = match (operator, number) {
                    (ReductionOperator::Min, Number::Float(_)) => "fcmp olt",
                    (ReductionOperator::Max, Number::Float(_)) => "fcmp ogt",
                    (ReductionOperator::Min, Number::Integer { signed: true, .. }) => "icmp slt",
                    (ReductionOperator::Min, Number::Integer { .. }) => "icmp ult",
                    (ReductionOperator::Max, Number::Integer { signed: true, .. }) => "icmp sgt",
                    (ReductionOperator::Max, Number::Integer { .. }) => "icmp ugt",
                    _ => return Err(Unsupported::new("a `prange` reduction of a `bool`")),
                };
                let smaller = self.emit(&pick(comparison));
                self.emit(&format!("select i1 {smaller}, {llvm} {b}, {llvm} {a}"))
            }
            (_, Number::Bool) => {
                return Err(Unsupported::new("a `prange` reduction of a `bool`"));
            }
        })
    }

    /// `a` plus or times `b`, integers, with a panic on overflow; wrapping instead in a release
    /// build, as the loop body's own arithmetic does there (decision 0030).
    fn checked(
        &mut self,
        operator: BinaryOperator,
        signed: bool,
        llvm: &str,
        a: &str,
        b: &str,
    ) -> String {
        let operation = if operator == BinaryOperator::Add {
            "add"
        } else {
            "mul"
        };
        if self.constants.wraps {
            return self.emit(&format!("{operation} {llvm} {a}, {b}"));
        }
        let sign = if signed { 's' } else { 'u' };
        let name = format!("llvm.{sign}{operation}.with.overflow.{llvm}");
        self.declare(
            &name,
            &format!("declare {{ {llvm}, i1 }} @{name}({llvm}, {llvm})"),
        );
        let pair = self.emit(&format!(
            "call {{ {llvm}, i1 }} @{name}({llvm} {a}, {llvm} {b})"
        ));
        let overflowed = self.emit(&format!("extractvalue {{ {llvm}, i1 }} {pair}, 1"));
        let failure = self.overflow(operator);
        let failed = self.failure_block(&failure);
        let fine = self.fresh_block();
        self.line(&format!(
            "br i1 {overflowed}, label %{failed}, label %{fine}"
        ));
        self.line_label(&fine);
        self.emit(&format!("extractvalue {{ {llvm}, i1 }} {pair}, 0"))
    }

    /// The failure an overflow of `operator` reports.
    fn overflow(&mut self, operator: BinaryOperator) -> crate::panic::Failure {
        match self.failure(&Check::NoOverflow(operator)) {
            Ok(failure) => failure,
            Err(_) => crate::panic::Failure::Message("integer overflow".to_owned()),
        }
    }
}

/// The value a chunk's private copy starts at: the operator's identity for the LLVM type `llvm`.
fn identity(
    operator: ReductionOperator,
    number: Number,
    llvm: &str,
) -> Result<String, Unsupported> {
    let single = llvm == "float";
    Ok(match (operator, number) {
        (_, Number::Bool) => return Err(Unsupported::new("a `prange` reduction of a `bool`")),
        // -0.0, not 0.0: `-0.0 + x` is `x` for every `x`, `-0.0` included, so an empty loop
        // leaves a negative zero as it was.
        (ReductionOperator::Add, Number::Float(_)) => float(-0.0, single),
        (ReductionOperator::Multiply, Number::Float(_)) => float(1.0, single),
        (ReductionOperator::Min, Number::Float(_)) => float(f64::INFINITY, single),
        (ReductionOperator::Max, Number::Float(_)) => float(f64::NEG_INFINITY, single),
        (ReductionOperator::Multiply, Number::Integer { .. }) => "1".to_owned(),
        (ReductionOperator::Min, Number::Integer { bits, signed: true }) => {
            ((1_i128 << (bits - 1)) - 1).to_string()
        }
        (ReductionOperator::Max, Number::Integer { bits, signed: true }) => {
            (-(1_i128 << (bits - 1))).to_string()
        }
        // Unsigned: the largest is all ones, which LLVM writes as -1.
        (ReductionOperator::Min, Number::Integer { .. }) => "-1".to_owned(),
        // Unsigned, the smallest is 0, the identity of a sum too.
        (ReductionOperator::Add | ReductionOperator::Max, Number::Integer { .. }) => "0".to_owned(),
    })
}

/// The chunk function `symbol` the runtime calls with `(context, chunk, first, last)`: it
/// loads each argument from the context as `passed` says, starts each reduction's private copy
/// at its identity, calls `body` for iterations `first..last` in order, and stores each copy
/// in the chunk's place among the reduction's partials. `reductions` holds each reduced
/// value's LLVM type and identity, and `accumulated` each accumulated array's header type: the
/// body gets a view of each array, and the runtime hears when the chunk is done.
fn chunk_function(
    symbol: &str,
    body: &str,
    passed: &[Passed],
    reductions: &[(String, String)],
    accumulated: &[String],
) -> String {
    let slots = passed.len() + reductions.len() + 2 * accumulated.len();
    let mut entry = String::new();
    let mut arguments = vec!["i64 %iteration".to_owned()];
    let slot = |entry: &mut String, position: usize| {
        let _ = writeln!(
            entry,
            "  %s{position} = getelementptr inbounds [{slots} x ptr], ptr %context, i64 0, \
             i64 {position}\n  %p{position} = load ptr, ptr %s{position}"
        );
    };
    for (position, how) in passed.iter().enumerate() {
        slot(&mut entry, position);
        match how {
            Passed::Pointer => arguments.push(format!("ptr %p{position}")),
            Passed::Value(llvm) => {
                let _ = writeln!(entry, "  %v{position} = load {llvm}, ptr %p{position}");
                arguments.push(format!("{llvm} %v{position}"));
            }
        }
    }
    let mut finish = String::new();
    for (index, (llvm, identity)) in reductions.iter().enumerate() {
        let position = passed.len() + index;
        slot(&mut entry, position);
        let _ = writeln!(
            entry,
            "  %r{index} = alloca {llvm}\n  store {llvm} {identity}, ptr %r{index}"
        );
        arguments.push(format!("ptr %r{index}"));
        let _ = writeln!(
            finish,
            "  %f{index} = load {llvm}, ptr %r{index}\n  \
             %e{index} = getelementptr inbounds {llvm}, ptr %p{position}, i64 %chunk\n  \
             store {llvm} %f{index}, ptr %e{index}"
        );
    }
    let [views, viewed, done] =
        accumulate::chunk_views(accumulated, passed.len() + reductions.len(), slots);
    entry.push_str(&views);
    if !viewed.is_empty() {
        arguments.push(viewed);
    }
    finish.push_str(&done);
    format!(
        "define internal void {}(ptr %context, i64 %chunk, i64 %first, i64 %last) {{\n\
         entry:\n{entry}  br label %test\n\
         test:\n  %iteration = phi i64 [ %first, %entry ], [ %next, %step ]\n  \
         %more = icmp slt i64 %iteration, %last\n  br i1 %more, label %step, label %done\n\
         step:\n  call void {}({})\n  %next = add i64 %iteration, 1\n  br label %test\n\
         done:\n{finish}  ret void\n}}\n",
        global(symbol),
        global(body),
        arguments.join(", ")
    )
}
