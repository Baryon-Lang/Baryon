# 0024. Accumulated arrays compile to an `Accumulate` statement over lazily made private blocks

Status: Proposed
Date: 2026-11-08
Roadmap: 1.12.1

## Context

Decision [0013](0013-parallel-accumulation.md) settles what `prange(n, accumulate=forces)`
means. Inside the loop, the program may only add into `forces`, at any index. The result is the
serial loop's result, grouped in the fixed reduction tree, so it is the same bits on any number of
threads. The checker enforces those rules today (roadmap 1.8). Lowering refuses the argument
with "not lowered yet".

That record leaves the implementation to "blocked private buffers" behind the runtime interface.
Building it crosses four crates, so this record settles the shape before code
(`CONTRIBUTING.md` rule 7):

- `mir` needs a way to say "add into this element".
- Lowering emits it.
- The runtime holds the private blocks.
- The LLVM backend connects the three.

Three facts constrain the design:

1. **The scalar path is already built** (pull request #76). A `ParallelFor` passes each shared
   place to an outlined body, and each reduction gets a private copy per chunk at the operator's
   identity. The partials combine in the reference's pairwise tree, and the value from before the
   loop comes in last.
2. **The body must stay ordinary code.** A bounds check reads the array's lengths through the
   same parameter it would outside a loop. A second kind of array type for one loop argument
   would spread through every pass.
3. **Memory is the reason for blocks.** One private copy of the array per chunk is 1024 copies.
   The reference ("Accumulating into shared arrays") promises about one extra copy for spatially
   sorted data.

## Decision

### In `mir`: an accumulation list and one new statement

`TerminatorKind::ParallelFor` gains `accumulations: Vec<Place>`, the arrays listed in
`accumulate=`. The outlined body takes one more parameter per accumulation, after the
reductions, in `InOut` mode with the array's own type.

A new statement adds into an element:

```text
Accumulate { place: Place, operator: AccumulateOperator, value: Operand }
```

- `place` is the accumulated parameter projected by `Index`, and possibly then into a `Fixed`
  element.
- `operator` is `Add` or `Subtract`. `x -= e` stays a subtraction rather than adding `-e`, so an
  integer `e` equal to the type's minimum is not negated.
- `value` has the element's type.

The validator accepts `Accumulate` only in an outlined body, and only on a parameter that the
body's `ParallelFor` lists in `accumulations`. Lowering uses that parameter for nothing else
except `Len`, for bounds checks. The checker's rules guarantee this, so the validator does not
repeat it.

### In lowering

In a body, `forces[j] += e` and `forces[j] -= e` on an accumulated array lower to the usual
bounds checks and then one `Accumulate`. Nothing else changes, because the checker has already
rejected every other use.

### In the runtime: lazy blocks, combined as soon as a pair of chunks is done

An *accumulator* is a private sum per chunk for each element. The runtime stores each chunk's
sums in fixed-size blocks of the array's scalars, which are the numbers inside its elements: a
`Fixed[f64, 3]` element is three `f64` scalars.

A chunk makes a block the first time it adds into it, filled with the identity: `-0.0` for
floats, which is the exact additive identity, and `0` for integers. A block a chunk never touched
is therefore exactly the identity. Leaving it out of a sum changes no bits, so skipping it is
exact, not an approximation. The block size changes performance only, never results. It starts
at 4 KiB and changes only with a benchmark.

New exported functions:

| Function | Job |
| --- | --- |
| `baryon_rt_accumulator_new(iterations, scalars, kind, location)` | An accumulator for a loop of `iterations` iterations into an array of `scalars` scalars of one `kind`: `i8` to `u64`, `f32` or `f64` |
| `baryon_rt_accumulator_slot(accumulator, chunk, scalar)` | The address of scalar `scalar` in chunk `chunk`'s private copy, making the chunk's block table and the block if needed |
| `baryon_rt_accumulator_chunk_done(accumulator, chunk)` | Called by a chunk when it ends: combines the subtrees this chunk completes, freeing the blocks it absorbs |
| `baryon_rt_accumulator_finish(accumulator, data, rank, lengths, strides, lanes)` | Adds the combined sums into the array, with the value from before the loop first in each addition, then frees everything |

The combine is the reference's pairwise tree. The tree applies, element by element, to the
chunk sums. Node `(i, stride)` is ready when both of its halves are. Whichever chunk finishes
the second half combines it, so the tree is the same and only the moment it runs changes.
Combining a block present on only one side moves it without adding. An integer overflow in the
combine panics with the loop's location, as the scalar combine does in pull request #76.

### What it costs in memory

The thread pool hands out chunks in increasing order, so subtrees close in near order. A
finished subtree stays alive only while it waits for its sibling, and the sibling holds an
unfinished chunk: one still running, or the next one not yet started. Each unfinished chunk keeps
at most one waiting subtree per tree level alive, the finished siblings on its path to the
root. With `L = ceil(log2(chunks))` levels, which is 10 for the full 1024 chunks, the live
partial copies are at most:

- one per chunk still running, at most `threads`;
- `L` waiting subtrees for each running chunk, and `L` more for the next chunk not yet started.

The worst case is therefore `threads + (threads + 1) × L` live copies. A copy is never bigger
than the array itself: `elements × bytes per element`, for example 24 bytes per `Fixed[f64, 3]`
force vector. A copy is that big only when its chunks touched every block. The worst case also
needs one chunk to stall while every chunk after it finishes. For spatially sorted data, a chunk
touches a few blocks, so the extra memory stays near one copy of the array in total.

Each live copy also has a block table: one 8-byte pointer per block of the array, or
`ceil(array bytes / 4096) × 8` bytes, which is 1/512 of the array. A chunk makes its table the
first time it adds, and the table is freed with the copy, so the tables follow the same bound.
One table per chunk made up front would cost twice the array for 1024 chunks, which is why the
tables are lazy.

For example, take a million atoms with `Fixed[f64, 3]` forces, a 24 MB array, on 8 threads.
The worst case is `8 + 9 × 10 = 98` copies, about 2.4 GB, plus 98 tables of 47 KB each. A
spatially sorted run uses about 24 MB more than the serial loop. Combining after the loop
instead would keep all 1024 chunks' copies alive, a worst case of 24 GB.

The reference currently says the worst case is "one copy per chunk running at once". That
leaves out the waiting subtrees. The implementation pull request replaces that sentence with
the bound above.

### In the LLVM backend

- **The parent.** Before `baryon_rt_parallel_for`, it makes one accumulator per accumulation and
  puts its pointer in the context. After the loop, it calls `finish` with the array's header.
- **The chunk function.** It builds, on its stack, one *view* per accumulation:
  `{ header, ptr accumulator, i64 chunk }`. It passes a pointer to the view where the body
  expects the array. Because the header comes first, `Len` and bounds checks in the body read it
  unchanged. When the chunk ends, it calls `chunk_done`.
- **`Accumulate`.** It computes the element's row-major scalar offset from the indices and the
  lengths, and calls `slot`. Then it adds or subtracts in place with the same arithmetic as `+=`
  outside a loop. A `Fixed` element is added lane by lane.

The call to `slot` per add is the narrow first version. Caching the last block in the body is
the obvious next step, and it waits for a benchmark that shows the call matters (roadmap 1.14.1
adds the half-list Lennard-Jones baseline).

### Not in the first version

- Slices as targets: `forces[a:b] += v`.
- Complex elements, which the backend does not represent yet.
- Combining blocks in parallel inside `finish`.

Each is reported as "not lowered yet" or "not supported by the LLVM backend yet", never
miscompiled.

## Consequences

- **What users get.** Half-list Lennard-Jones, histograms and charge deposition build and give
  the same bits on any machine, using the same tree as scalar reductions.
- **Memory.** It stays near one extra copy for local access patterns. The worst case is
  `threads + (threads + 1) × ceil(log2(chunks))` copies with their block tables, never one copy
  per chunk.
- **Speed.** One runtime call per add, until a benchmark justifies caching.
- **Interface changes.** `mir` changes in its own pull request first, as an interface crate must:
  the new `ParallelFor` field, the `Accumulate` statement, the `AccumulateOperator` enum and the
  validation. The runtime functions follow, then lowering and the backend together.
- **Other backends.** A future backend (the Cranelift backend, 2.1, or a GPU one) reads the same
  `Accumulate` statement and may pick another strategy. Unordered atomics still need the GPU
  record that decision 0013 asks for.
