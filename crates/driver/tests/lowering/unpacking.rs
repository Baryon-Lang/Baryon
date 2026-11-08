//! Lowering of unpacking assignment and of the array methods `copy` and `sum`.

use crate::lower;

/// `u, v = v, u` swaps two arrays by moving each into a tuple and out again, copying nothing:
/// the drops of the old values are of places already moved out, which drop elaboration removes.
#[test]
fn a_swap_moves_both_arrays() {
    let program = "\
        def swap(a: Owned[Array[f64, 1]], b: Owned[Array[f64, 1]]):\n\
        \x20   a, b = b, a\n";
    assert_eq!(
        lower(program, "swap"),
        "\
fn(_1: Owned[Array[f64, 1]], _2: Owned[Array[f64, 1]]) -> None {
    let _0: None; // return
    let _1: Array[f64, 1]; // parameter a
    let _2: Array[f64, 1]; // parameter b
    let _3: tuple[Array[f64, 1], Array[f64, 1]];
    let _4: Array[f64, 1];
    let _5: Array[f64, 1];

    bb0: {
        _3 = tuple[Array[f64, 1], Array[f64, 1]](move _2, move _1);
        _4 = move _3.0;
        drop(_1);
        _1 = move _4;
        _5 = move _3.1;
        drop(_2);
        _2 = move _5;
        drop(_3);
        _0 = None;
        drop(_2);
        drop(_1);
        return;
    }
}
"
    );
}

/// A `Fixed` vector unpacks element by element, each read at a constant index.
#[test]
fn a_fixed_vector_unpacks_into_its_elements() {
    let program = "\
        Vec2 = Fixed[f64, (2,)]\n\
        \n\
        \n\
        def norm1(v: Vec2) -> f64:\n\
        \x20   x, y = v\n\
        \x20   return x + y\n";
    assert_eq!(
        lower(program, "norm1"),
        "\
fn(_1: Fixed[f64, (2,)]) -> f64 {
    let _0: f64; // return
    let _1: Fixed[f64, (2,)]; // parameter v
    let _2: f64; // x
    let _3: f64; // y
    let _4: Fixed[f64, (2,)];
    let _5: i64;
    let _6: i64;

    bb0: {
        _4 = copy _1;
        _5 = 0_i64;
        _2 = copy _4[_5];
        _6 = 1_i64;
        _3 = copy _4[_6];
        _0 = Add(copy _2, copy _3);
        return;
    }
}
"
    );
}

/// `a.copy()` is a whole-array expression with one leaf, and `a.sum()` of floats calls the
/// pairwise `sum` built-in on it.
#[test]
fn copy_and_sum_of_an_array() {
    let program = "\
        def total(a: Array[f64, 2]) -> f64:\n\
        \x20   b = a.copy()\n\
        \x20   return b.sum()\n";
    assert_eq!(
        lower(program, "total"),
        "\
fn(_1: Array[f64, 2]) -> f64 {
    let _0: f64; // return
    let _1: Array[f64, 2]; // parameter a
    let _2: Array[f64, 2]; // b
    let _3: Array[f64, 2];
    let _4: f64;

    bb0: {
        _3 = elementwise copy _1;
        drop(_2);
        _2 = move _3;
        _4 = call sum(_2) -> [return: bb1];
    }

    bb1: {
        _0 = copy _4;
        drop(_2);
        return;
    }
}
"
    );
}
