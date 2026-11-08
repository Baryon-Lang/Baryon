//! End-to-end tests of lowering (roadmap 1.9.3): programs parsed, resolved and checked, then
//! each function lowered to the mid-level IR and compared by its full text form. Lowering checks
//! every body it builds against the IR's structural rules in debug builds, so each test also
//! validates its body.

use driver::Session;

/// The IR text of the function `name` in `program`. When the program has errors, or lowering
/// gives some, it returns them instead, rendered and joined by blank lines, so a failing test
/// shows what went wrong.
fn lower(program: &str, name: &str) -> String {
    let mut session = Session::new();
    if let Err(error) = session.add_file("main.npl", program) {
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
/// false, and `+=` adds in place.
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
    let _6: bool;
    let _7: bool;
    let _8: i64;
    let _9: i64;

    bb0: {
        _2 = 0_i64;
        _4 = 1_i64;
        _5 = Add(copy _1, 1_i64);
        goto -> bb1;
    }

    bb1: {
        _6 = Less(copy _4, copy _5);
        branch copy _6 -> [true: bb2, false: bb4];
    }

    bb2: {
        _3 = copy _4;
        _8 = Remainder(copy _3, 3_i64);
        _7 = Equal(copy _8, 0_i64);
        branch copy _7 -> [true: bb6, false: bb5];
    }

    bb3: {
        _4 = Add(copy _4, 1_i64);
        goto -> bb1;
    }

    bb4: {
        _0 = copy _2;
        return;
    }

    bb5: {
        _9 = Remainder(copy _3, 5_i64);
        _7 = Equal(copy _9, 0_i64);
        goto -> bb6;
    }

    bb6: {
        branch copy _7 -> [true: bb7, false: bb8];
    }

    bb7: {
        _2 = Add(copy _2, copy _3);
        goto -> bb8;
    }

    bb8: {
        goto -> bb3;
    }
}
"
    );
}

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
    let _8: str;
    let _9: Cell;
    let _10: str;
    let _11: Cell;
    let _12: f64;
    let _13: Cell;

    bb0: {
        _6 = Less(copy _1, 0.0_f64);
        branch copy _6 -> [true: bb1, false: bb2];
    }

    bb1: {
        _7 = Bad();
        drop(_5);
        drop(_3);
        drop(_2);
        raise move _7;
    }

    bb2: {
        _8 = \"cell\";
        drop(_2);
        _2 = move _8;
        _9 = Cell(move _2, copy _1);
        drop(_3);
        _3 = move _9;
        _4 = call weigh(_3) -> [return: bb3];
    }

    bb3: {
        _10 = \"other\";
        drop(_2);
        _2 = move _10;
        _11 = call keep(owned move _3) -> [return: bb4];
    }

    bb4: {
        drop(_5);
        _5 = move _11;
        _13 = Cell(\"x\", 2.0_f64);
        _12 = call weigh(_13) -> [return: bb5];
    }

    bb5: {
        drop(_13);
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
    let _4: bool;

    bb0: {
        _2 = copy _1;
        goto -> bb1;
    }

    bb1: {
        _3 = Greater(copy _2, 0_i64);
        branch copy _3 -> [true: bb2, false: bb3];
    }

    bb2: {
        _2 = Subtract(copy _2, 1_i64);
        _4 = Equal(copy _2, 100_i64);
        branch copy _4 -> [true: bb4, false: bb5];
    }

    bb3: {
        _0 = None;
        return;
    }

    bb4: {
        goto -> bb1;
    }

    bb5: {
        goto -> bb1;
    }
}
"
    );
}

/// A construct outside the first slice is an error that names it, and gives no body.
#[test]
fn constructs_not_lowered_yet_are_errors() {
    let program = "\
        def power(x: f64) -> f64:\n\
        \x20   ys = [x]\n\
        \x20   return x ** 2.0\n";
    assert_eq!(
        lower(program, "power"),
        "\
error: lowering does not handle list displays yet
 --> main.npl:2:10
  |
2 |     ys = [x]
  |          ^^^ not lowered yet
  = note: the compiler lowers more of the language as roadmap item 1.9.3 continues

error: lowering does not handle `**` yet
 --> main.npl:3:12
  |
3 |     return x ** 2.0
  |            ^^^^^^^^ not lowered yet
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
  --> main.npl:19:14
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
  --> main.npl:10:12
   |
10 |     return x // 2
   |            ^^^^^^ not lowered yet
   = note: the compiler lowers more of the language as roadmap item 1.9.3 continues"
    );
}
