//! User traits and embedded structs (decision 0031) built and run: generic functions bounded by
//! a trait call each type's own method, and an embedded struct's fields and methods are reached
//! through the struct that embeds it.

use super::{baryon_build, package, stderr};
use std::process::Command;

/// The program the test runs: a trait required by another, a struct implementing it, a struct
/// that embeds that one and gets the first trait's methods from it, an `InOut` trait method,
/// and generic functions over all of them.
const PROGRAM: &str = "\
class Body(Protocol):
    def energy(self) -> f64: ...

    def mass(self) -> f64: ...


class Charged(Protocol):
    def charge(self) -> f64: ...


class ChargedBody(Body, Charged, Protocol):
    pass


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


class Ion(Struct, Copy, ChargedBody, Heated):
    particle: Embed[Particle]
    q: f64

    def charge(self) -> f64:
        return self.q


def pair_energy[T: Body](a: T, b: T) -> f64:
    return a.energy() + b.energy()


def force[T: ChargedBody](a: T, b: T) -> f64:
    return a.charge() * b.charge() / (a.mass() + pair_energy(a, b))


def warm[T: Heated](thing: InOut[T]):
    thing.heat(1.0)


def main():
    ion = Ion(Particle(2.0, 3.0), 2.0)
    p = Particle(1.0, 1.0)
    print(ion.m, ion.v, ion.q, ion.energy())
    print(pair_energy(p, p), pair_energy(ion, ion))
    print(force(ion, ion))
    warm(p)
    warm(ion)
    ion.heat(1.0)
    ion.m = 4.0
    print(p.v, ion.particle.v, ion.energy())
";

/// Each instance of a generic function calls the method of the type it is compiled for: an
/// `Ion`'s `energy` and `mass` are its embedded `Particle`'s, an `InOut` method changes the
/// caller's value, and `ion.m` reads and writes the embedded field.
#[test]
fn traits_and_embedded_structs_build_and_run() {
    let directory = package("traits", &[("main.byn", PROGRAM)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&program).output().unwrap();
    assert_eq!(stderr(&run), "");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "3.0 2.0 2.0 6.0\n1.0 12.0\n0.26666666666666666\n2.0 4.0 32.0\n"
    );
}
