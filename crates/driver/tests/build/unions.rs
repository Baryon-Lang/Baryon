//! Unions of structs built and run (decision 0032): members made into their union, `match`
//! with class patterns picking the member, `print` of a union, and a union whose member owns
//! memory dropped.

use super::{baryon_build, package, stderr};
use std::io;
use std::process::Command;

/// Builds `program` as a package's `main.byn`, runs it, and returns what it printed and wrote
/// to standard error. A failed build returns its errors as what the program printed.
fn run(name: &str, program: &str) -> io::Result<(String, String)> {
    let directory = package(name, &[("main.byn", program)])?;
    let (output, program) = baryon_build(&directory)?;
    if !output.status.success() {
        return Ok((stderr(&output), String::new()));
    }
    let run = Command::new(&program).output()?;
    Ok((
        String::from_utf8_lossy(&run.stdout).into_owned(),
        stderr(&run),
    ))
}

/// Decision 0032's boundary conditions: each kind passed where a `Boundary` is expected and
/// taken apart by keyword, by position and with no fields.
#[test]
fn boundary_conditions_build_and_run() {
    let program = "\
class Periodic(Struct, Copy):
    pass


class Dirichlet(Struct, Copy):
    value: f64


class Neumann(Struct, Copy):
    flux: f64


class Robin(Struct, Copy):
    a: f64
    b: f64
    g: f64


Boundary = Periodic | Dirichlet | Neumann | Robin


def ghost_value(boundary: Boundary, inside: f64, dx: f64) -> f64:
    match boundary:
        case Periodic():
            return inside
        case Dirichlet(value=v):
            return 2.0 * v - inside
        case Neumann(flux):
            return inside + flux * dx
        case Robin(a, b, g):
            return (g * dx + (b - 0.5 * a * dx) * inside) / (b + 0.5 * a * dx)


def main():
    edges: list[Boundary] = [Periodic(), Dirichlet(300.0), Neumann(2.0), Robin(1.0, 2.0, 3.0)]
    for edge in edges:
        print(ghost_value(edge, 1.0, 0.1))
    left: Boundary = Robin(1.0, 2.0, 3.0)
    left = Dirichlet(0.5)
    print(left, ghost_value(left, 1.0, 0.1))
";
    let printed = "1.0\n599.0\n1.2\n1.0975609756097562\nDirichlet(value=0.5) 0.0\n";
    assert_eq!(
        run("unions-boundary", program).unwrap(),
        (printed.to_owned(), String::new())
    );
}

/// A union with an enum and `None` among its members: `case None:`, enum member patterns, and
/// a literal inside a class pattern, each tested in order.
#[test]
fn unions_with_enums_and_none_build_and_run() {
    let program = "\
class Side(Enum):
    LEFT = 0
    RIGHT = 1


class Wall(Struct, Copy):
    position: f64


Event = Side | Wall | None


def code(event: Event) -> i64:
    match event:
        case None:
            return 0
        case Side.LEFT:
            return 1
        case Side.RIGHT:
            return 2
        case Wall(position=0.0):
            return 3
        case Wall():
            return 4


def main():
    print(code(None), code(Side.LEFT), code(Side.RIGHT), code(Wall(0.0)), code(Wall(1.5)))
    print(Wall(1.5), Side.RIGHT)
";
    let printed = "0 1 2 3 4\nWall(position=1.5) Side.RIGHT\n";
    assert_eq!(
        run("unions-events", program).unwrap(),
        (printed.to_owned(), String::new())
    );
}

/// A member holding a `str` makes the union own memory: a `match` reads the other member, and
/// the drop at the end of `main` frees the string through a switch on the tag.
#[test]
fn a_union_that_owns_memory_is_dropped_by_its_member() {
    let program = "\
class Named(Struct):
    name: str


class Plain(Struct, Copy):
    x: f64


Thing = Named | Plain


def size(thing: Thing) -> f64:
    match thing:
        case Plain(x):
            return x
        case _:
            return 0.0


def main():
    named: Thing = Named(\"argon\".copy())
    print(size(named), size(Plain(2.0)))
    print(named)
";
    let printed = "0.0 2.0\nNamed(name='argon')\n";
    assert_eq!(
        run("unions-owned", program).unwrap(),
        (printed.to_owned(), String::new())
    );
}
