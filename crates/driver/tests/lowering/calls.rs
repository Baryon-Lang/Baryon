//! Lowering of calls: arguments in their parameters' types, calls inside generic bodies, and the
//! built-ins `min` and `max`.

use crate::lower;

/// An argument passed where its parameter wants an optional or another number type is converted
/// first, into a temporary when the parameter is borrowed, so the callee reads the layout it
/// expects. A value that owns memory cannot be wrapped without moving it, so borrowing one as an
/// optional is not lowered yet.
#[test]
fn arguments_take_their_parameters_type() {
    let program = r"class Particle(Struct, Copy):
    mass: f64


def f(particle: Particle | None, d: f64 | None, n: Owned[i64 | None]) -> f64:
    return 0.0


def g(values: Array[f64, 1] | None) -> f64:
    return 0.0


def caller() -> f64:
    p = Particle(2.0)
    return f(p, 1.0, 3)


def array_caller(a: Array[f64, 1]) -> f64:
    return g(a)
";
    assert_eq!(
        lower(program, "caller"),
        r"fn() -> f64 {
    let _0: f64; // return
    let _1: Particle; // p
    let _2: Particle | None;
    let _3: f64 | None;
    let _4: i64 | None;

    bb0: {
        _1 = Particle(2.0_f64);
        _2 = Particle | None(copy _1);
        _3 = f64 | None(1.0_f64);
        _4 = i64 | None(3_i64);
        _0 = call f(_2, _3, owned copy _4) -> [return: bb1];
    }

    bb1: {
        return;
    }
}
"
    );
    assert_eq!(
        lower(program, "array_caller"),
        r"error: lowering does not handle borrowing a value that is not a copy type as an optional yet
  --> main.byn:19:14
   |
19 |     return g(a)
   |              ^ not lowered yet
   = note: the compiler lowers more of the language as roadmap item 1.9.3 continues"
    );
}

/// A generic body keeps its type parameters: `+`, `-` and `*` on `T` are the instance's
/// arithmetic, a literal of type `T` keeps the value written, and a call of a generic function
/// names its type arguments. `//` depends on whether `T` is an integer, so it waits.
#[test]
fn generic_bodies_keep_their_type_parameters() {
    let program = r"def square[T: Number](x: T) -> T:
    return x * x


def twice[T: Number](x: T) -> T:
    return square(x) * 2 - x


def halve[T: Number](x: T) -> T:
    return x // 2
";
    assert_eq!(
        lower(program, "twice"),
        r"fn(_1: T) -> T {
    let _0: T; // return
    let _1: T; // parameter x
    let _2: T;
    let _3: T;

    bb0: {
        _3 = call square[T](_1) -> [return: bb1];
    }

    bb1: {
        _2 = Multiply(copy _3, 2_T);
        _0 = Subtract(copy _2, copy _1);
        return;
    }
}
"
    );
    assert_eq!(
        lower(program, "halve"),
        r"error: lowering does not handle `//` on a type parameter yet
  --> main.byn:10:12
   |
10 |     return x // 2
   |            ^^^^^^ not lowered yet
   = note: the compiler lowers more of the language as roadmap item 1.9.3 continues"
    );
}

/// `min` and `max` of numbers convert every argument to the result type, then keep the first
/// best value: a later argument replaces it only when strictly smaller, or larger, as in Python.
#[test]
fn min_and_max_keep_the_first_best_value() {
    let program = "
def clamp(x: f64, n: i64) -> f64:
    return max(0, min(x, n, 10.5))
";
    assert_eq!(
        lower(program, "clamp"),
        "\
fn(_1: f64, _2: i64) -> f64 {
    let _0: f64; // return
    let _1: f64; // parameter x
    let _2: i64; // parameter n
    let _3: f64;
    let _4: f64;
    let _5: f64;
    let _6: f64;
    let _7: f64;
    let _8: f64;
    let _9: f64;
    let _10: bool;
    let _11: f64;
    let _12: bool;
    let _13: bool;

    bb0: {
        _4 = f64(0_i64);
        _3 = copy _4;
        _7 = copy _1;
        _9 = f64(copy _2);
        _8 = copy _9;
        _10 = Less(copy _8, copy _7);
        branch copy _10 -> [true: bb1, false: bb2];
    }

    bb1: {
        _7 = copy _8;
        goto -> bb2;
    }

    bb2: {
        _11 = 10.5_f64;
        _12 = Less(copy _11, copy _7);
        branch copy _12 -> [true: bb3, false: bb4];
    }

    bb3: {
        _7 = copy _11;
        goto -> bb4;
    }

    bb4: {
        _6 = copy _7;
        _5 = copy _6;
        _13 = Greater(copy _5, copy _3);
        branch copy _13 -> [true: bb5, false: bb6];
    }

    bb5: {
        _3 = copy _5;
        goto -> bb6;
    }

    bb6: {
        _0 = copy _3;
        return;
    }
}
"
    );
}
