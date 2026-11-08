//! Lowering of `random.Philox` and the constants of `math`.

use crate::lower;

/// `Philox(seed)` calls `random.Philox` with stream 0, `uniform()` passes the generator
/// `InOut`, and `math.pi` is a constant.
#[test]
fn philox_calls_and_math_constants() {
    let program = "\
        import math\n\
        from random import Philox\n\
        \n\
        \n\
        def draw(seed: int) -> f64:\n\
        \x20   generator = Philox(seed)\n\
        \x20   return generator.uniform() * math.pi\n";
    assert_eq!(
        lower(program, "draw"),
        "\
fn(_1: i64) -> f64 {
    let _0: f64; // return
    let _1: i64; // parameter seed
    let _2: Philox; // generator
    let _3: Philox;
    let _4: f64;

    bb0: {
        _3 = call random.Philox(owned copy _1, owned 0_i64) -> [return: bb1];
    }

    bb1: {
        drop(_2);
        _2 = move _3;
        _4 = call Philox.uniform(inout _2) -> [return: bb2];
    }

    bb2: {
        _0 = Multiply(copy _4, 3.141592653589793_f64);
        drop(_2);
        return;
    }
}
"
    );
}
