//! `__init__` on structs built and run (decision 0068, roadmap 1.7.17): a call of the class
//! runs `__init__`, which sets each field through `self`, on every path, and the struct comes
//! back whole. The ownership sweep's `initialized` site runs the same build under Valgrind for
//! every kind of value.

use super::examples::prints;

/// Fields set on two paths, one of which returns early, an element of an array field set in
/// place, a field read after it is set, and methods called on the struct once it is built.
const LINE: &str = "\
class Line(Struct):
    slope: f64
    offset: f64
    points: Array[f64, 1]
    side: str

    def __init__(self, x0: f64, y0: f64, x1: f64, y1: f64):
        self.slope = (y1 - y0) / (x1 - x0)
        self.offset = y0 - self.slope * x0
        self.points = Array.zeros(3)
        self.points[0] = x0
        self.points[1] = x1
        self.points[2] = self.slope
        if x0 < 0.0:
            self.side = \"left\"
            return
        self.side = \"right\"

    def at(self, x: f64) -> f64:
        return self.slope * x + self.offset


def main():
    line = Line(0.0, 1.0, 2.0, 5.0)
    print(line.slope, line.offset, line.at(3.0), line.points.sum(), line.side)
    other = Line(-1.0, 0.0, 1.0, 1.0)
    print(other.side, other.at(0.0))
";

/// A `Copy` struct, a field set twice (the first value is dropped), a struct field changed
/// after it is set, a loop over a field, structs built in a loop and kept in a list, and
/// defaults, a literal and a computed one, left out by position and by keyword.
const SHAPES: &str = "\
class Point(Struct, Copy):
    x: f64
    y: f64

    def __init__(self, r: f64):
        self.x = r
        self.y = 2.0 * r


class Inner(Struct):
    values: list[f64]


class Outer(Struct):
    inner: Inner
    label: str
    count: int

    def __init__(self):
        self.label = \"first\"
        self.label = \"second\"
        self.inner = Inner([1.0, 2.0])
        self.inner.values.append(3.0)
        self.count = 0
        for v in self.inner.values:
            self.count += 1


class Grid(Struct):
    n: int
    step: f64
    values: Array[f64, 1]

    def __init__(self, n: int, length: f64 = 2.0, fill: f64 = 2.0 * 3.5):
        self.n = n
        self.step = length / f64(n)
        self.values = Array.fill(n, fill)


def build(n: int) -> list[Outer]:
    out: list[Outer] = []
    for i in range(n):
        out.append(Outer())
    return out


def main():
    p = Point(1.5)
    q = p
    print(p.x, p.y, q.y)
    many = build(3)
    print(len(many), many[2].label, many[1].count, many[0].inner.values)
    g = Grid(4)
    h = Grid(5, fill=3.0)
    print(g.step, g.values.sum(), h.step, h.values.sum())
";

/// A struct with both `__init__` and `__del__` runs `__del__` once, for the struct
/// `__init__` built, and never for a part of it.
const LOGGED: &str = "\
class Log(Struct):
    name: str
    lines: list[str]

    def __init__(self, name: Owned[str]):
        self.name = name
        self.lines = [\"opened\"]

    def __del__(self: InOut[Self]):
        print(f\"del {self.name} {len(self.lines)}\")


def main():
    log = Log(\"run\")
    log.lines.append(\"step\")
    print(\"end of main\")
";

/// Generic structs built by their `__init__`: the class's type arguments found from the
/// arguments, written on the class, found inside another generic function, and joined by
/// `__init__`'s own type parameter.
const GENERIC: &str = "\
class Grid[T: Number](Struct):
    values: list[T]
    total: T
    count: int

    def __init__(self, values: list[T], scale: T):
        self.values = []
        self.total = T(0)
        for v in values:
            self.values.append(v * scale)
            self.total = self.total + v * scale
        self.count = len(values)

    def doubled(self) -> T:
        return self.total * T(2)


class Tagged[T](Struct):
    tag: str
    item: T

    def __init__(self, item: Owned[T]):
        self.tag = \"t\"
        self.item = item


class Sum[T: Number](Struct):
    total: T

    def __init__[U: Number](self, xs: list[U], zero: T):
        self.total = zero
        for x in xs:
            self.total = self.total + T(x)


def make[U: Number](xs: list[U]) -> Grid[U]:
    grid = Grid(xs, U(1))
    return grid


def main():
    g = Grid([1.0, 2.0, 3.0], 1.0)
    print(g.total, g.count, g.doubled())
    h = Grid([1, 2, 3], 10)
    print(h.total, h.values)
    w = Grid[f64]([], 2)
    print(w.count, w.values)
    m = make([4, 5])
    print(m.total)
    t = Tagged([\"a\", \"b\"])
    k = Tagged[list[str]]([])
    print(t.tag, t.item, len(k.item))
    s = Sum([1, 2, 3], 0.5)
    r = Sum[i64]([1.5], 0)
    print(s.total, r.total)
";

/// [`LINE`] builds and prints what Python prints for the same class.
#[test]
fn init_sets_every_field_on_every_path() {
    prints("init-line", LINE, "2.0 1.0 7.0 4.0 right\nleft 0.5\n").unwrap();
}

/// [`SHAPES`] builds and prints what Python prints for the same classes.
#[test]
fn init_builds_copy_nested_and_defaulted_structs() {
    let expected = "1.5 3.0 3.0\n3 second 3 [1.0, 2.0, 3.0]\n0.5 28.0 0.4 15.0\n";
    prints("init-shapes", SHAPES, expected).unwrap();
}

/// [`GENERIC`] builds and prints what Python prints for the same classes.
#[test]
fn init_builds_generic_structs() {
    let expected = "6.0 3 12.0\n60 [10, 20, 30]\n0 []\n9\nt ['a', 'b'] 0\n6.5 1\n";
    prints("init-generic", GENERIC, expected).unwrap();
}

/// [`LOGGED`] prints `__del__`'s line once, after `main`'s own.
#[test]
fn init_and_del_run_once_each() {
    prints("init-logged", LOGGED, "end of main\ndel run 2\n").unwrap();
}

/// Hooke's law from six measured extensions: the least-squares line `LineFit` builds in its
/// `__init__`. `scipy.stats.linregress`, given the same points, prints the same numbers.
#[test]
fn spring_fit_builds_and_runs() {
    prints(
        "example-spring-fit",
        include_str!("../../../../docs/examples/spring_fit.byn"),
        "6 points, R^2 = 0.999925\n\
         spring constant k = 24.42 N/m\n\
         force at 15 cm: 3.670 N\n",
    )
    .unwrap();
}
