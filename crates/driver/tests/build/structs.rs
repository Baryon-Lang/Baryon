//! Structs built and run: arrays of `Copy` structs, constructors with optional fields, and
//! structs called through `__call__`.

use super::examples::prints;
use super::{baryon_build, package, stderr};
use std::process::Command;

/// `Array.zeros(n, dtype=Particle)` zeroes every field, `Fixed` fields included; elements and
/// their fields are then set and read in place. Before this, `zeros` of a struct made an
/// integer 0 of the struct's type, which the backend refused.
#[test]
fn arrays_of_copy_structs_build_and_run() {
    let program = "\
Vec3 = Fixed[f64, (3,)]


class Particle(Struct, Copy):
    position: Vec3
    mass: f64
    count: i32


def main():
    particles = Array.zeros(4, dtype=Particle)
    print(particles[3].mass, particles[3].position[2], particles[3].count)
    particles[1] = Particle(Vec3(1.0, 2.0, 3.0), 4.0, 5)
    particles[2].mass = 6.0
    particles[2].position[0] = 7.0
    print(particles[1].position[1], particles[1].count, particles[2].mass)
    print(particles[2].position[0], particles[0].mass)
";
    let directory = package("structs", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&program).output().unwrap();
    assert_eq!(stderr(&run), "");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "0.0 0.0 0\n2.0 5 6.0\n7.0 0.0\n"
    );
}

/// A constructor's arguments take their fields' types, as a call's arguments take their
/// parameters': a `T` or `None` given for a `T | None` field is wrapped first. Before this, the
/// `f64` went into the field unwrapped, which `clang` rejected, and `None` was not built.
#[test]
fn constructors_wrap_values_for_optional_fields() {
    let program = "\
class Sample(Struct, Copy):
    weight: f64 | None
    count: i64


def main():
    given = Sample(0.5, 2)
    missing = Sample(None, 3)
    narrow: f32 = 1.5
    converted = Sample(f64(narrow), 4)
    print(given.weight, missing.weight, converted.weight)
";
    let directory = package("optional-fields", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&program).output().unwrap();
    assert_eq!(stderr(&run), "");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "0.5 None 1.5\n");
}

/// A method called through its class takes no receiver (roadmap 1.9.28): `Probe.named(...)`
/// is a second constructor with no `self`, and `Probe.label(p)` and `Probe.bump(p, 2.0)` pass
/// the struct as `self` by position, read-only and `InOut`, as Python does. Before, lowering
/// reported every such call as an array method it does not build.
#[test]
fn methods_called_through_their_class_build_and_run() {
    let program = "\
class Probe(Struct):
    name: str
    value: f64

    def named(name: str) -> Probe:
        return Probe(name.copy(), 0.0)

    def label(self) -> str:
        return self.name + \"!\"

    def bump(self: InOut[Probe], by: f64):
        self.value += by


def main():
    p = Probe.named(\"heat\" + \"er\")
    print(p.label(), Probe.label(p))
    Probe.bump(p, 2.0)
    print(p.value)
    xs = [Probe.named(\"a\"), Probe.named(\"b\")]
    print(len(xs), xs[1].name)
";
    let directory = package("class_functions", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "heater! heater!\n2.0\n2 b\n"
    );
}

/// A struct called like a function through its `__call__` method (decision 0058): a generic
/// struct at two float types, a call in a loop, a field's struct called as `SciPy` calls
/// `sol.sol(t)`, `InOut` and `Owned` receivers, and an error raised from the call. Each line
/// is what Python prints for the same program, except the caught error, which prints as Baryon
/// prints an error value.
#[test]
fn structs_with_call_are_called_like_functions() {
    let program = "\
class Line[T: Float](Struct, Copy):
    slope: T
    intercept: T

    def __call__(self, x: T) -> T:
        return self.slope * x + self.intercept


class Table(Struct):
    xs: Array[f64, 1]
    ys: Array[f64, 1]

    def __call__(self, x: f64) -> f64:
        i = 0
        while i < len(self.xs) - 2 and self.xs[i + 1] < x:
            i += 1
        w = (x - self.xs[i]) / (self.xs[i + 1] - self.xs[i])
        return (1.0 - w) * self.ys[i] + w * self.ys[i + 1]


class Result(Struct):
    t: Array[f64, 1]
    sol: Table


class Counter(Struct):
    seen: list[int]

    def __call__(self: InOut[Self], value: int) -> int:
        self.seen.append(value)
        return len(self.seen)


class Bag(Struct):
    items: list[str]

    def __call__(self: Owned[Self], last: str) -> list[str]:
        items = self.items
        items.append(last.copy())
        return items


class Limited(Struct, Copy):
    limit: float

    def __call__(self, x: float) -> Raises[float, ValueError]:
        if x > self.limit:
            raise ValueError(\"over the limit\")
        return x


def main():
    line = Line(2.0, 1.0)
    narrow = Line(f32(0.5), f32(0.25))
    print(line(3.0), narrow(f32(2.0)))
    total = 0.0
    for i in range(4):
        total += line(float(i))
    print(total)
    result = Result(Array([0.0, 1.0]), Table(Array([0.0, 1.0, 2.0]), Array([0.0, 10.0, 40.0])))
    print(result.sol(0.5), result.sol(1.5))
    counter = Counter([])
    counter(7)
    print(counter(8), counter.seen)
    bag = Bag([\"a\"])
    print(bag(\"b\"))
    limited = Limited(1.0)
    try:
        print(limited(0.5))
        print(limited(2.0))
    except ValueError as error:
        print(error)
";
    prints(
        "call-dunder",
        program,
        "7.0 1.25\n16.0\n5.0 25.0\n2 [7, 8]\n['a', 'b']\n0.5\nValueError(message='over the limit')\n",
    )
    .unwrap();
}
