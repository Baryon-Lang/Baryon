//! Lowering of lists: displays, `append`, `len`, indexing, `Array(xs)` and comprehensions.

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

/// A comprehension is the loop it reads as: an empty list, a loop over `xs` that reads each
/// element at its counter, a branch past the append when the condition is false, and an append
/// of the value.
#[test]
fn a_comprehension_is_a_loop_that_appends() {
    let program = "\
        def positives(xs: list[f64]) -> list[f64]:\n\
        \x20   return [2 * x for x in xs if x > 0]\n";
    assert_eq!(
        lower(program, "positives"),
        "\
fn(_1: list[f64]) -> list[f64] {
    let _0: list[f64]; // return
    let _1: list[f64]; // parameter xs
    let _2: f64; // x
    let _3: i64;
    let _4: i64;
    let _5: i64;
    let _6: bool;
    let _7: bool;
    let _8: f64;
    let _9: None;

    bb0: {
        _0 = list[f64]();
        _3 = len(_1, 0);
        _4 = copy _3;
        _5 = 0_i64;
        goto -> bb1;
    }

    bb1: {
        _6 = Less(copy _5, copy _4);
        branch copy _6 -> [true: bb2, false: bb4];
    }

    bb2: {
        _2 = copy _1[_5];
        _7 = Greater(copy _2, 0.0_f64);
        branch copy _7 -> [true: bb5, false: bb3];
    }

    bb3: {
        _5 = Add(copy _5, 1_i64);
        goto -> bb1;
    }

    bb4: {
        return;
    }

    bb5: {
        _8 = Multiply(2.0_f64, copy _2);
        _9 = call list.append(inout _0, owned copy _8) -> [return: bb6];
    }

    bb6: {
        goto -> bb3;
    }
}
"
    );
}

/// `prange` in a comprehension is not lowered yet: it is a statement's parallel loop. Nor is a
/// loop variable that borrows a `str` and is also used outside the loop, since reading it as the
/// element it borrows is exact only inside.
#[test]
fn loops_not_lowered_yet_in_comprehensions_are_errors() {
    let program = "\
        def lengths(names: list[str]) -> list[i64]:\n\
        \x20   evens = [i for i in prange(4)]\n\
        \x20   name = \"none\"\n\
        \x20   for name in names:\n\
        \x20       print(name)\n\
        \x20   return evens\n";
    assert_eq!(
        lower(program, "lengths"),
        "\
error: lowering does not handle `prange` and `simd` in a comprehension yet
 --> main.byn:2:25
  |
2 |     evens = [i for i in prange(4)]
  |                         ^^^^^^^^^ not lowered yet
  = note: the compiler lowers more of the language as roadmap item 1.9.3 continues

error: lowering does not handle a loop variable that borrows an element and is also used outside the loop yet
 --> main.byn:4:9
  |
4 |     for name in names:
  |         ^^^^ not lowered yet
  = note: the compiler lowers more of the language as roadmap item 1.9.3 continues"
    );
}

/// A loop over a list of `str`s borrows each element: the loop variable gets no value of its
/// own, and each read of it reads the element in place, so nothing is copied or dropped.
#[test]
fn a_loop_over_strings_borrows_each_element() {
    let program = "\
        def total(names: list[str]) -> i64:\n\
        \x20   n = 0\n\
        \x20   for name in names:\n\
        \x20       n += len(name)\n\
        \x20   return n\n";
    assert_eq!(
        lower(program, "total"),
        "\
fn(_1: list[str]) -> i64 {
    let _0: i64; // return
    let _1: list[str]; // parameter names
    let _2: i64; // n
    let _3: str; // name
    let _4: i64;
    let _5: i64;
    let _6: i64;
    let _7: bool;
    let _8: i64;
    let _9: tuple[i64, bool];
    let _10: bool;

    bb0: {
        _2 = 0_i64;
        _4 = len(_1, 0);
        _5 = copy _4;
        _6 = 0_i64;
        goto -> bb1;
    }

    bb1: {
        _7 = Less(copy _6, copy _5);
        branch copy _7 -> [true: bb2, false: bb4];
    }

    bb2: {
        _8 = call len of str(_1[_6]) -> [return: bb5];
    }

    bb3: {
        _6 = Add(copy _6, 1_i64);
        goto -> bb1;
    }

    bb4: {
        _0 = copy _2;
        drop(_3);
        return;
    }

    bb5: {
        _9 = CheckedAdd(copy _2, copy _8);
        _10 = Not(copy _9.1);
        assert(copy _10, no overflow: Add) -> bb6;
    }

    bb6: {
        _2 = copy _9.0;
        goto -> bb3;
    }
}
"
    );
}

/// A loop over a `str` counts its characters once, then each pass takes the character at a byte
/// cursor, which the call moves on; the loop variable borrows it in place.
#[test]
fn a_loop_over_a_string_takes_one_character_at_a_time() {
    let program = "\
        def vowels(text: str) -> i64:\n\
        \x20   n = 0\n\
        \x20   for c in text:\n\
        \x20       if c in \"aeiou\":\n\
        \x20           n += 1\n\
        \x20   return n\n";
    assert_eq!(
        lower(program, "vowels"),
        "\
fn(_1: str) -> i64 {
    let _0: i64; // return
    let _1: str; // parameter text
    let _2: i64; // n
    let _3: str; // c
    let _4: i64;
    let _5: i64;
    let _6: i64;
    let _7: i64;
    let _8: bool;
    let _9: str;
    let _10: bool;
    let _11: str;
    let _12: tuple[i64, bool];
    let _13: bool;

    bb0: {
        _2 = 0_i64;
        _4 = call len of str(_1) -> [return: bb1];
    }

    bb1: {
        _5 = 0_i64;
        _6 = copy _4;
        _7 = 0_i64;
        goto -> bb2;
    }

    bb2: {
        _8 = Less(copy _7, copy _6);
        branch copy _8 -> [true: bb3, false: bb5];
    }

    bb3: {
        _9 = call str character(_1, inout _5) -> [return: bb6];
    }

    bb4: {
        _7 = Add(copy _7, 1_i64);
        goto -> bb2;
    }

    bb5: {
        _0 = copy _2;
        drop(_3);
        return;
    }

    bb6: {
        _11 = \"aeiou\";
        _10 = call str in(_11, _9) -> [return: bb7];
    }

    bb7: {
        drop(_11);
        branch copy _10 -> [true: bb8, false: bb9];
    }

    bb8: {
        _12 = CheckedAdd(copy _2, 1_i64);
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
