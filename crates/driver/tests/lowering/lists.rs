//! Lowering of lists: displays, `append`, `len`, indexing and `Array(xs)`.

use crate::lower;

/// A display converts each item to the element type, so `[1]` in a `list[f64]` holds `1.0`;
/// `append` passes the list `InOut` and the value `Owned`; `len` reads the length, and an index
/// is checked against it.
#[test]
fn displays_append_len_and_index() {
    let program = "\
        def grow(x: f64) -> f64:\n\
        \x20   xs: list[f64] = [1, x]\n\
        \x20   xs.append(2)\n\
        \x20   return xs[len(xs) - 1]\n";
    assert_eq!(
        lower(program, "grow"),
        "\
fn(_1: f64) -> f64 {
    let _0: f64; // return
    let _1: f64; // parameter x
    let _2: list[f64]; // xs
    let _3: list[f64];
    let _4: None;
    let _5: i64;
    let _6: i64;
    let _7: i64;
    let _8: i64;
    let _9: i64;
    let _10: tuple[i64, bool];
    let _11: bool;
    let _12: bool;
    let _13: bool;
    let _14: bool;

    bb0: {
        _3 = list[f64](1.0_f64, copy _1);
        drop(_2);
        _2 = move _3;
        _4 = call list.append(inout _2, owned 2.0_f64) -> [return: bb1];
    }

    bb1: {
        _5 = len(_2, 0);
        _9 = len(_2, 0);
        _8 = copy _9;
        _10 = CheckedSubtract(copy _8, 1_i64);
        _11 = Not(copy _10.1);
        assert(copy _11, no overflow: Subtract) -> bb2;
    }

    bb2: {
        _7 = copy _10.0;
        _6 = copy _7;
        _12 = GreaterEqual(copy _6, 0_i64);
        _13 = Less(copy _6, copy _5);
        _14 = BitAnd(copy _12, copy _13);
        assert(copy _14, index in bounds in dimension 0: copy _6 < copy _5) -> bb3;
    }

    bb3: {
        _0 = copy _2[_6];
        drop(_2);
        return;
    }
}
"
    );
}

/// `Array(xs)` allocates an array as long as the list and copies each element in a loop; the
/// list is only borrowed.
#[test]
fn arrays_copy_a_list() {
    let program = "\
        def build(xs: list[i64]) -> Array[i64, 1]:\n\
        \x20   return Array(xs)\n";
    assert_eq!(
        lower(program, "build"),
        "\
fn(_1: list[i64]) -> Array[i64, 1] {
    let _0: Array[i64, 1]; // return
    let _1: list[i64]; // parameter xs
    let _2: i64;
    let _3: i64;
    let _4: bool;

    bb0: {
        _2 = len(_1, 0);
        _0 = new Array[i64, 1](copy _2);
        _3 = 0_i64;
        goto -> bb1;
    }

    bb1: {
        _4 = Less(copy _3, copy _2);
        branch copy _4 -> [true: bb2, false: bb3];
    }

    bb2: {
        _0[_3] = copy _1[_3];
        _3 = Add(copy _3, 1_i64);
        goto -> bb1;
    }

    bb3: {
        return;
    }
}
"
    );
}
