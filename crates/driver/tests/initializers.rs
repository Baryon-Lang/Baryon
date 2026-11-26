//! Checking `__init__` on structs (decision 0068, roadmap 1.7.17), end to end: programs whose
//! `__init__` misses a field, uses `self` as a whole, is called by name or has another shape go
//! through the parser, the syntax check, the resolver and the checker, and the rendered
//! diagnostics are compared in full.

use driver::Session;

/// Checks a package of one file, `main.byn`, and returns every diagnostic, rendered and joined
/// by blank lines. The program's first newline, which only lets a test start it on its own
/// line, is dropped.
fn check_one(text: &str) -> String {
    let mut session = Session::new();
    if let Err(error) = session.add_file("main.byn", text.strip_prefix('\n').unwrap_or(text)) {
        return error.to_string();
    }
    session
        .check()
        .iter()
        .map(|diagnostic| diagnostics::render(diagnostic, &session).to_string())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// A field read before it is set, and a `return` that leaves two fields unset, are
/// each reported where the path reaches them, once per field.
#[test]
fn every_field_is_set_on_every_path() {
    let program = r#"
class Line(Struct):
    slope: f64
    offset: f64
    name: str

    def __init__(self, x: f64, flag: bool):
        if flag:
            self.slope = x
        self.offset = self.slope
        if x > 2.0:
            return
        self.name = "line"


def main():
    line = Line(1.0, True)
    print(line.offset)
"#;
    assert_eq!(
        check_one(program),
        r"error: `self.slope` is read before `__init__` sets it
 --> main.byn:9:23
  |
9 |         self.offset = self.slope
  |                       ^^^^^^^^^^ not set on every path to here
  = note: set the field first; until every field is set, `self` is not a whole struct

error: `__init__` can end without setting `slope`
  --> main.byn:11:13
   |
11 |             return
   |             ^^^^^^ a path ends here
   = note: set every field on every path through `__init__`

error: `__init__` can end without setting `name`
  --> main.byn:11:13
   |
11 |             return
   |             ^^^^^^ a path ends here
   = note: set every field on every path through `__init__`"
    );
}

/// Using `self` as a whole, moving a field back out, and naming a field the class does
/// not declare are errors inside `__init__`, and a falling-through end that misses a field is
/// reported at the method's name.
#[test]
fn self_is_used_only_through_its_fields() {
    let program = r#"
class Track(Struct):
    name: str
    points: list[f64]

    def __init__(self, name: str):
        self.name = name.copy()
        print(self)
        self.points = []
        self.points.append(self.length())
        kept = self.points
        self.speed = 1.0

    def length(self) -> f64:
        return 0.0


class Half(Struct):
    a: f64
    b: f64

    def __init__(self, a: f64):
        self.a = a


def main():
    track = Track("north")
    print(track.name, Half(1.0).a)
"#;
    assert_eq!(
        check_one(program),
        r"error: `self` is used only through its fields in `__init__`
 --> main.byn:7:15
  |
7 |         print(self)
  |               ^^^^ the struct is not built yet
  = note: compute what a method would give from the arguments first, then set the field (decision 0068)

error: `self` is used only through its fields in `__init__`
 --> main.byn:9:28
  |
9 |         self.points.append(self.length())
  |                            ^^^^ the struct is not built yet
  = note: compute what a method would give from the arguments first, then set the field (decision 0068)

error: a field of `self` cannot be moved out in `__init__`
  --> main.byn:10:16
   |
10 |         kept = self.points
   |                ^^^^^^^^^^^ moved here
   = note: keep a copy instead: `self.field.copy()`

error: `Track` has no field `speed`
  --> main.byn:11:14
   |
11 |         self.speed = 1.0
   |              ^^^^^ not a field
   = note: `__init__` sets the fields the class declares, and uses `self` only through them

error: `__init__` can end without setting `b`
  --> main.byn:21:9
   |
21 |     def __init__(self, a: f64):
   |         ^^^^^^^^ a path ends here
   = note: set every field on every path through `__init__`"
    );
}

/// Calling `__init__` by name is an error, and a call of the class is checked against
/// `__init__`'s parameters, not the fields.
#[test]
fn init_is_called_only_through_the_class() {
    let program = r"
class Grid(Struct):
    n: int
    step: f64

    def __init__(self, n: int, length: f64 = 1.0):
        self.n = n
        self.step = length / f64(n)


def main():
    grid = Grid(4)
    grid.__init__(8)
    other = Grid(4, 0.5, 2.0)
    print(grid.step, other.n)
";
    assert_eq!(
        check_one(program),
        r"error: `__init__` runs when the class is called, never by name
  --> main.byn:12:5
   |
12 |     grid.__init__(8)
   |     ^^^^^^^^^^^^^ called here
   = note: call the class instead: `CubicSpline(x, y)` (decision 0068)

error: `Grid` takes 2 arguments, found 3
  --> main.byn:13:13
   |
13 |     other = Grid(4, 0.5, 2.0)
   |             ^^^^^^^^^^^^^^^^^ too many arguments"
    );
}

/// An `__init__` that declares a return type, returns a value or has no `self` is reported
/// where it is written.
#[test]
fn init_has_one_shape() {
    let program = r"
class Returns(Struct):
    x: f64

    def __init__(self, x: f64) -> f64:
        self.x = x


class Gives(Struct):
    x: f64

    def __init__(self, x: f64):
        self.x = x
        return self


class Plain(Struct):
    x: f64

    def __init__(x: f64):
        print(x)


def main():
    print(Returns(1.0).x, Gives(2.0).x)
";
    assert_eq!(
        check_one(program),
        r"error: `__init__` returns nothing
 --> main.byn:4:35
  |
4 |     def __init__(self, x: f64) -> f64:
  |                                   ^^^ a return type
  = note: calling the class gives the struct `__init__` builds; remove the return type, and let a bad argument panic (decision 0068)

error: `__init__` returns nothing
  --> main.byn:13:16
   |
13 |         return self
   |                ^^^^ a value
   = note: calling the class gives the struct; write `return` alone

error: `__init__` takes `self` first, with no type
  --> main.byn:19:9
   |
19 |     def __init__(x: f64):
   |         ^^^^^^^^ no `self`
   = note: write `def __init__(self, x: f64):`"
    );
}

/// A generic struct's `__init__` finds the class's type arguments from its arguments, as the
/// constructor does. One no argument fixes is reported at the call, and is fine once written
/// on the class; a written one is checked against its parameter's bound.
#[test]
fn generic_init_finds_the_class_type_arguments() {
    let program = r"
class Box[T: Number](Struct):
    item: T

    def __init__(self, item: T):
        self.item = item


class Holder[T](Struct):
    items: list[T]

    def __init__(self):
        self.items = []


def main():
    print(Box(1.5).item, Holder[f64]().items)
    empty = Holder()
    text = Box[str]('a')
";
    assert_eq!(
        check_one(program),
        r"error: `Holder` does not say what `T` is from its arguments
  --> main.byn:17:13
   |
17 |     empty = Holder()
   |             ^^^^^^^^ type argument not found
   = note: write it on the class, for example `Holder[f64](...)`

error: `Box` needs `T` to be a `Number`, found `str`
  --> main.byn:18:12
   |
18 |     text = Box[str]('a')
   |            ^^^^^^^^ bound not satisfied"
    );
}
