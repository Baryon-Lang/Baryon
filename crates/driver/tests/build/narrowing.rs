//! Narrowing by `is not None` in built programs, where a read of the narrowed name must reach
//! the value inside the optional, whichever place the name stands for; and conditional
//! expressions whose branches are `None` and a value, built where an optional is expected.

use super::{baryon_build, package, stderr};
use std::process::Command;

/// A loop variable that borrows an element of a `list[str | None]` is narrowed by
/// `is not None` as a plain variable is (roadmap 1.9.35): `item.copy()` and `item[:]` read the
/// `str` inside. Before, lowering read the element itself, which the backend could not copy
/// and gave `clang` IR whose types disagreed.
#[test]
fn borrowed_loop_variables_are_narrowed() {
    let program = "\
def present(items: list[str | None]) -> list[str]:
    out: list[str] = []
    for item in items:
        if item is not None:
            out.append(item.copy())
            out.append(item[:1])
    return out


def main():
    items: list[str | None] = [f\"a{1}\", None, f\"b{2}\"]
    for item in items:
        if item is not None:
            kept = item.copy()
            part = item[:]
            print(kept, part, len(item))
        else:
            print(\"none\")
    print(present(items))
";
    let directory = package("narrowed_loop_variables", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert!(ran.status.success(), "{ran:?}");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "a1 a1 2\nnone\nb2 b2 2\n['a1', 'a', 'b2', 'b']\n"
    );
}

/// A conditional expression with `None` in one branch and a value in the other has the
/// optional type the context expects (roadmap 1.9.44): an annotated variable, a return value
/// and an argument, for a number and for a `str`, which moves into the optional. A union of
/// structs takes a conditional of two members the same way. Before, the checker reported
/// "both branches must have one type, found `None` and `f64`".
#[test]
fn conditionals_of_none_and_a_value_are_optional() {
    let program = "\
class Circle(Struct, Copy):
    r: f64


class Square(Struct, Copy):
    s: f64


def pick(c: bool, x: f64) -> f64 | None:
    return None if c else x


def show(v: f64 | None) -> None:
    print(v)


def main():
    x = 2.0
    c = True
    a: f64 | None = None if c else x
    b: f64 | None = x if c else None
    print(a, b, pick(False, 3.0))
    show(None if c else x)
    s = \"text\"
    name: str | None = None if not c else s
    shape: Circle | Square = Circle(1.0) if c else Square(2.0)
    print(name, shape)
";
    let directory = package("optional_conditionals", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert!(ran.status.success(), "{ran:?}");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "None 2.0 3.0\nNone\ntext Circle(r=1.0)\n"
    );
}

/// A `None` passed to a `T | None` parameter tells nothing of `T`, so `T` comes from the other
/// arguments (roadmap 1.9.47): `pick(None, 3)` and `pick(2.5, None)` call the `int` and `f64`
/// instances, and `Slot(None, 2.0)` builds a `Slot[f64]`. A generic struct's optional field
/// takes a value converted into its optional type, `Slot(1.0, 2.0)` and `Named(name)`. Before,
/// the `None` made `T` the type `None` and the checker reported "`b` has type `None | None`
/// here, found `i64`", and `Slot(1.0, 2.0)` gave `clang` IR whose types disagreed.
#[test]
fn none_arguments_leave_type_parameters_to_the_others() {
    let program = "\
class Slot[T](Struct, Copy):
    a: T | None
    b: T


class Named[T](Struct):
    label: T | None


def pick[T: Number](a: T | None, b: T | None) -> T:
    if a is not None:
        return a
    if b is not None:
        return b
    return 0


def main():
    print(pick(None, 3), pick(2.5, None), pick(None, 1.5))
    s = Slot(None, 2.0)
    t = Slot(1.0, 2.0)
    print(s.a, s.b, t.a, t.b)
    name = \"x\"
    named = Named(name)
    print(named.label)
";
    let directory = package("optional_type_arguments", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert!(ran.status.success(), "{ran:?}");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "3 2.5 1.5\nNone 2.0 1.0 2.0\nx\n"
    );
}

/// A type parameter that a `None` argument makes the type `None` gives `T | None` the type
/// `None`, as a union's repeats do not matter (roadmap 1.9.51): `show(None)`, `echo(None)` and
/// `first(None, None)` build and run beside the same functions at `f64`, `int` and `str`.
/// Before, the backend stopped on "does not compile values of type `None` yet".
#[test]
fn a_type_parameter_can_be_none() {
    let program = "\
def show[T](x: T | None) -> str:
    if x is None:
        return \"none\"
    return \"some\"


def echo[T](x: Owned[T | None]) -> T | None:
    print(x)
    return x


def first[T](a: T | None, b: T | None) -> bool:
    match a:
        case None:
            return b is None
        case _:
            return True


def main():
    print(show(None), show(2.0), show(f\"s{1}\"))
    print(echo(None), echo(1), echo(f\"t{2}\"))
    print(first(None, None), first(None, 2))
";
    let directory = package("none_type_arguments", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert!(ran.status.success(), "{ran:?}");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "none some some\nNone\n1\nt2\nNone 1 t2\nTrue False\n"
    );
}

/// A checked `T | None` whose `T` is a frame, a text array or an array with gaps reads the
/// value inside (roadmap 1.9.55). These are stored as tuples of arrays, and the read compared
/// the checker's type with the stored one, so it read the whole optional: the backend stopped
/// on "values of type `tuple[Array[f64, 1], Array[i64, 1]] | None`", or `clang` on a store.
#[test]
fn optional_frames_and_text_arrays_read_their_value_once_checked() {
    let program = "\
class P(Struct):
    x: f64
    n: i64
    name: str


def pick(df: DataFrame[P], take: bool) -> DataFrame[P] | None:
    if take:
        return df.head(1)
    return None


def main():
    df: DataFrame[P] = DataFrame([P(1.0, 2, \"a\"), P(3.0, 4, \"b\")])
    best = pick(df, True)
    if best is not None:
        b = best.copy()
        print(len(b), b.x[0], best.name[0], best.n.sum())
    print(pick(df, False) is None)
    texts: Array[str, 1] | None = Array([\"u\", \"v\"])
    if texts is not None:
        print(texts[1], len(texts))
    gaps: Array[f64 | None, 1] | None = Array([1.0, None])
    if gaps is not None:
        print(gaps[1] is None)
";
    let directory = package("optional_frames", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert!(ran.status.success(), "{ran:?}");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "1 1.0 a 2\nTrue\nv 2\nTrue\n"
    );
}
