//! End-to-end tests of lowering (roadmap 1.9.3): programs parsed, resolved and checked, then
//! each function lowered to the mid-level IR and compared by its full text form. Lowering checks
//! every body it builds against the IR's structural rules in debug builds, so each test also
//! validates its body.
//!
//! This file holds the helper every test shares and the tests of control flow and checks; each
//! submodule tests one other part of lowering.

use driver::Session;

mod calls;
mod dicts;
mod lists;
mod math;
mod random;
mod slices;
mod unpacking;
mod values;

/// The IR text of the function `name` in `program`. When the program has errors, or lowering
/// gives some, it returns them instead, rendered and joined by blank lines, so a failing test
/// shows what went wrong.
pub(crate) fn lower(program: &str, name: &str) -> String {
    let mut session = Session::new();
    if let Err(error) = session.add_file("main.byn", program) {
        return error.to_string();
    }
    let mut errors = session.check();
    if errors.is_empty() {
        match session.mir(name) {
            None => return format!("no function `{name}`"),
            Some(Ok(text)) => return text,
            Some(Err(found)) => errors = found,
        }
    }
    errors
        .iter()
        .map(|error| diagnostics::render(error, &session).to_string())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// A `for` over `range` keeps a hidden counter, `or` tests its right side only when the left is
/// false, and `+=` adds in place; `n + 1` and `+=` check for overflow (decision 0030).
#[test]
fn loops_and_conditions_become_blocks() {
    let program = "\
        def sum_multiples(n: i64) -> i64:\n\
        \x20   total = 0\n\
        \x20   for i in range(1, n + 1):\n\
        \x20       if i % 3 == 0 or i % 5 == 0:\n\
        \x20           total += i\n\
        \x20   return total\n";
    assert_eq!(
        lower(program, "sum_multiples"),
        "\
fn(_1: i64) -> i64 {
    let _0: i64; // return
    let _1: i64; // parameter n
    let _2: i64; // total
    let _3: i64; // i
    let _4: i64;
    let _5: i64;
    let _6: tuple[i64, bool];
    let _7: bool;
    let _8: bool;
    let _9: bool;
    let _10: i64;
    let _11: i64;
    let _12: tuple[i64, bool];
    let _13: bool;

    bb0: {
        _2 = 0_i64;
        _4 = 1_i64;
        _6 = CheckedAdd(copy _1, 1_i64);
        _7 = Not(copy _6.1);
        assert(copy _7, no overflow: Add) -> bb1;
    }

    bb1: {
        _5 = copy _6.0;
        goto -> bb2;
    }

    bb2: {
        _8 = Less(copy _4, copy _5);
        branch copy _8 -> [true: bb3, false: bb5];
    }

    bb3: {
        _3 = copy _4;
        _10 = Remainder(copy _3, 3_i64);
        _9 = Equal(copy _10, 0_i64);
        branch copy _9 -> [true: bb7, false: bb6];
    }

    bb4: {
        _4 = Add(copy _4, 1_i64);
        goto -> bb2;
    }

    bb5: {
        _0 = copy _2;
        return;
    }

    bb6: {
        _11 = Remainder(copy _3, 5_i64);
        _9 = Equal(copy _11, 0_i64);
        goto -> bb7;
    }

    bb7: {
        branch copy _9 -> [true: bb8, false: bb9];
    }

    bb8: {
        _12 = CheckedAdd(copy _2, copy _3);
        _13 = Not(copy _12.1);
        assert(copy _13, no overflow: Add) -> bb10;
    }

    bb9: {
        goto -> bb4;
    }

    bb10: {
        _2 = copy _12.0;
        goto -> bb9;
    }
}
"
    );
}

/// `0 < x <= 10` tests its second link only when the first held, and `and` skips its right
/// side when the left is false.
#[test]
fn chained_comparisons_short_circuit() {
    let program = "\
        def in_range(x: i32, y: i64) -> bool:\n\
        \x20   return 0 < x <= 10 and y // 2 != 1\n";
    assert_eq!(
        lower(program, "in_range"),
        "\
fn(_1: i32, _2: i64) -> bool {
    let _0: bool; // return
    let _1: i32; // parameter x
    let _2: i64; // parameter y
    let _3: i64;

    bb0: {
        _0 = Less(0_i32, copy _1);
        branch copy _0 -> [true: bb1, false: bb2];
    }

    bb1: {
        _0 = LessEqual(copy _1, 10_i32);
        goto -> bb2;
    }

    bb2: {
        branch copy _0 -> [true: bb3, false: bb4];
    }

    bb3: {
        _3 = FloorDivide(copy _2, 2_i64);
        _0 = NotEqual(copy _3, 1_i64);
        goto -> bb4;
    }

    bb4: {
        return;
    }
}
"
    );
}

/// Integer `//` checks its divisor first, `assert` panics unless its condition holds, and `/`
/// on integers converts both sides to `f64` (decision 0014).
#[test]
fn division_and_assert_are_checked() {
    let program = "\
        def ratio(n: i64, d: i64) -> f64:\n\
        \x20   r = n // d\n\
        \x20   assert r > 0\n\
        \x20   return n / d\n";
    assert_eq!(
        lower(program, "ratio"),
        "\
fn(_1: i64, _2: i64) -> f64 {
    let _0: f64; // return
    let _1: i64; // parameter n
    let _2: i64; // parameter d
    let _3: i64; // r
    let _4: bool;
    let _5: bool;
    let _6: f64;
    let _7: f64;

    bb0: {
        _4 = NotEqual(copy _2, 0_i64);
        assert(copy _4, non-zero divisor) -> bb1;
    }

    bb1: {
        _3 = FloorDivide(copy _1, copy _2);
        _5 = Greater(copy _3, 0_i64);
        assert(copy _5, assertion) -> bb2;
    }

    bb2: {
        _6 = f64(copy _1);
        _7 = f64(copy _2);
        _0 = Divide(copy _6, copy _7);
        return;
    }
}
"
    );
}

/// A construct outside the first slice is an error that names it, and gives no body. `str(x)`
/// is not lowered at all, and `in` on a tuple not yet; a list of `str`s is.
#[test]
fn constructs_not_lowered_yet_are_errors() {
    let program = "\
        def power(x: f64, n: i64) -> i64:\n\
        \x20   ys = [str(x)]\n\
        \x20   return n ** 2 if n in (1, 2) else n\n";
    assert_eq!(
        lower(program, "power"),
        "\
error: lowering does not handle the built-in `str` yet
 --> main.byn:2:11
  |
2 |     ys = [str(x)]
  |           ^^^^^^ not lowered yet
  = note: the compiler lowers more of the language as roadmap item 1.9.3 continues

error: lowering does not handle `in` yet
 --> main.byn:3:23
  |
3 |     return n ** 2 if n in (1, 2) else n
  |                       ^^^^^^^^^^ not lowered yet
  = note: the compiler lowers more of the language as roadmap item 1.9.3 continues"
    );
}

/// A `range` with a negative step tests the counter against the stop the other way, and
/// `break` leaves the loop.
#[test]
fn a_negative_step_counts_down() {
    let program = "\
        def first_negative(n: i64) -> i64:\n\
        \x20   found = 0\n\
        \x20   for i in range(n, -10, -2):\n\
        \x20       if i < 0:\n\
        \x20           found = i\n\
        \x20           break\n\
        \x20   return found\n";
    assert_eq!(
        lower(program, "first_negative"),
        "\
fn(_1: i64) -> i64 {
    let _0: i64; // return
    let _1: i64; // parameter n
    let _2: i64; // found
    let _3: i64; // i
    let _4: i64;
    let _5: i64;
    let _6: bool;
    let _7: bool;

    bb0: {
        _2 = 0_i64;
        _4 = copy _1;
        _5 = -10_i64;
        goto -> bb1;
    }

    bb1: {
        _6 = Greater(copy _4, copy _5);
        branch copy _6 -> [true: bb2, false: bb4];
    }

    bb2: {
        _3 = copy _4;
        _7 = Less(copy _3, 0_i64);
        branch copy _7 -> [true: bb5, false: bb6];
    }

    bb3: {
        _4 = Add(copy _4, -2_i64);
        goto -> bb1;
    }

    bb4: {
        _0 = copy _2;
        return;
    }

    bb5: {
        _2 = copy _3;
        goto -> bb4;
    }

    bb6: {
        goto -> bb3;
    }
}
"
    );
}
