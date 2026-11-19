//! The chunk function of a `prange` loop (decision 0018): the function the runtime calls for
//! each chunk of iterations, which unpacks the loop's context, gives each reduction a private
//! copy, and calls the outlined body once per iteration. It is plain LLVM text, written apart
//! from the [`parallel`](crate::parallel) module that emits the loop itself.

use crate::accumulate;
use crate::literal::global;
use std::fmt::Write as _;

/// How one argument reaches the outlined body: through its address, or loaded from it.
#[derive(Clone, Debug)]
pub(crate) enum Passed {
    /// Not at all: a value that takes no bytes, such as a function the body calls, whose type
    /// already names it (decision 0026). The outlined body has no LLVM parameter for it, as no
    /// function does, so its context slot is left unwritten and never read.
    Nothing,
    /// As a pointer: an `InOut` argument, or a borrow of a value passed by reference.
    Pointer,
    /// As a value of this LLVM type, loaded once per chunk.
    Value(String),
}

/// The chunk function `symbol` the runtime calls with `(context, chunk, first, last)`: it
/// loads each argument from the context as `passed` says, starts each reduction's private copy
/// at its identity, calls `body` for iterations `first..last` in order, and stores each copy
/// in the chunk's place among the reduction's partials. `reductions` holds each reduced
/// value's LLVM type and identity, and `accumulated` each accumulated array's header type: the
/// body gets a view of each array, and the runtime hears when the chunk is done.
pub(crate) fn chunk_function(
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
        if matches!(how, Passed::Nothing) {
            continue;
        }
        slot(&mut entry, position);
        match how {
            Passed::Nothing => {}
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
