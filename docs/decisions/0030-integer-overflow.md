# 0030. Integer overflow panics in debug builds and wraps in release builds

Status: Accepted
Date: 2026-11-09
Roadmap: D.11, 1.11.7

## Context

An integer type holds a fixed range: `i64` reaches 9 223 372 036 854 775 807, `i32` about 2.1
billion. Arithmetic whose exact result falls outside the range *overflows*. In numeric code that
is almost always a bug, such as a particle index multiplied past `i32`, and a silently wrapped
number is a wrong answer with no error, the failure the reference already refuses for
conversions ([Types](../reference/types.md#conversions)).

The design plan says overflow panics in debug builds and left release builds open as roadmap
decision D.11: wrap, as C and Fortran compilers do in practice and Rust's release builds do, or
panic there too, as Swift does. Until now there was one build mode, and it wrapped. The mid-level
IR was built ready for either answer: integer arithmetic that can overflow can be a
`CheckedBinary`, which yields the result and an overflow flag, followed by an `Assert` on the flag
([0018](0018-mid-level-ir.md)).

A check costs little in straight-line code, but it stops LLVM from vectorizing a loop that adds
integers, because each lane would need its own panic. Bounds checks stay on in every build
([0010](0010-bounds-checks.md)) because an unchecked index corrupts memory; a wrapped integer does
not.

## Decision

Collins chose this on 2026-11-09.

- **Two build modes.** `baryon build`, `run` and `test` make a *debug build* by default and a
  *release build* with `--release`. Both are optimized (`clang -O2`) and keep line tables, so a
  panic prints a backtrace in either; only the overflow check differs. The driver's `Profile`
  names the mode.
- **Debug builds panic.** Integer `+`, `-` and `*`, the in-place `+=`, `-=` and `*=`, and `-x` on
  a signed integer lower to a `CheckedBinary` and an `Assert` that the flag is `False`. A failed
  check panics with `integer overflow in` and the operator, at the expression's line and column,
  and exits with status 101, as every panic does.
- **Release builds wrap.** The LLVM backend leaves out every `Assert` of `NoOverflow`
  (`Module::wrap_on_overflow`), so the result wraps around in two's complement and the flag goes
  unused; LLVM folds the intrinsic into a plain `add`, `sub` or `mul`. A `prange` reduction of
  integers combines its partial results with wrapping arithmetic too, so a release build gives
  the same answer on any number of threads.
- **The IR stays the same for both.** Lowering always writes the check; the backend decides. A
  body is lowered and cached once, whatever the build mode, and a pass that proves a check
  cannot fail, as bounds-check hoisting will (roadmap 3.2), serves both modes.

## Consequences

- A program that overflows stops at the line that did it while it is being written and tested,
  and runs at full speed once released, as Rust programs do. A result that differs between the
  two builds is an overflow the debug build would have reported.
- A debug build of a loop that adds integers is not vectorized. Benchmarks and timing runs use
  `--release`.
- Not checked yet, so they wrap in both builds: arithmetic in a generic function on a type
  parameter instantiated with an integer, since lowering sees only the parameter; `abs` of the
  type's minimum; whole-array arithmetic on integer arrays, `a + b`; integer `a.sum()` and
  `v.dot(w)`; and indices the compiler computes itself, such as a `range` loop's counter. Roadmap
  idea I.29 checks them. Combining the private copies of an `accumulate=` array checks for
  overflow in both builds, in the runtime.
- The panic names the operator but not the operands. Naming them is roadmap idea I.18.
- Other checks, such as bounds, division by zero and lossy conversions, stay on in both builds.
  A future flag that turns more checks off would be a new decision.
