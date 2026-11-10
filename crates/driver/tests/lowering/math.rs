//! Lowering of `**` on floats, of `dot` and `round` on `Fixed` vectors, of `abs`, and of
//! number conversions.

use crate::lower;

/// `x ** 3` on a float is a call of `math.pow`, with the integer exponent converted to the
/// base's type, and `**=` calls it on the target in place.
#[test]
fn float_powers_call_pow() {
    let program = "\
        def cube(x: f64) -> f64:\n\
        \x20   y = x ** 3\n\
        \x20   y **= 0.5\n\
        \x20   return y\n";
    assert_eq!(
        lower(program, "cube"),
        "\
fn(_1: f64) -> f64 {
    let _0: f64; // return
    let _1: f64; // parameter x
    let _2: f64; // y

    bb0: {
        _2 = call math.pow(owned copy _1, owned 3.0_f64) -> [return: bb1];
    }

    bb1: {
        _2 = call math.pow(owned copy _2, owned 0.5_f64) -> [return: bb2];
    }

    bb2: {
        _0 = copy _2;
        return;
    }
}
"
    );
}

/// `v.dot(w)` adds the products in order, and `v.round()` calls `round` on each element.
#[test]
fn dot_and_round_of_a_fixed_vector() {
    let program = "\
        Vec2 = Fixed[f64, (2,)]\n\
        \n\
        \n\
        def snapped(v: Vec2) -> f64:\n\
        \x20   return v.round().dot(v)\n";
    assert_eq!(
        lower(program, "snapped"),
        "\
fn(_1: Fixed[f64, (2,)]) -> f64 {
    let _0: f64; // return
    let _1: Fixed[f64, (2,)]; // parameter v
    let _2: Fixed[f64, (2,)];
    let _3: i64;
    let _4: f64;
    let _5: i64;
    let _6: f64;
    let _7: f64;
    let _8: f64;
    let _9: i64;
    let _10: bool;

    bb0: {
        _3 = 0_i64;
        _4 = call round(owned copy _1[_3]) -> [return: bb1];
    }

    bb1: {
        _5 = 1_i64;
        _6 = call round(owned copy _1[_5]) -> [return: bb2];
    }

    bb2: {
        _2 = Fixed[f64, (2,)](copy _4, copy _6);
        _7 = 0.0_f64;
        _9 = 0_i64;
        goto -> bb3;
    }

    bb3: {
        _10 = Less(copy _9, 2_i64);
        branch copy _10 -> [true: bb4, false: bb5];
    }

    bb4: {
        _8 = Multiply(copy _2[_9], copy _1[_9]);
        _7 = Add(copy _7, copy _8);
        _9 = Add(copy _9, 1_i64);
        goto -> bb3;
    }

    bb5: {
        _0 = copy _7;
        return;
    }
}
"
    );
}

/// `sqrt(n)` on an integer converts `n` to `f64` first, and `math.pow` on an `f32` calls the
/// `f32` function, with its literal exponent written as an `f32`, as `x ** 3` does.
#[test]
fn math_functions_convert_their_arguments() {
    let program = "\
        import math\n\
        from math import sqrt\n\
        \n\
        \n\
        def roots(n: int, x: f32) -> f32:\n\
        \x20   r = sqrt(n)\n\
        \x20   return math.pow(x, 3)\n";
    assert_eq!(
        lower(program, "roots"),
        "\
fn(_1: i64, _2: f32) -> f32 {
    let _0: f32; // return
    let _1: i64; // parameter n
    let _2: f32; // parameter x
    let _3: f64; // r
    let _4: f64;

    bb0: {
        _4 = f64(copy _1);
        _3 = call math.sqrt(owned copy _4) -> [return: bb1];
    }

    bb1: {
        _0 = call math.pow(owned copy _2, owned 3.0_f32) -> [return: bb2];
    }

    bb2: {
        return;
    }
}
"
    );
}

/// `abs` of a float negates at zero too, so `abs(-0.0)` is `0.0`, and keeps everything else;
/// `abs` of an integer negates only below zero, checked for overflow as `-x` is, since the
/// absolute value of the type's minimum does not fit (decision 0030).
#[test]
fn abs_negates_below_zero() {
    let program = "\
        def size(x: f64, n: i32) -> f64:\n\
        \x20   return abs(x) + abs(n)\n";
    assert_eq!(
        lower(program, "size"),
        "\
fn(_1: f64, _2: i32) -> f64 {
    let _0: f64; // return
    let _1: f64; // parameter x
    let _2: i32; // parameter n
    let _3: f64;
    let _4: f64;
    let _5: bool;
    let _6: i32;
    let _7: i32;
    let _8: bool;
    let _9: tuple[i32, bool];
    let _10: bool;
    let _11: f64;

    bb0: {
        _4 = copy _1;
        _5 = LessEqual(copy _4, 0.0_f64);
        branch copy _5 -> [true: bb1, false: bb2];
    }

    bb1: {
        _3 = Subtract(0.0_f64, copy _4);
        goto -> bb3;
    }

    bb2: {
        _3 = copy _4;
        goto -> bb3;
    }

    bb3: {
        _7 = copy _2;
        _8 = Less(copy _7, 0_i32);
        branch copy _8 -> [true: bb4, false: bb5];
    }

    bb4: {
        _9 = CheckedSubtract(0_i32, copy _7);
        _10 = Not(copy _9.1);
        assert(copy _10, no overflow: Subtract) -> bb7;
    }

    bb5: {
        _6 = copy _7;
        goto -> bb6;
    }

    bb6: {
        _11 = f64(copy _6);
        _0 = Add(copy _3, copy _11);
        return;
    }

    bb7: {
        _6 = copy _9.0;
        goto -> bb6;
    }
}
"
    );
}

/// `float(text)` parses a string, which no backend does yet, so it is reported. Before this,
/// the string constant was stored into the float, and clang refused the program.
#[test]
fn a_string_converted_to_a_number_is_reported() {
    let program = "\
        def parse() -> f64:\n\
        \x20   return float(\"nan\")\n";
    assert_eq!(
        lower(program, "parse"),
        "\
error: lowering does not handle converting a `str` to `f64` yet
 --> main.byn:2:12
  |
2 |     return float(\"nan\")
  |            ^^^^^^^^^^^^ not lowered yet
  = note: the compiler lowers more of the language as roadmap item 1.9.3 continues"
    );
}
