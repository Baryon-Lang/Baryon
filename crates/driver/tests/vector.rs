//! Vector loops, end to end, through every stage up to the type checker: a `simd(range(...))`
//! body follows the rules of a `prange` body (decision 0011, checked by the compiler's own
//! rules until roadmap 3.7 adds LLVM's verdict), may sit inside a `prange` loop, and holds no
//! `prange` and no other `simd`.
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

/// `axpy`, a dot product reduction, and a `simd` row loop inside a `prange` loop over the rows,
/// each writing only at its own loop variable.
#[test]
fn vector_loops_that_follow_the_rules_check_clean() {
    let program = r"
def axpy(a: f64, x: Array[f64, 1], y: InOut[Array[f64, 1]], n: i64):
    for i in simd(range(n)):
        y[i] = a * x[i] + y[i]


def dot(x: Array[f64, 1], y: Array[f64, 1], n: i64) -> f64:
    total = 0.0
    for i in simd(range(0, n, 1)):
        total += x[i] * y[i]
    return total


def scale(grid: InOut[Array[f64, 2]], n: i64):
    for i in prange(n):
        factor = 2.0
        for j in simd(range(n)):
            grid[i, j] = grid[i, j] * factor
";
    assert_eq!(check_one(program), "");
}

/// A dependence between iterations, a shared scalar written without a reduction, a loop that
/// ends early, a `simd` without `range`, and loops nested the wrong way round.
#[test]
fn vector_loops_that_break_the_rules_are_reported() {
    let program = r"
def run(a: InOut[Array[f64, 1]], n: i64) -> f64:
    last = 0.0
    for i in simd(range(1, n)):
        a[i] = a[i - 1] + 1.0
        last = a[i]
        if a[i] > 2.0:
            break
    for i in simd(n):
        pass
    for i in simd(range(n)):
        for j in prange(n):
            pass
        for j in simd(range(n)):
            pass
    return last
";
    assert_eq!(
        check_one(program),
        r"error: a vector loop cannot end early
 --> main.byn:7:13
  |
7 |             break
  |             ^^^^^ leaves the loop
 ::: main.byn:3:14
  |
3 |     for i in simd(range(1, n)):
  |              ----------------- iterations run several at a time
  = note: every iteration runs; skip the rest of one with `continue`, or use `range` for a loop that stops early

error: an iteration of this vector loop may read an element of `a` that another iteration writes
 --> main.byn:4:16
  |
4 |         a[i] = a[i - 1] + 1.0
  |                ^^^^^^^^ indexed by `i - 1`, not by the loop variable `i`
 ::: main.byn:3:14
  |
3 |     for i in simd(range(1, n)):
  |              ----------------- iterations run several at a time
  = note: an array the loop writes may be read only at the loop variable's index; read a copy made before the loop instead

error: iterations of this vector loop all write `last`
 --> main.byn:5:9
  |
5 |         last = a[i]
  |         ^^^^ not a reduction
 ::: main.byn:3:14
  |
3 |     for i in simd(range(1, n)):
  |              ----------------- iterations run several at a time
  = note: a shared variable is written in a vector loop only as a reduction: `x += e`, `x -= e`, `x *= e`, `x = min(x, e)` or `x = max(x, e)`
  = note: to keep a value for one iteration, assign a new variable inside the loop

error: `simd` takes one `range(...)`
 --> main.byn:8:14
  |
8 |     for i in simd(n):
  |              ^^^^^^^ not `simd(range(...))`
  = note: write the loop as `for i in simd(range(n)):`

error: a parallel loop cannot run inside a vector loop
  --> main.byn:11:18
   |
11 |         for j in prange(n):
   |                  ^^^^^^^^^ inside a vector loop
  ::: main.byn:10:14
   |
10 |     for i in simd(range(n)):
   |              -------------- the outer vector loop
   = note: swap them: put the `prange` loop outside and the `simd` loop inside

error: a vector loop cannot run inside another
  --> main.byn:13:18
   |
13 |         for j in simd(range(n)):
   |                  ^^^^^^^^^^^^^^ inside a vector loop
  ::: main.byn:10:14
   |
10 |     for i in simd(range(n)):
   |              -------------- the outer vector loop
   = note: use `range` for one of them: only one loop is vectorized"
    );
}
