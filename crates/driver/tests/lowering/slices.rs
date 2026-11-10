//! Lowering of slices: a range becomes its start, length and step, checked to lie within its
//! dimension, and the subscript a place with a `Slice` step.

use crate::lower;

/// `a[1:n]` passed to a borrowed parameter: the default step is the constant 1, so it needs no
/// zero check and the range's length is `stop - start`, or 0 for a range that ends before it
/// starts; the start and the stop the program wrote are each checked from 0 to the length.
#[test]
fn a_forward_range_is_checked_and_measured() {
    let program = "\
        def total(a: Array[f64, 1]) -> f64:\n\
        \x20   return a.sum()\n\
        \n\
        \n\
        def tail(a: Array[f64, 1], n: i64) -> f64:\n\
        \x20   return total(a[1:n])\n";
    assert_eq!(
        lower(program, "tail"),
        "\
fn(_1: Array[f64, 1], _2: i64) -> f64 {
    let _0: f64; // return
    let _1: Array[f64, 1]; // parameter a
    let _2: i64; // parameter n
    let _3: i64;
    let _4: i64;
    let _5: i64;
    let _6: i64;
    let _7: i64;
    let _8: bool;
    let _9: bool;
    let _10: bool;
    let _11: bool;
    let _12: bool;

    bb0: {
        _3 = len(_1, 0);
        _4 = 1_i64;
        _5 = 1_i64;
        _6 = copy _2;
        _7 = Subtract(copy _6, copy _5);
        _8 = Less(copy _7, 0_i64);
        branch copy _8 -> [true: bb1, false: bb2];
    }

    bb1: {
        _7 = 0_i64;
        goto -> bb2;
    }

    bb2: {
        _9 = GreaterEqual(copy _5, 0_i64);
        _10 = LessEqual(copy _5, copy _3);
        _11 = GreaterEqual(copy _6, 0_i64);
        _12 = LessEqual(copy _6, copy _3);
        _9 = BitAnd(copy _9, copy _10);
        _9 = BitAnd(copy _9, copy _11);
        _9 = BitAnd(copy _9, copy _12);
        assert(copy _9, slice in bounds in dimension 0: copy _5:copy _6 within copy _3) -> bb3;
    }

    bb3: {
        _0 = call total(_1[_5 for _7 by _4]) -> [return: bb4];
    }

    bb4: {
        return;
    }
}
"
    );
}

/// `grid[::s, k] += 1.0` stays one whole-array expression whose destination is the view. The
/// step `s` is known only at run time, so it is checked not to be 0, its sign picks the
/// defaults, and the length is `ceil(distance / |s|)`; the index `k` is checked as any index.
#[test]
fn a_column_is_written_in_place() {
    let program = "\
        def bump(grid: InOut[Array[f64, 2]], k: i64, s: i64):\n\
        \x20   grid[::s, k] += 1.0\n";
    assert_eq!(
        lower(program, "bump"),
        "\
fn(_1: InOut[Array[f64, 2]], _2: i64, _3: i64) -> None {
    let _0: None; // return
    let _1: Array[f64, 2]; // parameter grid
    let _2: i64; // parameter k
    let _3: i64; // parameter s
    let _4: i64;
    let _5: i64;
    let _6: bool;
    let _7: i64;
    let _8: i64;
    let _9: i64;
    let _10: bool;
    let _11: i64;
    let _12: i64;
    let _13: bool;
    let _14: i64;
    let _15: bool;
    let _16: i64;
    let _17: i64;
    let _18: bool;
    let _19: bool;
    let _20: bool;

    bb0: {
        _4 = len(_1, 0);
        _5 = copy _3;
        _6 = NotEqual(copy _5, 0_i64);
        assert(copy _6, non-zero slice step) -> bb1;
    }

    bb1: {
        _10 = Less(copy _5, 0_i64);
        branch copy _10 -> [true: bb2, false: bb3];
    }

    bb2: {
        _7 = Subtract(copy _4, 1_i64);
        _8 = -1_i64;
        _9 = Subtract(copy _7, copy _8);
        _11 = Subtract(0_i64, copy _5);
        _12 = Add(copy _9, copy _11);
        _12 = Subtract(copy _12, 1_i64);
        _9 = FloorDivide(copy _12, copy _11);
        _13 = Less(copy _9, 0_i64);
        branch copy _13 -> [true: bb5, false: bb6];
    }

    bb3: {
        _7 = 0_i64;
        _8 = copy _4;
        _9 = Subtract(copy _8, copy _7);
        _14 = Add(copy _9, copy _5);
        _14 = Subtract(copy _14, 1_i64);
        _9 = FloorDivide(copy _14, copy _5);
        _15 = Less(copy _9, 0_i64);
        branch copy _15 -> [true: bb7, false: bb8];
    }

    bb4: {
        _16 = len(_1, 1);
        _17 = copy _2;
        _18 = GreaterEqual(copy _17, 0_i64);
        _19 = Less(copy _17, copy _16);
        _20 = BitAnd(copy _18, copy _19);
        assert(copy _20, index in bounds in dimension 1: copy _17 < copy _16) -> bb9;
    }

    bb5: {
        _9 = 0_i64;
        goto -> bb6;
    }

    bb6: {
        goto -> bb4;
    }

    bb7: {
        _9 = 0_i64;
        goto -> bb8;
    }

    bb8: {
        goto -> bb4;
    }

    bb9: {
        _1[_7 for _9 by _5, _17] = elementwise Add(copy _1[_7 for _9 by _5, _17], 1.0_f64);
        _0 = None;
        return;
    }
}
"
    );
}
