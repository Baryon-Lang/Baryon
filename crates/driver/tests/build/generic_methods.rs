//! Methods of generic structs, `Callable` parameters passed on, and arguments to generic
//! parameters, built and run (roadmap 1.9.18, decision 0026).

use super::{baryon_build, package, stderr};
use std::io;
use std::process::Command;

/// Builds the program `text` as the `main.byn` of the package `name`, runs it, and returns what
/// it printed, after checking that the build succeeded and the program printed nothing to
/// standard error.
///
/// # Errors
///
/// When writing the package or starting `baryon` or the program fails.
fn output(name: &str, text: &str) -> io::Result<String> {
    let directory = package(name, &[("main.byn", text)])?;
    let (built, program) = baryon_build(&directory)?;
    assert!(built.status.success(), "{}", stderr(&built));
    let ran = Command::new(&program).output()?;
    assert_eq!(stderr(&ran), "");
    Ok(String::from_utf8_lossy(&ran.stdout).into_owned())
}

/// Each method of a generic struct is compiled once per type the struct is used at: a method
/// that reads fields of type `T`, one that calls another method on `self`, one that builds a
/// new `Pair[T]`, an `InOut[Self]` method, a method with a type parameter of its own, and a
/// method called from a generic function on a `Pair[T]`. Before, each was reported as "not
/// compiled yet".
#[test]
fn methods_of_generic_structs_build_at_each_type() {
    let program = "\
class Pair[T: Number](Struct, Copy):
    a: T
    b: T

    def total(self) -> T:
        return self.a + self.b

    def doubled(self) -> T:
        return self.total() * 2

    def swapped(self) -> Pair[T]:
        return Pair(self.b, self.a)

    def bump(self: InOut[Self], step: T) -> None:
        self.a = self.a + step

    def first_or[U: Number](self, other: U) -> U:
        return other


class Samples[T: Float](Struct):
    values: Array[T, 1]

    def scale(self: InOut[Self], k: T) -> None:
        for i in range(len(self.values)):
            self.values[i] = self.values[i] * k


def swapped_total[T: Number](pair: Pair[T]) -> T:
    flipped = pair.swapped()
    return flipped.total()


def main():
    p = Pair(1.5, 2.5)
    q = Pair(3, 4)
    q.bump(2)
    print(p.total(), q.total(), q.doubled(), p.first_or(7))
    print(swapped_total(p), swapped_total(q))
    s = Samples(Array.fill(3, 2.0))
    s.scale(1.5)
    print(s.values[2])
";
    assert_eq!(
        output("generic-methods", program).unwrap(),
        "4.0 9 18 7\n4.0 9\n3.0\n"
    );
}

/// A `Callable` parameter passed on to another call, and copied into a local, calls the
/// function the caller received, even when another `Callable` parameter of the same signature
/// received a different one: `both(square, cube, 2.0)` calls `square` through `f` and `k`, and
/// `cube` through `g` and `h`.
#[test]
fn callable_parameters_are_passed_on_and_copied() {
    let program = "\
def square(x: f64) -> f64:
    return x * x


def cube(x: f64) -> f64:
    return x * x * x


def apply(f: Callable[[f64], f64], x: f64) -> f64:
    return f(x)


def twice(f: Callable[[f64], f64], x: f64) -> f64:
    return apply(f, apply(f, x))


def both(f: Callable[[f64], f64], g: Callable[[f64], f64], x: f64) -> f64:
    h = g
    k = f
    return apply(f, x) + 10.0 * apply(g, x) + 100.0 * h(x) + 1000.0 * k(x)


class Pair[T: Float](Struct, Copy):
    a: T
    b: T

    def map(self, f: Callable[[T], T]) -> Pair[T]:
        return Pair(f(self.a), f(self.b))

    def map_twice(self, f: Callable[[T], T]) -> Pair[T]:
        once = self.map(f)
        return once.map(f)


def half(x: f64) -> f64:
    return x / 2.0


def main():
    print(twice(square, 3.0), both(square, cube, 2.0), both(cube, cube, 2.0))
    p = Pair(8.0, 4.0).map_twice(half)
    print(p.a, p.b)
";
    assert_eq!(
        output("callables-passed-on", program).unwrap(),
        "81.0 4884.0 8888.0\n2.0 1.0\n"
    );
}

/// A local that holds one of two different functions, depending on a branch, would need a
/// function value at run time, which version 0 leaves out (decision 0026): the build reports
/// it rather than calling the wrong one.
#[test]
fn a_local_holding_either_of_two_functions_is_reported() {
    let program = "\
def square(x: f64) -> f64:
    return x * x


def cube(x: f64) -> f64:
    return x * x * x


def pick(f: Callable[[f64], f64], g: Callable[[f64], f64], first: bool) -> f64:
    h = g
    if first:
        h = f
    return h(2.0)


def main():
    print(pick(square, cube, True))
";
    let directory = package("callable-either", &[("main.byn", program)]).unwrap();
    let (built, _) = baryon_build(&directory).unwrap();
    assert!(!built.status.success());
    assert_eq!(
        stderr(&built),
        "\
error: the LLVM backend does not compile calls of `Callable` values yet
 --> main.byn:9:5
  |
9 | def pick(f: Callable[[f64], f64], g: Callable[[f64], f64], first: bool) -> f64:
  |     ^^^^ the LLVM backend does not compile calls of `Callable` values yet

"
    );
}

/// An argument is passed at its parameter's type with the call's type arguments in place: an
/// `Owned[T]` argument moves a `list[int]`, an `Array[f64, 1]` or an `int` widened to the `f64`
/// that `T` stands for, an `Owned[T]` method parameter of a generic struct takes the struct's
/// `T`, and a borrowed `T | None` receives an `int | None`. Before, the caller built a temporary
/// of type `T` itself: each `Owned` call failed with "the LLVM backend does not compile values
/// of type `T` yet" after `baryon check` passed, and `present(3)` returned `False`.
#[test]
fn arguments_take_the_instantiated_parameter_type() {
    let program = "\
def first[T](x: Owned[T]) -> T:
    return x


def pick[T](x: Owned[T], y: Owned[T]) -> T:
    return y


def present[T](x: T | None) -> bool:
    return x is not None


class Bag[T](Struct):
    items: list[T]

    def put(self: InOut[Self], item: Owned[T]) -> None:
        self.items.append(item)


def main():
    xs = [1, 2]
    same = first(xs)
    values = first(Array.fill(2, 0.5))
    print(same, values[1], pick(1.5, 2))
    bag = Bag([1])
    bag.put(2)
    print(bag.items, present(3))
";
    assert_eq!(
        output("generic-arguments", program).unwrap(),
        "[1, 2] 0.5 2.0\n[1, 2] True\n"
    );
}

/// A generic struct's methods called through its class (roadmap 1.9.30), built and run: with
/// the type arguments found from the arguments, `Pair.both(1.5)` and `Pair.total(p)`, as the
/// constructor finds them; written on the class, `Pair[i64].both(2)` and `Bag[str].empty()`,
/// which no argument could fix; and a method with type parameters of its own. Each is
/// compiled once per type argument, and the `Bag` of `str` is freed with its list.
#[test]
fn generic_methods_called_through_their_class_build_and_run() {
    let program = "\
class Pair[T: Number](Struct, Copy):
    a: T
    b: T

    def both(x: T) -> Pair[T]:
        return Pair(x, x)

    def total(self) -> T:
        return self.a + self.b

    def first_of[U: Number](self, other: U) -> T:
        return self.a


class Bag[T](Struct):
    items: list[T]

    def of(item: Owned[T]) -> Bag[T]:
        return Bag([item])

    def empty() -> Bag[T]:
        items: list[T] = []
        return Bag(items)


def main():
    p = Pair.both(1.5)
    q = Pair[i64].both(2)
    print(p.a, Pair.total(p), Pair[i64].total(q), Pair.first_of(q, 0.5))
    words = Bag[str].empty()
    words.items.append(\"hi\")
    numbers = Bag.of(3)
    print(words.items, numbers.items)
";
    assert_eq!(
        output("generic-class-functions", program).unwrap(),
        "1.5 3.0 4 2\n['hi'] [3]\n"
    );
}
