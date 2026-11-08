//! Lowering of `**` on floats and of `dot` and `round` on `Fixed` vectors.

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
