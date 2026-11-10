//! Traits and embedded structs (decision 0031), end to end through the type checker: traits
//! declared as `Protocol` classes, implemented by naming them as a base, used as bounds, and
//! `Embed[T]` fields whose names the outer struct reaches. Diagnostics are compared in full.
//!
//! These tests live in the driver because they need several implementations, and only an
//! application crate may depend on more than one (decision 0001).

use driver::Session;

/// Checks the files `files`, each a name and its text, and returns every diagnostic, rendered
/// and joined by blank lines. An empty string means the program is clean. Each text's first
/// newline, which only lets a test start it on its own line, is dropped.
fn check(files: &[(&str, &str)]) -> String {
    let mut session = Session::new();
    for (name, text) in files {
        if let Err(error) = session.add_file(*name, text.strip_prefix('\n').unwrap_or(text)) {
            return error.to_string();
        }
    }
    session
        .check()
        .iter()
        .map(|diagnostic| diagnostics::render(diagnostic, &session).to_string())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The traits of `physics.byn`, which the tests import.
const PHYSICS: &str = r#"
class Body(Protocol):
    """A point mass that knows its own energy."""

    def energy(self) -> f64: ...

    def mass(self) -> f64: ...


class Charged(Protocol):
    def charge(self) -> f64: ...


class ChargedBody(Body, Charged, Protocol):
    pass
"#;

/// A trait required by another, a struct implementing one, a struct that embeds it and gets
/// the first trait's methods from it, generic functions bounded by both traits, a trait method
/// whose `self` is `InOut`, and a trait named through its module.
#[test]
fn traits_and_embedding_check_clean() {
    let program = r"
import physics
from physics import Body, ChargedBody


class Heated(Protocol):
    def heat(self: InOut[Self], by: f64): ...


class Particle(Struct, Copy, Body, Heated):
    v: f64
    m: f64

    def energy(self) -> f64:
        return 0.5 * self.m * self.v * self.v

    def mass(self) -> f64:
        return self.m

    def heat(self: InOut[Self], by: f64):
        self.v = self.v + by


class Ion(Struct, Copy, ChargedBody):
    particle: Embed[Particle]
    q: f64

    def charge(self) -> f64:
        return self.q


class Probe(Struct, physics.Charged):
    q: f64

    def charge(self) -> f64:
        return self.q


def total_energy[T: Body](bodies: list[T]) -> f64:
    total = 0.0
    for body in bodies:
        total += body.energy()
    return total


def force[T: ChargedBody](a: T, b: T) -> f64:
    return a.charge() * b.charge() / (a.mass() + b.mass())


def warm[T: Heated](thing: InOut[T]):
    thing.heat(1.0)


def main():
    ion = Ion(Particle(2.0, 3.0), 1.0)
    mass: f64 = ion.m + ion.particle.m
    print(mass, ion.energy(), force(ion, ion))
    print(total_energy([ion, ion]) + total_energy([Particle(1.0, 1.0)]))
    p = Particle(1.0, 1.0)
    warm(p)
    ion.heat(2.0)
";
    assert_eq!(
        check(&[("physics.byn", PHYSICS), ("main.byn", program)]),
        ""
    );
}

/// A trait with a field and a method body, traits that require each other, a struct that
/// names a class that is not a trait, a method with the wrong signature, a missing method, one
/// missing through a module-qualified trait, and an enum naming a trait.
#[test]
fn trait_declaration_mistakes_are_reported() {
    let program = r"
import physics
from physics import Body


class Loud(Protocol):
    volume: f64

    def shout(self) -> f64:
        return 1.0


class Loop(Lop, Protocol):
    pass


class Lop(Loop, Protocol):
    pass


class Rock(Struct):
    m: f64


class Particle(Struct, Copy, Body, Rock):
    m: f64

    def energy(self) -> f32:
        return f32(0.5)


class Probe(Struct, physics.Charged):
    q: f64


class Kind(Enum, Body):
    ONE = 1
";
    let expected = r"
error: a trait has methods only, no fields
 --> main.byn:6:5
  |
6 |     volume: f64
  |     ^^^^^^^^^^^ not a method
  = note: a type that implements the trait declares its own fields; a method such as `def mass(self) -> f64: ...` can give access to one

error: a class cannot inherit from `Rock`
  --> main.byn:24:36
   |
24 | class Particle(Struct, Copy, Body, Rock):
   |                                    ^^^^ not a trait
   = note: there is no class inheritance (decision 0015); to share behavior, declare `class Rock(Protocol)` with the methods, and to share fields, hold a `Rock` as a field or embed it: `base: Embed[Rock]` (decision 0031)

error: an enum cannot implement a trait in version 0
  --> main.byn:35:18
   |
35 | class Kind(Enum, Body):
   |                  ^^^^ a trait
   = note: enums have no methods yet, so they cannot define a trait's methods

error: a trait's method has the body `...`
 --> main.byn:8:9
  |
8 |     def shout(self) -> f64:
  |         ^^^^^ has a body
  = note: a trait lists its methods; each type that implements it defines them: `def shout(self) -> f64: ...`
  = note: default method bodies are not in version 0 (decision 0031, Ideas I.31)

error: `Loop` requires itself
  --> main.byn:12:7
   |
12 | class Loop(Lop, Protocol):
   |       ^^^^ through the traits it requires
   = note: a trait cannot be part of what it requires

error: `Lop` requires itself
  --> main.byn:16:7
   |
16 | class Lop(Loop, Protocol):
   |       ^^^ through the traits it requires
   = note: a trait cannot be part of what it requires

error: `Particle.energy` does not match `energy` in the trait `Body`
  --> main.byn:24:30
   |
24 | class Particle(Struct, Copy, Body, Rock):
   |                              ^^^^ the trait is named here
  ::: main.byn:27:9
   |
27 |     def energy(self) -> f32:
   |         ------ defined here
   = note: `Body` lists `def energy(self) -> f64`
   = note: `Particle` defines `def energy(self) -> f32`

error: `Particle` does not define `mass`, which the trait `Body` lists
  --> main.byn:24:30
   |
24 | class Particle(Struct, Copy, Body, Rock):
   |                              ^^^^ the trait is named here
   = note: add `def mass(self) -> f64` to `Particle`

error: `Probe` does not define `charge`, which the trait `Charged` lists
  --> main.byn:31:21
   |
31 | class Probe(Struct, physics.Charged):
   |                     ^^^^^^^^^^^^^^^ the trait is named here
   = note: add `def charge(self) -> f64` to `Probe`
";
    assert_eq!(
        check(&[("physics.byn", PHYSICS), ("main.byn", program)]),
        expected.strip_prefix('\n').unwrap_or(expected).trim_end()
    );
}

/// Two bounds on one type parameter, a field and an unlisted method on a trait-bounded
/// parameter, a trait and `Dyn` where a type is needed, a trait built like a struct, and a type
/// that does not name the trait its call needs.
#[test]
fn trait_use_mistakes_are_reported() {
    let program = r"
from physics import Body


class Rock(Struct):
    m: f64


def total[T: (Body, Rock)](x: T) -> f64:
    return 0.0


def weigh[T: Body](x: T) -> f64:
    y = x.m
    return x.weight()


def use(b: Body, d: Dyn[Body]) -> f64:
    q = Body()
    return weigh(Rock(1.0))
";
    let expected = r"
error: a type parameter has one bound
 --> main.byn:8:14
  |
8 | def total[T: (Body, Rock)](x: T) -> f64:
  |              ^^^^^^^^^^^^ more than one trait
  = note: declare a trait that requires them all, `class ChargedBody(Body, Charged, Protocol): pass`, and bound by it; in Python `[T: (A, B)]` means `T` is exactly `A` or exactly `B` (decision 0031)

error: `T` has no attribute `m`
  --> main.byn:13:9
   |
13 |     y = x.m
   |         ^^^ not known for a type parameter
   = note: a type parameter has only what its bound allows, and a trait has methods only: call a method such as `body.mass()`

error: `T` has no method `weight`
  --> main.byn:14:12
   |
14 |     return x.weight()
   |            ^^^^^^^^ not in the trait
   = note: `T` has only the methods its bound `Body` and the traits it requires list

error: `Body` is a trait, not a type
  --> main.byn:17:12
   |
17 | def use(b: Body, d: Dyn[Body]) -> f64:
   |            ^^^^ expected a type
   = note: bound a type parameter by it, `def f[T: Body](x: T)`, to accept any type that implements it; a value that may be one of several such types, `Dyn[Body]`, comes with roadmap item 3.5

error: `Dyn` is not available yet
  --> main.byn:17:21
   |
17 | def use(b: Body, d: Dyn[Body]) -> f64:
   |                     ^^^^^^^^^ trait objects are not in version 0
   = note: a value of any type with a trait, `Dyn[Body]`, comes with roadmap item 3.5 (decision 0031); a generic function `def f[T: Body](x: T)` takes one such type at a time, compiled for each

error: `Body` is a trait, which cannot be built
  --> main.byn:18:9
   |
18 |     q = Body()
   |         ^^^^^^ not a struct
   = note: build a struct that implements it instead

error: `weigh` needs `T` to be a `Body`, found `Rock`
  --> main.byn:19:12
   |
19 |     return weigh(Rock(1.0))
   |            ^^^^^^^^^^^^^^^^ bound not satisfied
   = note: a type has a trait only by naming it among its bases: `class Particle(Struct, Body)`
";
    assert_eq!(
        check(&[("physics.byn", PHYSICS), ("main.byn", program)]),
        expected.strip_prefix('\n').unwrap_or(expected).trim_end()
    );
}

/// Two embedded structs that both provide a name, a struct that embeds itself directly and
/// through another, an embedded type that is not a struct, and a promoted method that takes
/// its struct as `Owned`, called directly or through a trait.
#[test]
fn embedding_mistakes_are_reported() {
    let program = r"
class A(Struct):
    m: f64

    def take(self: Owned[Self]) -> f64:
        return self.m


class B(Struct):
    m: f64


class Both(Struct):
    a: Embed[A]
    b: Embed[B]


class Selfish(Struct):
    s: Embed[Selfish]


class Outer(Struct):
    inner: Embed[Inner]


class Inner(Struct):
    outer: Embed[Outer]


class Bad(Struct):
    n: Embed[f64]


class Taken(Protocol):
    def take(self: Owned[Self]) -> f64: ...


class Holder(Struct, Taken):
    a: Embed[A]


def main():
    both = Both(A(1.0), B(2.0))
    print(both.m)
    print(both.take())
";
    let expected = r"
error: `Selfish` cannot embed itself
  --> main.byn:18:14
   |
18 |     s: Embed[Selfish]
   |              ^^^^^^^ the struct being declared
   = note: a struct that held itself would have no end

error: only a struct can be embedded, not `f64`
  --> main.byn:30:14
   |
30 |     n: Embed[f64]
   |              ^^^ not a struct
   = note: hold it as a plain field instead: `name: T`

error: `take` takes its embedded struct as `Owned`
  --> main.byn:44:11
   |
44 |     print(both.take())
   |           ^^^^^^^^^ would move a field out of the struct
   = note: call it on the embedded field by name, `value.field.method()`, which moves the whole struct

error: `Both.m` could mean more than one embedded field
  --> main.byn:12:7
   |
12 | class Both(Struct):
   |       ^^^^
   = note: it is `a.m` or `b.m`
   = note: define `m` on `Both` itself, which hides both, or rename one of them

error: `Outer` embeds itself
  --> main.byn:21:7
   |
21 | class Outer(Struct):
   |       ^^^^^
   = note: through Outer embeds Inner and back: a struct that held itself would have no end

error: `Inner` embeds itself
  --> main.byn:25:7
   |
25 | class Inner(Struct):
   |       ^^^^^
   = note: through Inner embeds Outer and back: a struct that held itself would have no end

error: `Holder` gets `take` from the embedded `A`, which takes it as `Owned`
  --> main.byn:37:22
   |
37 | class Holder(Struct, Taken):
   |                      ^^^^^ the trait is named here
   = note: called through the trait, it would move the `A` out of a `Holder`; define `take` on `Holder` itself
";
    assert_eq!(
        check(&[("main.byn", program)]),
        expected.strip_prefix('\n').unwrap_or(expected).trim_end()
    );
}
