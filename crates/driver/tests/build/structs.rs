//! Structs built and run: arrays of `Copy` structs, and constructors with optional fields.

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
