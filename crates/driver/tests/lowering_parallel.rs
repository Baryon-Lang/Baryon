//! End-to-end tests of lowering `prange` loops (roadmap 1.9.7): the body outlined into a body of
//! its own, run by `ParallelFor` with the shared variables in their modes and the reductions.
//! Each program is parsed, resolved and checked, then each named function is lowered and
//! compared by its full IR text.

use driver::Session;

/// The IR text of each function in `names`, from `program` after its first newline, joined by
/// blank lines. When the program has errors, or lowering gives some, it returns them instead,
/// rendered, so a failing test shows what went wrong.
fn lower(program: &str, names: &[&str]) -> String {
    let mut session = Session::new();
    let program = program.strip_prefix('\n').unwrap_or(program);
    if let Err(error) = session.add_file("main.byn", program) {
        return error.to_string();
    }
    let mut errors = session.check();
    if errors.is_empty() {
        let mut bodies = Vec::new();
        for name in names {
            match session.mir(name) {
                None => return format!("no function `{name}`"),
                Some(Ok(text)) => bodies.push(text),
                Some(Err(found)) => {
                    errors = found;
                    break;
                }
            }
        }
        if errors.is_empty() {
            return bodies.join("\n");
        }
    }
    errors
        .iter()
        .map(|error| diagnostics::render(error, &session).to_string())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The body of a `prange` loop becomes `body0`, which runs one iteration: it reads `xs` and
/// `factor` through borrows, writes `ys[i]` through an `InOut` borrow, and adds into a private
/// `total` that the loop reduces with `Add`. `y` is private to each iteration. The loop counts
/// `len(xs) - 0` iterations, and `i` is the iteration number itself.
#[test]
fn prange_bodies_are_outlined() {
    let program = "
def scale(xs: Array[f64, 1], ys: InOut[Array[f64, 1]], factor: f64) -> f64:
    total = 0.0
    for i in prange(len(xs)):
        y = xs[i] * factor
        ys[i] = y
        total += y
    return total
";
    assert_eq!(
        lower(program, &["scale"]),
        "\
fn(_1: Array[f64, 1], _2: InOut[Array[f64, 1]], _3: f64) -> f64 {
    let _0: f64; // return
    let _1: Array[f64, 1]; // parameter xs
    let _2: Array[f64, 1]; // parameter ys
    let _3: f64; // parameter factor
    let _4: f64; // total
    let _5: i64; // i
    let _6: f64; // y
    let _7: i64;
    let _8: i64;
    let _9: i64;
    let _10: i64;

    bb0: {
        _4 = 0.0_f64;
        _7 = 0_i64;
        _9 = len(_1, 0);
        _8 = copy _9;
        _10 = Subtract(copy _8, copy _7);
        parallel_for body0(copy _10; _1, inout _2, _3, reduce Add _4) -> bb1;
    }

    bb1: {
        _0 = copy _4;
        return;
    }
}

body0: fn(_1: Owned[i64], _2: Array[f64, 1], _3: InOut[Array[f64, 1]], _4: f64, _5: InOut[f64]) -> None {
    let _0: None; // return
    let _1: i64; // parameter iteration
    let _2: Array[f64, 1]; // parameter xs
    let _3: Array[f64, 1]; // parameter ys
    let _4: f64; // parameter factor
    let _5: f64; // parameter total
    let _6: i64; // i
    let _7: f64; // y
    let _8: i64;
    let _9: i64;
    let _10: bool;
    let _11: bool;
    let _12: bool;
    let _13: i64;
    let _14: i64;
    let _15: bool;
    let _16: bool;
    let _17: bool;

    bb0: {
        _6 = copy _1;
        _8 = len(_2, 0);
        _9 = copy _6;
        _10 = GreaterEqual(copy _9, 0_i64);
        _11 = Less(copy _9, copy _8);
        _12 = BitAnd(copy _10, copy _11);
        assert(copy _12, index in bounds in dimension 0: copy _9 < copy _8) -> bb2;
    }

    bb1: {
        _0 = None;
        return;
    }

    bb2: {
        _7 = Multiply(copy _2[_9], copy _4);
        _13 = len(_3, 0);
        _14 = copy _6;
        _15 = GreaterEqual(copy _14, 0_i64);
        _16 = Less(copy _14, copy _13);
        _17 = BitAnd(copy _15, copy _16);
        assert(copy _17, index in bounds in dimension 0: copy _14 < copy _13) -> bb3;
    }

    bb3: {
        _3[_14] = copy _7;
        _5 = Add(copy _5, copy _7);
        goto -> bb1;
    }
}
"
    );
}

/// `prange(1, n, 2)` runs `(n - 1 + 1) // 2` iterations, passes the start in, and computes
/// `i = start + iteration * 2`. `continue` ends the iteration, and `*=` reduces with `Multiply`.
#[test]
fn prange_with_a_start_and_a_step() {
    let program = "
def odd(n: i64) -> i64:
    product = 1
    for i in prange(1, n, 2):
        if i == 5:
            continue
        product *= i
    return product
";
    assert_eq!(
        lower(program, &["odd"]),
        "\
fn(_1: i64) -> i64 {
    let _0: i64; // return
    let _1: i64; // parameter n
    let _2: i64; // product
    let _3: i64; // i
    let _4: i64;
    let _5: i64;
    let _6: i64;
    let _7: i64;
    let _8: i64;

    bb0: {
        _2 = 1_i64;
        _4 = 1_i64;
        _5 = copy _1;
        _6 = Subtract(copy _5, copy _4);
        _7 = Add(copy _6, 1_i64);
        _8 = FloorDivide(copy _7, 2_i64);
        parallel_for body0(copy _8; _4, reduce Multiply _2) -> bb1;
    }

    bb1: {
        _0 = copy _2;
        return;
    }
}

body0: fn(_1: Owned[i64], _2: i64, _3: InOut[i64]) -> None {
    let _0: None; // return
    let _1: i64; // parameter iteration
    let _2: i64; // parameter start
    let _3: i64; // parameter product
    let _4: i64; // i
    let _5: i64;
    let _6: bool;

    bb0: {
        _5 = Multiply(copy _1, 2_i64);
        _4 = Add(copy _2, copy _5);
        _6 = Equal(copy _4, 5_i64);
        branch copy _6 -> [true: bb2, false: bb3];
    }

    bb1: {
        _0 = None;
        return;
    }

    bb2: {
        goto -> bb1;
    }

    bb3: {
        _3 = Multiply(copy _3, copy _4);
        goto -> bb1;
    }
}
"
    );
}

/// An array listed in `accumulate=` is passed last, `InOut`, and listed as accumulated; in the
/// body, `bins[values[i]] += 1` checks the index, then adds with `Accumulate`.
#[test]
fn accumulate_adds_with_accumulate_statements() {
    let program = "
def histogram(bins: InOut[Array[i64, 1]], values: Array[i64, 1]):
    for i in prange(len(values), accumulate=bins):
        bins[values[i]] += 1
        bins[0] -= 1
";
    assert_eq!(
        lower(program, &["histogram"]),
        "\
fn(_1: InOut[Array[i64, 1]], _2: Array[i64, 1]) -> None {
    let _0: None; // return
    let _1: Array[i64, 1]; // parameter bins
    let _2: Array[i64, 1]; // parameter values
    let _3: i64; // i
    let _4: i64;
    let _5: i64;
    let _6: i64;
    let _7: i64;

    bb0: {
        _4 = 0_i64;
        _6 = len(_2, 0);
        _5 = copy _6;
        _7 = Subtract(copy _5, copy _4);
        parallel_for body0(copy _7; _2, accumulate _1) -> bb1;
    }

    bb1: {
        _0 = None;
        return;
    }
}

body0: fn(_1: Owned[i64], _2: Array[i64, 1], _3: InOut[Array[i64, 1]]) -> None {
    let _0: None; // return
    let _1: i64; // parameter iteration
    let _2: Array[i64, 1]; // parameter values
    let _3: Array[i64, 1]; // parameter bins
    let _4: i64; // i
    let _5: i64;
    let _6: i64;
    let _7: i64;
    let _8: i64;
    let _9: bool;
    let _10: bool;
    let _11: bool;
    let _12: bool;
    let _13: bool;
    let _14: bool;
    let _15: i64;
    let _16: i64;
    let _17: bool;
    let _18: bool;
    let _19: bool;

    bb0: {
        _4 = copy _1;
        _5 = len(_3, 0);
        _7 = len(_2, 0);
        _8 = copy _4;
        _9 = GreaterEqual(copy _8, 0_i64);
        _10 = Less(copy _8, copy _7);
        _11 = BitAnd(copy _9, copy _10);
        assert(copy _11, index in bounds in dimension 0: copy _8 < copy _7) -> bb2;
    }

    bb1: {
        _0 = None;
        return;
    }

    bb2: {
        _6 = copy _2[_8];
        _12 = GreaterEqual(copy _6, 0_i64);
        _13 = Less(copy _6, copy _5);
        _14 = BitAnd(copy _12, copy _13);
        assert(copy _14, index in bounds in dimension 0: copy _6 < copy _5) -> bb3;
    }

    bb3: {
        accumulate _3[_6] += 1_i64;
        _15 = len(_3, 0);
        _16 = 0_i64;
        _17 = GreaterEqual(copy _16, 0_i64);
        _18 = Less(copy _16, copy _15);
        _19 = BitAnd(copy _17, copy _18);
        assert(copy _19, index in bounds in dimension 0: copy _16 < copy _15) -> bb4;
    }

    bb4: {
        accumulate _3[_16] -= 1_i64;
        goto -> bb1;
    }
}
"
    );
}

/// `accumulate=atoms.forces`: the body reaches `atoms` through its own borrowed parameter, for
/// the bounds check's length, and the add goes to the accumulated parameter `_3`.
#[test]
fn accumulate_of_a_field_adds_through_its_own_parameter() {
    let program = "
class Atoms(Struct):
    forces: Array[f64, 1]

def push(atoms: InOut[Atoms], n: i64):
    for i in prange(n, accumulate=atoms.forces):
        atoms.forces[0] += 1.0
";
    assert_eq!(
        lower(program, &["push"]),
        "\
fn(_1: InOut[Atoms], _2: i64) -> None {
    let _0: None; // return
    let _1: Atoms; // parameter atoms
    let _2: i64; // parameter n
    let _3: i64; // i
    let _4: i64;
    let _5: i64;
    let _6: i64;

    bb0: {
        _4 = 0_i64;
        _5 = copy _2;
        _6 = Subtract(copy _5, copy _4);
        parallel_for body0(copy _6; _1, accumulate _1.0) -> bb1;
    }

    bb1: {
        _0 = None;
        return;
    }
}

body0: fn(_1: Owned[i64], _2: Atoms, _3: InOut[Array[f64, 1]]) -> None {
    let _0: None; // return
    let _1: i64; // parameter iteration
    let _2: Atoms; // parameter atoms
    let _3: Array[f64, 1]; // parameter atoms.forces
    let _4: i64; // i
    let _5: i64;
    let _6: i64;
    let _7: bool;
    let _8: bool;
    let _9: bool;

    bb0: {
        _4 = copy _1;
        _5 = len(_2.0, 0);
        _6 = 0_i64;
        _7 = GreaterEqual(copy _6, 0_i64);
        _8 = Less(copy _6, copy _5);
        _9 = BitAnd(copy _7, copy _8);
        assert(copy _9, index in bounds in dimension 0: copy _6 < copy _5) -> bb2;
    }

    bb1: {
        _0 = None;
        return;
    }

    bb2: {
        accumulate _3[_6] += 1.0_f64;
        goto -> bb1;
    }
}
"
    );
}

/// `low = min(low, xs[i])` on a shared `low` is a `Min` reduction: each iteration lowers the
/// `min` as written, against the chunk's private `low`.
#[test]
fn min_is_a_reduction() {
    let smallest = "
def smallest(xs: Array[f64, 1]) -> f64:
    low = xs[0]
    for i in prange(len(xs)):
        low = min(low, xs[i])
    return low
";
    assert_eq!(
        lower(smallest, &["smallest"]),
        "\
fn(_1: Array[f64, 1]) -> f64 {
    let _0: f64; // return
    let _1: Array[f64, 1]; // parameter xs
    let _2: f64; // low
    let _3: i64; // i
    let _4: i64;
    let _5: i64;
    let _6: bool;
    let _7: bool;
    let _8: bool;
    let _9: i64;
    let _10: i64;
    let _11: i64;
    let _12: i64;

    bb0: {
        _4 = len(_1, 0);
        _5 = 0_i64;
        _6 = GreaterEqual(copy _5, 0_i64);
        _7 = Less(copy _5, copy _4);
        _8 = BitAnd(copy _6, copy _7);
        assert(copy _8, index in bounds in dimension 0: copy _5 < copy _4) -> bb1;
    }

    bb1: {
        _2 = copy _1[_5];
        _9 = 0_i64;
        _11 = len(_1, 0);
        _10 = copy _11;
        _12 = Subtract(copy _10, copy _9);
        parallel_for body0(copy _12; _1, reduce Min _2) -> bb2;
    }

    bb2: {
        _0 = copy _2;
        return;
    }
}

body0: fn(_1: Owned[i64], _2: Array[f64, 1], _3: InOut[f64]) -> None {
    let _0: None; // return
    let _1: i64; // parameter iteration
    let _2: Array[f64, 1]; // parameter xs
    let _3: f64; // parameter low
    let _4: i64; // i
    let _5: f64;
    let _6: f64;
    let _7: i64;
    let _8: i64;
    let _9: bool;
    let _10: bool;
    let _11: bool;
    let _12: bool;

    bb0: {
        _4 = copy _1;
        _5 = copy _3;
        _7 = len(_2, 0);
        _8 = copy _4;
        _9 = GreaterEqual(copy _8, 0_i64);
        _10 = Less(copy _8, copy _7);
        _11 = BitAnd(copy _9, copy _10);
        assert(copy _11, index in bounds in dimension 0: copy _8 < copy _7) -> bb2;
    }

    bb1: {
        _0 = None;
        return;
    }

    bb2: {
        _6 = copy _2[_8];
        _12 = Less(copy _6, copy _5);
        branch copy _12 -> [true: bb3, false: bb4];
    }

    bb3: {
        _5 = copy _6;
        goto -> bb4;
    }

    bb4: {
        _3 = copy _5;
        goto -> bb1;
    }
}
"
    );
}

/// A `simd` loop lowers as the `range` loop inside it, with its header block marked.
#[test]
fn simd_loops_mark_their_header() {
    let program = r"
def axpy(a: f64, x: Array[f64, 1], y: InOut[Array[f64, 1]], n: i64):
    for i in simd(range(n)):
        y[i] = a * x[i] + y[i]
";
    assert_eq!(
        lower(program, &["axpy"]),
        "fn(_1: f64, _2: Array[f64, 1], _3: InOut[Array[f64, 1]], _4: i64) -> None {
    let _0: None; // return
    let _1: f64; // parameter a
    let _2: Array[f64, 1]; // parameter x
    let _3: Array[f64, 1]; // parameter y
    let _4: i64; // parameter n
    let _5: i64; // i
    let _6: i64;
    let _7: i64;
    let _8: bool;
    let _9: i64;
    let _10: i64;
    let _11: bool;
    let _12: bool;
    let _13: bool;
    let _14: f64;
    let _15: i64;
    let _16: i64;
    let _17: bool;
    let _18: bool;
    let _19: bool;
    let _20: i64;
    let _21: i64;
    let _22: bool;
    let _23: bool;
    let _24: bool;

    bb0: {
        _6 = 0_i64;
        _7 = copy _4;
        goto -> bb1;
    }

    bb1 (simd): {
        _8 = Less(copy _6, copy _7);
        branch copy _8 -> [true: bb2, false: bb4];
    }

    bb2: {
        _5 = copy _6;
        _9 = len(_3, 0);
        _10 = copy _5;
        _11 = GreaterEqual(copy _10, 0_i64);
        _12 = Less(copy _10, copy _9);
        _13 = BitAnd(copy _11, copy _12);
        assert(copy _13, index in bounds in dimension 0: copy _10 < copy _9) -> bb5;
    }

    bb3: {
        _6 = Add(copy _6, 1_i64);
        goto -> bb1;
    }

    bb4: {
        _0 = None;
        return;
    }

    bb5: {
        _15 = len(_2, 0);
        _16 = copy _5;
        _17 = GreaterEqual(copy _16, 0_i64);
        _18 = Less(copy _16, copy _15);
        _19 = BitAnd(copy _17, copy _18);
        assert(copy _19, index in bounds in dimension 0: copy _16 < copy _15) -> bb6;
    }

    bb6: {
        _14 = Multiply(copy _1, copy _2[_16]);
        _20 = len(_3, 0);
        _21 = copy _5;
        _22 = GreaterEqual(copy _21, 0_i64);
        _23 = Less(copy _21, copy _20);
        _24 = BitAnd(copy _22, copy _23);
        assert(copy _24, index in bounds in dimension 0: copy _21 < copy _20) -> bb7;
    }

    bb7: {
        _3[_10] = Add(copy _14, copy _3[_21]);
        goto -> bb3;
    }
}
"
    );
}

/// A `simd` loop that sums floats is not marked, so LLVM keeps the plain loop's order (roadmap
/// 1.9.14); one that sums integers is.
#[test]
fn simd_float_sums_stay_unvectorized() {
    let program = r"
def floats(x: Array[f64, 1], n: i64) -> f64:
    s = 0.0
    for i in simd(range(n)):
        s += x[i]
    return s


def integers(n: i64) -> i64:
    s = 0
    for i in simd(range(n)):
        s += i
    return s
";
    let text = lower(program, &["floats", "integers"]);
    assert_eq!(text.matches(" (simd): {").count(), 1, "{text}");
    let integers = text.find("fn(_1: i64) -> i64").unwrap();
    assert!(text[integers..].contains(" (simd): {"), "{text}");
}
