//! End-to-end tests of lowering arrays (roadmap 1.9.3): indexing with its bounds checks,
//! `with unsafe:` and `a.unchecked`, lengths and shapes, `Array.zeros` and its kin, `Fixed`
//! arrays, and whole-array arithmetic. Each program is parsed, resolved and checked, then each
//! named function is lowered and compared by its full IR text.

use driver::Session;

/// The IR text of each function in `names`, from `program` after its first newline, joined by
/// blank lines. When the program has errors, or lowering gives some, it returns them instead,
/// rendered, so a failing test shows what went wrong.
fn lower(program: &str, names: &[&str]) -> String {
    let mut session = Session::new();
    let program = program.strip_prefix('\n').unwrap_or(program);
    if let Err(error) = session.add_file("main.npl", program) {
        return error.to_string();
    }
    let mut errors = session.check();
    if errors.is_empty() {
        let mut bodies = Vec::new();
        for name in names {
            match session.mir(name) {
                None => return format!("no function `{name}`"),
                Some(Ok(text)) => bodies.push(text),
                Some(Err(found)) => {
                    errors = found;
                    break;
                }
            }
        }
        if errors.is_empty() {
            return bodies.join("\n");
        }
    }
    errors
        .iter()
        .map(|error| diagnostics::render(error, &session).to_string())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Each index of `a[i, j]` is checked against its own dimension's length before the element is
/// read, and `a[-1]` counts from the end.
#[test]
fn indices_are_checked_against_each_length() {
    let program = r"
def get(a: Array[f64, 2], i: i64, j: i64) -> f64:
    return a[i, j]


def set_last(a: InOut[Array[f64, 1]], x: f64):
    a[-1] = x
";
    assert_eq!(
        lower(program, &["get", "set_last"]),
        r"fn(_1: Array[f64, 2], _2: i64, _3: i64) -> f64 {
    let _0: f64; // return
    let _1: Array[f64, 2]; // parameter a
    let _2: i64; // parameter i
    let _3: i64; // parameter j
    let _4: i64;
    let _5: i64;
    let _6: bool;
    let _7: bool;
    let _8: bool;
    let _9: i64;
    let _10: i64;
    let _11: bool;
    let _12: bool;
    let _13: bool;

    bb0: {
        _4 = len(_1, 0);
        _5 = copy _2;
        _6 = GreaterEqual(copy _5, 0_i64);
        _7 = Less(copy _5, copy _4);
        _8 = BitAnd(copy _6, copy _7);
        assert(copy _8, index in bounds in dimension 0: copy _5 < copy _4) -> bb1;
    }

    bb1: {
        _9 = len(_1, 1);
        _10 = copy _3;
        _11 = GreaterEqual(copy _10, 0_i64);
        _12 = Less(copy _10, copy _9);
        _13 = BitAnd(copy _11, copy _12);
        assert(copy _13, index in bounds in dimension 1: copy _10 < copy _9) -> bb2;
    }

    bb2: {
        _0 = copy _1[_5, _10];
        return;
    }
}

fn(_1: InOut[Array[f64, 1]], _2: f64) -> None {
    let _0: None; // return
    let _1: Array[f64, 1]; // parameter a
    let _2: f64; // parameter x
    let _3: i64;
    let _4: i64;
    let _5: bool;
    let _6: bool;
    let _7: bool;

    bb0: {
        _3 = len(_1, 0);
        _4 = Subtract(copy _3, 1_i64);
        _5 = GreaterEqual(copy _4, 0_i64);
        _6 = Less(copy _4, copy _3);
        _7 = BitAnd(copy _5, copy _6);
        assert(copy _7, index in bounds in dimension 0: copy _4 < copy _3) -> bb1;
    }

    bb1: {
        _1[_4] = copy _2;
        _0 = None;
        return;
    }
}
"
    );
}

/// `a.unchecked[i]` inside `with unsafe:` indexes with no check and no length read.
#[test]
fn unchecked_indices_skip_the_check() {
    let program = r"
def fast(a: Array[f64, 1], i: i64) -> f64:
    with unsafe:
        return a.unchecked[i]
";
    assert_eq!(
        lower(program, &["fast"]),
        r"fn(_1: Array[f64, 1], _2: i64) -> f64 {
    let _0: f64; // return
    let _1: Array[f64, 1]; // parameter a
    let _2: i64; // parameter i
    let _3: i64;

    bb0: {
        _3 = copy _2;
        _0 = copy _1[_3];
        return;
    }
}
"
    );
}

/// `len(a)`, `a.shape[1]` and `a.size` read the lengths; the size is their product.
#[test]
fn lengths_shapes_and_sizes_read_the_array() {
    let program = r"
def sizes(a: Array[f64, 2]) -> i64:
    return len(a) + a.shape[1] + a.size
";
    assert_eq!(
        lower(program, &["sizes"]),
        r"fn(_1: Array[f64, 2]) -> i64 {
    let _0: i64; // return
    let _1: Array[f64, 2]; // parameter a
    let _2: i64;
    let _3: i64;
    let _4: i64;
    let _5: i64;
    let _6: i64;
    let _7: i64;
    let _8: i64;
    let _9: i64;

    bb0: {
        _4 = len(_1, 0);
        _3 = copy _4;
        _6 = len(_1, 1);
        _5 = copy _6;
        _2 = Add(copy _3, copy _5);
        _8 = len(_1, 0);
        _9 = len(_1, 1);
        _7 = copy _8;
        _7 = Multiply(copy _7, copy _9);
        _0 = Add(copy _2, copy _7);
        return;
    }
}
"
    );
}

/// `Array.zeros` allocates the array, then a loop per dimension sets each element, the last
/// dimension innermost; an array of `Fixed` elements fills each with a built value.
#[test]
fn new_arrays_are_allocated_then_filled() {
    let program = r"
Vec3 = Fixed[f64, (3,)]


def make(n: i64) -> Array[f64, 2]:
    return Array.zeros((n, 3))


def cube(n: i64) -> Array[Vec3, 1]:
    return Array.zeros(n, dtype=Vec3)
";
    assert_eq!(
        lower(program, &["make", "cube"]),
        r"fn(_1: i64) -> Array[f64, 2] {
    let _0: Array[f64, 2]; // return
    let _1: i64; // parameter n
    let _2: i64;
    let _3: i64;
    let _4: i64;
    let _5: bool;
    let _6: i64;
    let _7: bool;

    bb0: {
        _2 = copy _1;
        _3 = 3_i64;
        _0 = new Array[f64, 2](copy _2, copy _3);
        _4 = 0_i64;
        goto -> bb1;
    }

    bb1: {
        _5 = Less(copy _4, copy _2);
        branch copy _5 -> [true: bb2, false: bb3];
    }

    bb2: {
        _6 = 0_i64;
        goto -> bb4;
    }

    bb3: {
        return;
    }

    bb4: {
        _7 = Less(copy _6, copy _3);
        branch copy _7 -> [true: bb5, false: bb6];
    }

    bb5: {
        _0[_4, _6] = 0.0_f64;
        _6 = Add(copy _6, 1_i64);
        goto -> bb4;
    }

    bb6: {
        _4 = Add(copy _4, 1_i64);
        goto -> bb1;
    }
}

fn(_1: i64) -> Array[Fixed[f64, (3,)], 1] {
    let _0: Array[Fixed[f64, (3,)], 1]; // return
    let _1: i64; // parameter n
    let _2: i64;
    let _3: Fixed[f64, (3,)];
    let _4: i64;
    let _5: bool;

    bb0: {
        _2 = copy _1;
        _3 = Fixed[f64, (3,)](0.0_f64, 0.0_f64, 0.0_f64);
        _0 = new Array[Fixed[f64, (3,)], 1](copy _2);
        _4 = 0_i64;
        goto -> bb1;
    }

    bb1: {
        _5 = Less(copy _4, copy _2);
        branch copy _5 -> [true: bb2, false: bb3];
    }

    bb2: {
        _0[_4] = copy _3;
        _4 = Add(copy _4, 1_i64);
        goto -> bb1;
    }

    bb3: {
        return;
    }
}
"
    );
}

/// Arithmetic on whole arrays becomes one elementwise expression for array expansion to loop
/// over, and `a += b` replaces `a` with the result.
#[test]
fn whole_array_expressions_stay_whole() {
    let program = r"
def axpy(a: Array[f64, 1], b: Array[f64, 1]) -> Array[f64, 1]:
    c = -a + b * 2
    return c


def accumulate(a: InOut[Array[f64, 1]], b: Array[f64, 1]):
    a += b
";
    assert_eq!(
        lower(program, &["axpy", "accumulate"]),
        r"fn(_1: Array[f64, 1], _2: Array[f64, 1]) -> Array[f64, 1] {
    let _0: Array[f64, 1]; // return
    let _1: Array[f64, 1]; // parameter a
    let _2: Array[f64, 1]; // parameter b
    let _3: Array[f64, 1]; // c
    let _4: Array[f64, 1];

    bb0: {
        _4 = elementwise Add(Negate(copy _1), Multiply(copy _2, 2.0_f64));
        drop(_3);
        _3 = move _4;
        _0 = move _3;
        drop(_3);
        return;
    }
}

fn(_1: InOut[Array[f64, 1]], _2: Array[f64, 1]) -> None {
    let _0: None; // return
    let _1: Array[f64, 1]; // parameter a
    let _2: Array[f64, 1]; // parameter b
    let _3: Array[f64, 1];

    bb0: {
        _3 = elementwise Add(copy _1, copy _2);
        drop(_1);
        _1 = move _3;
        _0 = None;
        return;
    }
}
"
    );
}

/// A `Fixed` array is built from its elements in row-major order, and a constant index into it
/// needs no check, even one that counts from the end.
#[test]
fn fixed_arrays_are_built_from_their_elements() {
    let program = r"
Vec3 = Fixed[f64, (3,)]
Mat2 = Fixed[f64, (2, 2)]


def origin() -> Vec3:
    return Vec3.zeros()


def identity() -> Mat2:
    return Mat2((1, 0), (0, 1))


def ends(v: Vec3) -> f64:
    return v[0] + v[-1]
";
    assert_eq!(
        lower(program, &["origin", "identity", "ends"]),
        r"fn() -> Fixed[f64, (3,)] {
    let _0: Fixed[f64, (3,)]; // return

    bb0: {
        _0 = Fixed[f64, (3,)](0.0_f64, 0.0_f64, 0.0_f64);
        return;
    }
}

fn() -> Fixed[f64, (2, 2)] {
    let _0: Fixed[f64, (2, 2)]; // return

    bb0: {
        _0 = Fixed[f64, (2, 2)](1.0_f64, 0.0_f64, 0.0_f64, 1.0_f64);
        return;
    }
}

fn(_1: Fixed[f64, (3,)]) -> f64 {
    let _0: f64; // return
    let _1: Fixed[f64, (3,)]; // parameter v
    let _2: i64;
    let _3: i64;

    bb0: {
        _2 = 0_i64;
        _3 = 2_i64;
        _0 = Add(copy _1[_2], copy _1[_3]);
        return;
    }
}
"
    );
}
