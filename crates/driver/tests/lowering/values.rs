//! Lowering of values: fields of `InOut` structs, moves and drops of owned values, narrowed
//! optionals, constants and conditional expressions.

use crate::lower;

/// An `InOut` struct's fields are read and written in place, `while` tests its condition on
/// every pass, and `break` leaves the loop.
#[test]
fn fields_of_an_inout_struct_are_places() {
    let program = "\
        class Particle(Struct):\n\
        \x20   x: f64\n\
        \x20   v: f64\n\
        \n\
        \n\
        def drift(p: InOut[Particle], dt: f64):\n\
        \x20   p.x += p.v * dt\n\
        \x20   while p.x > 10.0:\n\
        \x20       p.x -= 10.0\n\
        \x20       if p.x < 0.0:\n\
        \x20           break\n";
    assert_eq!(
        lower(program, "drift"),
        "\
fn(_1: InOut[Particle], _2: f64) -> None {
    let _0: None; // return
    let _1: Particle; // parameter p
    let _2: f64; // parameter dt
    let _3: f64;
    let _4: bool;
    let _5: bool;

    bb0: {
        _3 = Multiply(copy _1.1, copy _2);
        _1.0 = Add(copy _1.0, copy _3);
        goto -> bb1;
    }

    bb1: {
        _4 = Greater(copy _1.0, 10.0_f64);
        branch copy _4 -> [true: bb2, false: bb3];
    }

    bb2: {
        _1.0 = Subtract(copy _1.0, 10.0_f64);
        _5 = Less(copy _1.0, 0.0_f64);
        branch copy _5 -> [true: bb4, false: bb5];
    }

    bb3: {
        _0 = None;
        return;
    }

    bb4: {
        goto -> bb3;
    }

    bb5: {
        goto -> bb1;
    }
}
"
    );
}

/// A struct holding a `str` moves rather than copies: into a constructor, into an `Owned`
/// parameter and out as the result. A borrowed argument stays put, a temporary built only to
/// be borrowed is dropped after the call, a reassigned variable drops its old value, and every
/// exit, `raise` included, drops what the function owns.
#[test]
fn owned_values_move_and_drop() {
    let program = "\
        class Bad(Exception):\n\
        \x20   pass\n\
        \n\
        \n\
        class Cell(Struct):\n\
        \x20   name: str\n\
        \x20   mass: f64\n\
        \n\
        \n\
        def weigh(c: Cell) -> f64:\n\
        \x20   return c.mass\n\
        \n\
        \n\
        def keep(c: Owned[Cell]) -> Cell:\n\
        \x20   return c\n\
        \n\
        \n\
        def make(m: f64) -> Raises[Cell, Bad]:\n\
        \x20   if m < 0.0:\n\
        \x20       raise Bad()\n\
        \x20   label = \"cell\"\n\
        \x20   c = Cell(label, m)\n\
        \x20   w = weigh(c)\n\
        \x20   label = \"other\"\n\
        \x20   d = keep(c)\n\
        \x20   weigh(Cell(\"x\", 2.0))\n\
        \x20   return d\n";
    assert_eq!(
        lower(program, "make"),
        "\
fn(_1: f64) -> Cell {
    let _0: Cell; // return
    let _1: f64; // parameter m
    let _2: str; // label
    let _3: Cell; // c
    let _4: f64; // w
    let _5: Cell; // d
    let _6: bool;
    let _7: Bad;
    let _8: Bad;
    let _9: str;
    let _10: Cell;
    let _11: str;
    let _12: Cell;
    let _13: f64;
    let _14: Cell;

    bb0: {
        _6 = Less(copy _1, 0.0_f64);
        branch copy _6 -> [true: bb1, false: bb2];
    }

    bb1: {
        _7 = Bad();
        _8 = move _7;
        drop(_7);
        drop(_5);
        drop(_3);
        drop(_2);
        raise move _8;
    }

    bb2: {
        _9 = \"cell\";
        drop(_2);
        _2 = move _9;
        _10 = Cell(move _2, copy _1);
        drop(_3);
        _3 = move _10;
        _4 = call weigh(_3) -> [return: bb3];
    }

    bb3: {
        _11 = \"other\";
        drop(_2);
        _2 = move _11;
        _12 = call keep(owned move _3) -> [return: bb4];
    }

    bb4: {
        drop(_5);
        _5 = move _12;
        _14 = Cell(\"x\", 2.0_f64);
        _13 = call weigh(_14) -> [return: bb5];
    }

    bb5: {
        drop(_14);
        _0 = move _5;
        drop(_5);
        drop(_3);
        drop(_2);
        return;
    }
}
"
    );
}

/// After `x is None` fails, `x` reads the value inside the optional.
#[test]
fn a_none_check_narrows_to_the_value() {
    let program = "\
        def value_or_zero(x: f64 | None) -> f64:\n\
        \x20   if x is None:\n\
        \x20       return 0.0\n\
        \x20   return x\n";
    assert_eq!(
        lower(program, "value_or_zero"),
        "\
fn(_1: f64 | None) -> f64 {
    let _0: f64; // return
    let _1: f64 | None; // parameter x
    let _2: bool;
    let _3: bool;

    bb0: {
        _3 = is_present(_1);
        _2 = Not(copy _3);
        branch copy _2 -> [true: bb1, false: bb2];
    }

    bb1: {
        _0 = 0.0_f64;
        return;
    }

    bb2: {
        _0 = copy _1.value;
        return;
    }
}
"
    );
}

/// A `Final` constant and an enum member are constants, a conditional expression branches,
/// and a function that returns nothing returns `None` at its end.
#[test]
fn constants_and_conditional_expressions() {
    let program = "\
        LIMIT: Final = 100\n\
        \n\
        \n\
        class Side(Enum):\n\
        \x20   LEFT = 1\n\
        \x20   RIGHT = 2\n\
        \n\
        \n\
        def pick(n: i64) -> Side:\n\
        \x20   return Side.LEFT if n < LIMIT else Side.RIGHT\n\
        \n\
        \n\
        def count(n: i64):\n\
        \x20   k = n\n\
        \x20   while k > 0:\n\
        \x20       k -= 1\n\
        \x20       if k == LIMIT:\n\
        \x20           continue\n";
    assert_eq!(
        lower(program, "pick"),
        "\
fn(_1: i64) -> Side {
    let _0: Side; // return
    let _1: i64; // parameter n
    let _2: bool;

    bb0: {
        _2 = Less(copy _1, 100_i64);
        branch copy _2 -> [true: bb1, false: bb2];
    }

    bb1: {
        _0 = 0_Side;
        goto -> bb3;
    }

    bb2: {
        _0 = 1_Side;
        goto -> bb3;
    }

    bb3: {
        return;
    }
}
"
    );
    assert_eq!(
        lower(program, "count"),
        "\
fn(_1: i64) -> None {
    let _0: None; // return
    let _1: i64; // parameter n
    let _2: i64; // k
    let _3: bool;
    let _4: tuple[i64, bool];
    let _5: bool;
    let _6: bool;

    bb0: {
        _2 = copy _1;
        goto -> bb1;
    }

    bb1: {
        _3 = Greater(copy _2, 0_i64);
        branch copy _3 -> [true: bb2, false: bb3];
    }

    bb2: {
        _4 = CheckedSubtract(copy _2, 1_i64);
        _5 = Not(copy _4.1);
        assert(copy _5, no overflow: Subtract) -> bb4;
    }

    bb3: {
        _0 = None;
        return;
    }

    bb4: {
        _2 = copy _4.0;
        _6 = Equal(copy _2, 100_i64);
        branch copy _6 -> [true: bb5, false: bb6];
    }

    bb5: {
        goto -> bb1;
    }

    bb6: {
        goto -> bb1;
    }
}
"
    );
}
