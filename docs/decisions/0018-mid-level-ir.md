# 0018. The mid-level IR is basic blocks over typed locals and places

Status: Accepted
Date: 2026-11-07
Roadmap: 1.9

## Context

Between the type checker and the backends sits the mid-level IR, or MIR: the form every function
body takes once its types are known. Every later stage reads it. The reference and exclusivity
checker (1.8) and its successors state their rules over it, the language-specific optimizations
in the design plan (array fusion, drop elision, bounds-check hoisting, inlining) rewrite it, and
both backends, LLVM for release and Cranelift for debug, translate it without ever seeing the
syntax tree. Its shape is hard to change once three stages depend on it, so it gets a record
(`CONTRIBUTING.md` rule 7).

The design plan says the MIR is "in static single assignment form with explicit control flow,
drops and bounds checks", and that monomorphization happens in it. Five questions are open:

1. **SSA, or locals and places?** In SSA every value is assigned once and merges go through phi
   nodes. In the locals-and-places form of Rust's MIR, a function has numbered local variables
   that may be assigned many times, and memory is named by a *place*: a local followed by field
   and index steps, such as `particles[i].velocity`.
2. **How does `Raises[T, E]` lower?** Decision 0005 says an error comes back as an ordinary
   return value and `try`/`except` is a branch on it.
3. **Where does monomorphization happen**, and what does a backend see of generics?
4. **Do whole-array expressions and `prange` loops stay recognizable**, so that array fusion
   (3.1) and the parallel runtime (1.12) can see them, or are they loops by the time anything
   reads the IR?
5. **How are drops and bounds checks represented**, so optimizations can remove them and
   backends can trust them?

## Decision

### Locals and places, not SSA

A function body is a list of *locals* and a list of *basic blocks*. Each local has a type and a
kind: the return value, a parameter, a named variable or a compiler temporary. Each basic block is
a list of statements followed by exactly one terminator, the only place control flow can branch.
Memory is read and written through places: a local and a list of projections, each a field, an
array element, or the value inside an optional.

The reasons:

- **The reference rules are stated over places.** Decision 0006 and the reference's exclusivity
  rule say two arguments may not overlap when one of them is written. "Overlap" is a question
  about access paths: `a.x` and `a.y` are disjoint, `a[i]` and `a` are not. A place is that access
  path, written down. In SSA the path is lost in a chain of address computations and must be
  rebuilt.
- **The metadata LLVM needs comes from places.** `noalias` on an `InOut` parameter and
  `dereferenceable` on a borrow follow directly from the parameter's mode and the place passed.
- **Drops need storage.** A drop names the place whose value ends. SSA values have no storage.
- **The backends build SSA for us.** LLVM's `mem2reg` pass and Cranelift's SSA builder turn
  assignments to locals into SSA, and do it well. Writing our own SSA construction would cost
  weeks and gain nothing an optimization in the first two phases needs. A pass that wants SSA
  can build it locally, as Rust's optimizations do.

This record supersedes the design plan's "static single assignment form".

### References stay second class in the IR

The type system has no reference types: a parameter is a read-only borrow, an `InOut` borrow or
an `Owned` value (decision 0006), and references cannot be stored. The IR keeps it that way. A
call argument is a place and a mode, not a pointer value, and a borrowed parameter is a local
whose storage belongs to the caller. A backend passes a borrow as a pointer; nothing else in the
IR knows that pointers exist. This keeps the 1.8 checker a check over places, and it means the
IR cannot express a dangling reference.

### `Raises` is an error edge on the call

A call to a fallible function is a `Call` terminator with two successors: the block to continue
in when it succeeds, with the value written to the call's destination, and an *error edge*: the
block to continue in when it fails, with the error written to a separate local. A body raises
with a `Raise` terminator, which returns one error value of one error type.

- Automatic propagation, `try` and `except` are all ordinary blocks after the error edge: a
  `Switch` on which error arrived, then a `Raise` of it, or the handler's code. No unwinding and
  no landing pads exist, as decision 0005 promises, and a call that cannot fail has no error edge
  and costs nothing extra.
- A function that raises several error types receives, at an error edge, one value holding
  exactly one of them. `ErrorIndex` reads which one, and the `Error` projection reads it as that
  type. A raise always carries a single error type, so a caller's and a callee's error lists
  never need converting into each other: propagating three error types is three small `Raise`
  blocks after one `Switch`. The backend chooses the binary encoding.

### Explicit drops and explicit checks

- **Drops are statements.** Lowering writes a `Drop` of every local whose type owns memory at
  every exit from its scope. A drop cannot fail: panics are not caught (decision 0005), so the
  IR needs no unwinding edges, and a statement is enough.
- **Drop elaboration is a pass.** As built, a `Drop` means "drop the value if it is still
  there", because a value moved out on one branch is gone on the other. A drop-elaboration pass
  removes the drops of values that are certainly moved and guards the rest with flags held in
  ordinary `bool` locals. After it, every `Drop` is unconditional. A body records which of these
  *phases* it is in, so a backend refuses a body it cannot trust.
- **Bounds checks are `Assert` terminators.** An `Index` projection itself is unchecked. Lowering
  puts an `Assert` that the index is below the length before every index, as decision 0010
  requires, and leaves it out only for `a.unchecked[i]`. Bounds-check hoisting (3.2) is then a
  pass that removes or moves `Assert`s, and a backend never has to guess what is checked.
- **Arithmetic that can overflow** is a `CheckedBinary` rvalue, which yields the result and an
  overflow flag, followed by an `Assert` on the flag. Whether release builds check is roadmap
  decision D.11; the IR expresses both answers, so this record does not settle it.

### Monomorphization is a query over built bodies

- Lowering builds one body per function, generic or not. A generic body's types may contain type
  parameters, and a call to a generic function names the callee and its type arguments.
- An *instance* is a function and the type arguments it is used at. An instance query
  substitutes the arguments into the built body and runs drop elaboration and the expansion
  passes below on the result. Arithmetic on a type parameter, `x + y` for `T: Number`, becomes
  primitive arithmetic once `T` is known; traits are standard and statically dispatched in the
  MVP, so no vtables or trait-method calls survive substitution.
- Collection starts from the entry points (`main` and each `@test`) and follows calls. Backends
  receive only instances, and an instance body never contains a type parameter.
- The design plan's deduplication of identical instances by the hash of their IR is an
  optimization for Phase 2, measured first.

Building generic bodies once keeps lowering cached per function, which the one-function edit
target depends on: an edit re-lowers one body and re-instantiates only the instances of it.

### Whole-array expressions stay whole; parallel loops are outlined

- **A whole-array expression is one rvalue.** `a + b * c` over arrays lowers to a single
  `Elementwise` rvalue: a small expression tree whose leaves are arrays and scalars. Array fusion
  (3.1) merges such trees across statements, where they are easy to see. An expansion pass turns
  each tree into a loop nest, with bounds checks proved away by the shapes, before a backend sees
  it. Lowering to loops first would leave fusion to rediscover them from blocks, which is the hard
  problem Fortran compilers spend years on.
- **A `prange` body is its own body.** Lowering moves the loop body into a separate MIR body, with
  the places it uses passed in as arguments in their modes: read-only, written at disjoint
  elements, or accumulated (decision 0013). The loop becomes a `ParallelFor` terminator naming the
  outlined body, the trip count and those arguments. The runtime (1.12) receives the body as a
  plain function, the 1.8 checker sees every shared array and how it is used in one place, and
  `noalias` follows from the argument modes as it does for calls.
- **A `simd` loop stays an ordinary loop** whose header block is marked. The check that
  decision 0011 promises runs on the marked loop, and the backend passes the mark on to its
  vectorizer.

`ParallelFor` and the `simd` mark join the IR with the roadmap item that lowers them; they are
recorded here so that item does not reopen the shape. `ParallelFor` joined with roadmap 1.9.7:
the outlined bodies live in their function's body, and each runs one iteration. A shared scalar
the loop reduces into, such as `energy += e`, is listed with its operator, and the backend gives
each chunk a private copy and combines the copies in the fixed tree the reference defines
([reproducible reductions](../reference/parallel-loops.md#reproducible-reductions), decision
[0021](0021-runtime-crate-and-unsafe-code.md)).

### One interface crate, one query per stage

The IR is the interface crate `mir`: data types, a validator for its structural invariants, and a
text form for tests and debugging. It holds no lowering and no passes. Lowering, drop
elaboration, monomorphization and each optimization are implementation crates that read and
write `mir` types, and the application wires them to queries (decision 0016): the built body of
each function, and the instance body of each instance.

## Consequences

- The 1.8 checker, `noalias` metadata and drop elaboration all work on the same access paths the
  reference rules are written in. No stage rebuilds what lowering knew.
- Both backends get a form they translate one block at a time: assignments to locals become
  stack slots that `mem2reg` or Cranelift promotes to registers.
- Optimizations that want SSA, such as global value numbering, must build it themselves or leave
  the work to LLVM. For the MVP that is LLVM's job anyway.
- Every index carries an explicit `Assert`, so IR for array-heavy loops is larger than the source
  until hoisting (3.2) runs. That is the cost of making each check visible and removable.
- An error edge per fallible call makes the control-flow graph larger than Rust's `?` would, by
  one small block per propagated error type. It keeps the error representation a backend choice.
- `Elementwise` is a high-level form that must be expanded before a backend sees it. A backend
  that meets one has received a body in the wrong phase, and the phase check catches that.
- `ParallelFor` reaches the backend, since only a backend can turn the outlined body into the
  plain function the runtime calls. (Clarified 2026-11-08 with roadmap 1.9.7: this bullet first
  grouped it with `Elementwise`, which contradicted the outlining above.)
- Views (slices passed as arguments) are not settled here. They will be a projection, never a
  value, because views cannot be bound to names; the shape comes with the item that lowers
  slicing.
