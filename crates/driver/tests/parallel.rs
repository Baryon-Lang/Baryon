//! Parallel loops, end to end, through every stage up to the type checker: a `prange` body
//! writes shared arrays only at the loop variable's index and shared scalars only as
//! reductions, never ends early, holds no other `prange`, and only adds into the arrays it
//! lists in `accumulate=`.
//!
//! These tests live in the driver because they need several implementations, and only an
//! application crate may depend on more than one (decision 0001).

use driver::Session;

/// Checks `main.byn` and returns every diagnostic, rendered and joined by blank lines. An empty
/// string means the program is clean. The program's first newline is dropped.
fn check_one(text: &str) -> String {
    let mut session = Session::new();
    if let Err(error) = session.add_file("main.byn", text.strip_prefix('\n').unwrap_or(text)) {
        return error.to_string();
    }
    session
        .check()
        .iter()
        .map(|diagnostic| diagnostics::render(diagnostic, &session).to_string())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Writes at the loop variable's index, in one fixed dimension of a grid; fields indexed the
/// same way; `+=`, `-=`, `*=`, `min` and `max` reductions; private variables; `continue`;
/// a `raise` caught inside the body; and an inner `range` loop.
#[test]
fn parallel_loops_that_follow_the_rules_check_clean() {
    let program = r"
class Atoms(Struct):
    positions: Array[f64, 1]
    velocities: Array[f64, 1]


class Bad(Exception):
    pass


def check(x: f64) -> Raises[f64, Bad]:
    if x < 0.0:
        raise Bad()
    return x


def run(atoms: InOut[Atoms], grid: InOut[Array[f64, 2]], n: i64) -> f64:
    total = 0.0
    product = 1.0
    low = 1.0e9
    high = -1.0e9
    for i in prange(n):
        atoms.velocities[i] += 0.5 * atoms.positions[i]
        if atoms.velocities[i] < 0.0:
            continue
        for j in range(n):
            grid[i, j] = grid[i, j] * 2.0 + atoms.positions[j]
        value = atoms.velocities[i]
        try:
            value = check(value)
            if value > 10.0:
                raise Bad()
        except Bad:
            value = 0.0
        total += value
        total -= 1.0
        product *= 2.0
        low = min(low, value)
        high = max(high, value)
    return total + product + low + high
";
    assert_eq!(check_one(program), "");
}

/// The half-list pair loop of the reference: every pair applied to both atoms through
/// `accumulate=`, with an inferred energy reduction.
#[test]
fn an_accumulated_array_may_be_added_to_at_any_index() {
    let program = r"
Vec3 = Fixed[f64, (3,)]


def compute_forces(positions: Array[Vec3, 1], forces: InOut[Array[Vec3, 1]], n: i64) -> f64:
    for i in prange(n):
        forces[i] = Vec3.zeros()
    energy = 0.0
    for i in prange(n, accumulate=forces):
        for j in range(i + 1, n):
            delta = positions[i] - positions[j]
            distance_squared = delta.dot(delta)
            inverse_6 = 1.0 / distance_squared**3
            force = delta * (24.0 * inverse_6 * (2.0 * inverse_6 - 1.0) / distance_squared)
            forces[i] += force
            forces[j] -= force
            energy += 4.0 * inverse_6 * (inverse_6 - 1.0)
    return energy
";
    assert_eq!(check_one(program), "");
}

/// Writing another iteration's element, reading one, using a written array whole, a write
/// that is not a reduction, two kinds of reduction, and reading a reduction.
#[test]
fn shared_writes_that_break_the_rules_are_reported() {
    let program = r"
def scale(values: InOut[Array[f64, 1]]):
    pass


def run(a: InOut[Array[f64, 1]], b: InOut[Array[f64, 1]], n: i64) -> f64:
    total = 0.0
    last = 0.0
    for i in prange(1, n):
        a[i] = a[i - 1] + 1.0
        b[i - 1] = 0.0
        scale(b)
        last = a[i]
        total += a[i]
        total *= 2.0
        if total > 1.0:
            continue
    return total + last
";
    assert_eq!(
        check_one(program),
        r"error: an iteration of this parallel loop may read an element of `a` that another iteration writes
 --> main.byn:9:16
  |
9 |         a[i] = a[i - 1] + 1.0
  |                ^^^^^^^^ indexed by `i - 1`, not by the loop variable `i`
 ::: main.byn:8:14
  |
8 |     for i in prange(1, n):
  |              ------------ iterations run in parallel
  = note: an array the loop writes may be read only at the loop variable's index; read a copy made before the loop instead

error: iterations of this parallel loop may write the same element of `b`
  --> main.byn:10:9
   |
10 |         b[i - 1] = 0.0
   |         ^^^^^^^^ indexed by `i - 1`, not by the loop variable `i`
  ::: main.byn:8:14
   |
 8 |     for i in prange(1, n):
   |              ------------ iterations run in parallel
   = note: each element of a shared array may be written only at the loop variable's index
   = note: or compute each element's total in its own iteration, or use `range` for this loop

error: `b` is written element by element in this parallel loop, so the body cannot use it whole
  --> main.byn:11:15
   |
11 |         scale(b)
   |               ^ used whole here
  ::: main.byn:8:14
   |
 8 |     for i in prange(1, n):
   |              ------------ iterations run in parallel
   = note: use single elements at the loop variable's index, or use `range` for this loop

error: iterations of this parallel loop all write `last`
  --> main.byn:12:9
   |
12 |         last = a[i]
   |         ^^^^ not a reduction
  ::: main.byn:8:14
   |
 8 |     for i in prange(1, n):
   |              ------------ iterations run in parallel
   = note: a shared variable is written in a parallel loop only as a reduction: `x += e`, `x -= e`, `x *= e`, `x = min(x, e)` or `x = max(x, e)`
   = note: to keep a value for one iteration, assign a new variable inside the loop

error: `total` is reduced two ways in this parallel loop
  --> main.byn:14:9
   |
14 |         total *= 2.0
   |         ^^^^^ a different reduction
  ::: main.byn:13:9
   |
13 |         total += a[i]
   |         ----- first reduced here
   = note: a shared variable reduces one way: `+=` and `-=` together, or `*=`, `min` or `max` alone

error: `total` is a reduction in this parallel loop, so the body cannot read it
  --> main.byn:15:12
   |
15 |         if total > 1.0:
   |            ^^^^^ read here
  ::: main.byn:13:9
   |
13 |         total += a[i]
   |         ----- reduced here
   = note: its total is known only after the loop"
    );
}

/// `return`, `break`, `raise` and an unhandled failing call in a parallel loop, even in a
/// function that declares the error, and a `prange` inside one.
#[test]
fn leaving_or_nesting_a_parallel_loop_is_reported() {
    let program = r"
class Bad(Exception):
    pass


def check(x: f64) -> Raises[f64, Bad]:
    return x


def run(a: Array[f64, 1], n: i64) -> Raises[f64, Bad]:
    for i in prange(n):
        if a[i] > 1.0:
            return a[i]
        if a[i] > 2.0:
            break
        if a[i] > 3.0:
            raise Bad()
        value = check(a[i])
        for j in prange(n):
            pass
    return 0.0
";
    assert_eq!(
        check_one(program),
        r"error: this `raise` fails with `Bad`, which is not handled inside this parallel loop
  --> main.byn:16:19
   |
16 |             raise Bad()
   |                   ^^^^^ can fail with `Bad`
   = note: catch it with `try:` and `except Bad:` inside the loop body: an error cannot leave a parallel loop

error: `check` can fail with `Bad`, which is not handled inside this parallel loop
  --> main.byn:17:17
   |
17 |         value = check(a[i])
   |                 ^^^^^^^^^^^ can fail with `Bad`
   = note: catch it with `try:` and `except Bad:` inside the loop body: an error cannot leave a parallel loop

error: a parallel loop cannot run inside another
  --> main.byn:18:18
   |
18 |         for j in prange(n):
   |                  ^^^^^^^^^ inside a parallel loop
  ::: main.byn:10:14
   |
10 |     for i in prange(n):
   |              --------- the outer parallel loop
   = note: use `range` for the inner loop: nested parallel loops would oversubscribe the cores

error: a parallel loop cannot end early
  --> main.byn:12:13
   |
12 |             return a[i]
   |             ^^^^^^^^^^^ returns from inside the loop
  ::: main.byn:10:14
   |
10 |     for i in prange(n):
   |              --------- iterations run in parallel
   = note: every iteration runs; skip the rest of one with `continue`, or use `range` for a loop that stops early

error: a parallel loop cannot end early
  --> main.byn:14:13
   |
14 |             break
   |             ^^^^^ leaves the loop
  ::: main.byn:10:14
   |
10 |     for i in prange(n):
   |              --------- iterations run in parallel
   = note: every iteration runs; skip the rest of one with `continue`, or use `range` for a loop that stops early"
    );
}

/// An accumulated array that is read, assigned and passed to a function; one that is never
/// added to; an entry that is not a name; one that is not an array of numbers; and an unknown
/// keyword.
#[test]
fn accumulate_mistakes_are_reported() {
    let program = r"
def total(values: Array[f64, 1]) -> f64:
    return values.sum()


def run(forces: InOut[Array[f64, 1]], spare: InOut[Array[f64, 1]], names: list[str], n: i64):
    for i in prange(n, accumulate=(forces, spare)):
        forces[i] += forces[0]
        forces[i] = 1.0
        x = total(forces)
    for i in prange(n, accumulate=forces[0]):
        pass
    for i in prange(n, accumulate=names, chunk=4):
        pass
";
    assert_eq!(
        check_one(program),
        r"error: `forces` is listed in `accumulate=`, so this loop may only add to it
 --> main.byn:7:22
  |
7 |         forces[i] += forces[0]
  |                      ^^^^^^^^^ read here
 ::: main.byn:6:36
  |
6 |     for i in prange(n, accumulate=(forces, spare)):
  |                                    ------ listed here
  = note: an accumulated array appears only as the target of `+=` or `-=`; a helper function cannot add into it, so return the contribution and add it in the loop

error: `forces` is listed in `accumulate=`, so this loop may only add to it
 --> main.byn:8:9
  |
8 |         forces[i] = 1.0
  |         ^^^^^^^^^ not an addition
 ::: main.byn:6:36
  |
6 |     for i in prange(n, accumulate=(forces, spare)):
  |                                    ------ listed here
  = note: an accumulated array appears only as the target of `+=` or `-=`; a helper function cannot add into it, so return the contribution and add it in the loop

error: `forces` is listed in `accumulate=`, so this loop may only add to it
 --> main.byn:9:19
  |
9 |         x = total(forces)
  |                   ^^^^^^ read here
 ::: main.byn:6:36
  |
6 |     for i in prange(n, accumulate=(forces, spare)):
  |                                    ------ listed here
  = note: an accumulated array appears only as the target of `+=` or `-=`; a helper function cannot add into it, so return the contribution and add it in the loop

error: `spare` is listed in `accumulate=`, but this loop never adds to it
 --> main.byn:6:44
  |
6 |     for i in prange(n, accumulate=(forces, spare)):
  |                                            ^^^^^ never added to
  = note: remove it from `accumulate=`

error: `accumulate=` lists arrays by name
  --> main.byn:10:35
   |
10 |     for i in prange(n, accumulate=forces[0]):
   |                                   ^^^^^^^^^ not a variable or field
   = note: name the arrays the loop adds into: `accumulate=forces` or `accumulate=(forces, virial)`

error: `accumulate=` needs an array of numbers, found `list[str]`
  --> main.byn:12:35
   |
12 |     for i in prange(n, accumulate=names, chunk=4):
   |                                   ^^^^^ cannot be added into
   = note: an accumulated array holds integers, floats, complex numbers, or `Fixed` arrays of them

error: `prange` has no parameter `chunk`
  --> main.byn:12:42
   |
12 |     for i in prange(n, accumulate=names, chunk=4):
   |                                          ^^^^^ unknown parameter
   = note: `prange` takes the arguments of `range`, and `accumulate=`"
    );
}

/// The pair loop that adds into `forces[j]` without `accumulate=`: the error suggests it.
#[test]
fn adding_into_another_iterations_element_suggests_accumulate() {
    let program = r"
def pairs(forces: InOut[Array[f64, 1]], n: i64):
    for i in prange(n):
        for j in range(i + 1, n):
            forces[i] += 1.0
            forces[j] -= 1.0
";
    assert_eq!(
        check_one(program),
        r"error: iterations of this parallel loop may write the same element of `forces`
 --> main.byn:5:13
  |
5 |             forces[j] -= 1.0
  |             ^^^^^^^^^ indexed by `j`, not by the loop variable `i`
 ::: main.byn:2:14
  |
2 |     for i in prange(n):
  |              --------- iterations run in parallel
  = note: each element of a shared array may be written only at the loop variable's index
  = note: if every write to `forces` in this loop adds to it, declare it add-only: `prange(n, accumulate=forces)`
  = note: or compute each element's total in its own iteration, or use `range` for this loop"
    );
}

/// `prange` cannot drive a comprehension, at any depth (Collins, 2026-11-10): a parallel loop
/// is a `for` statement, and the error shows that form. `simd(range(…))` and `range` can.
#[test]
fn prange_in_a_comprehension_is_reported() {
    let program = "
def table(n: i64) -> list[i64]:
    squares = [i * i for i in prange(n)]
    pairs = [i + j for i in range(n) for j in prange(i)]
    fine = [i for i in simd(range(n))]
    return fine
";
    assert_eq!(
        check_one(program),
        "error: `prange` cannot be used in a comprehension
 --> main.byn:2:31
  |
2 |     squares = [i * i for i in prange(n)]
  |                               ^^^^^^^^^ a parallel loop must be a `for` statement
  = note: make the result first, then fill it in parallel: `for i in prange(n): out[i] = f(i)`

error: `prange` cannot be used in a comprehension
 --> main.byn:3:47
  |
3 |     pairs = [i + j for i in range(n) for j in prange(i)]
  |                                               ^^^^^^^^^ a parallel loop must be a `for` statement
  = note: make the result first, then fill it in parallel: `for i in prange(n): out[i] = f(i)`"
    );
}

/// `sys.exit` written in a `prange` body would end the program while other iterations write
/// their files, so it is reported as `return` is (roadmap 1.13.17). The note points to a
/// reduction, the one way a parallel loop can tell what follows it that something failed.
#[test]
fn exit_inside_a_parallel_loop_is_reported() {
    let program = "
import sys


def main():
    failed = 0
    for i in prange(8):
        if i == 3:
            sys.exit(1)
        failed += 1
    if failed > 0:
        sys.exit(1)
";
    assert_eq!(
        check_one(program),
        "\
error: a parallel loop cannot end early
 --> main.byn:8:13
  |
8 |             sys.exit(1)
  |             ^^^^^^^^^^^ ends the program from inside the loop
 ::: main.byn:6:14
  |
6 |     for i in prange(8):
  |              --------- iterations run in parallel
  = note: every iteration runs; count failures with `failed += 1` and call `sys.exit` after the loop"
    );
}
