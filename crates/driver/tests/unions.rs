//! Unions of structs, end to end through the type checker (decision 0032): `A | B` annotations
//! and aliases, a member accepted where its union is expected, class patterns in `match`, and
//! a case for every member.
//!
//! These tests live in the driver because they need several implementations, and only an
//! application crate may depend on more than one (decision 0001).

use driver::Session;

/// Checks `main.byn` and returns every diagnostic, rendered and joined by blank lines. An empty
/// string means the program is clean. The program's first newline is dropped.
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

/// The boundary conditions of decision 0032, shared by the tests below.
const BOUNDARY: &str = r"
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
";

/// Decision 0032's worked example: members passed where the union is expected, and a `match`
/// that takes each member apart by keyword, by position and with no fields.
#[test]
fn a_union_of_structs_checks_clean() {
    let program = format!(
        "{BOUNDARY}{}",
        r"

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


def main() -> None:
    left: Boundary = Dirichlet(300.0)
    right = Neumann(0.0)
    print(ghost_value(left, 1.0, 0.1), ghost_value(right, 1.0, 0.1))
    print(ghost_value(Periodic(), 1.0, 0.1))
"
    );
    assert_eq!(check_one(&program), "");
}

/// Order and repeats do not matter, aliases flatten, a union with `None` and an enum member
/// takes `case None:` and the enum's members, and a wildcard covers the rest.
#[test]
fn unions_flatten_and_match_enums_and_none() {
    let program = r"
class Side(Enum):
    LEFT = 0
    RIGHT = 1


class Wall(Struct, Copy):
    side: Side


Hit = Wall | Side
Event = Side | Hit | None | Wall


def describe(event: Event) -> i64:
    match event:
        case None:
            return 0
        case Side.LEFT | Side.RIGHT:
            return 1
        case Wall(side=Side.LEFT):
            return 2
        case _:
            return 3


def main() -> None:
    hit: Wall | Side | None = Wall(Side.RIGHT)
    print(describe(hit), describe(None), describe(Side.LEFT))
";
    assert_eq!(check_one(program), "");
}

/// A missing member is named, as is a value of a member a case takes only partly apart.
#[test]
fn a_match_must_handle_every_member() {
    let program = format!(
        "{BOUNDARY}{}",
        r"

def flux(boundary: Boundary) -> f64:
    match boundary:
        case Periodic() | Dirichlet():
            return 0.0
        case Neumann(flux):
            return flux
    return 0.0


def flag(choice: Periodic | Dirichlet | bool | None) -> bool:
    return True
"
    );
    assert_eq!(
        check_one(&program),
        r"error: this `match` does not handle `Robin()`
  --> main.byn:23:11
   |
23 |     match boundary:
   |           ^^^^^^^^ not every value is handled
   = note: add `case Robin():`, or `case _:` for every value not matched yet; a `case` with an `if` guard may not match, so it does not count

error: `bool` cannot be a member of a union
  --> main.byn:31:41
   |
31 | def flag(choice: Periodic | Dirichlet | bool | None) -> bool:
   |                                         ^^^^ not a struct, enum or error type
   = note: a union's members are structs, enums, error types and `None`; numbers, strings, arrays and type parameters are not supported in version 0 (decision 0032)"
    );
}

/// Class patterns that name the wrong class or fields, and a union used as one of its members.
#[test]
fn class_pattern_mistakes_are_reported() {
    let program = format!(
        "{BOUNDARY}{}",
        r"

class Side(Enum):
    LEFT = 0


def value(boundary: Boundary, wall: Dirichlet) -> f64:
    match wall:
        case Neumann(flux):
            return flux
        case Dirichlet(1.0, 2.0):
            return 1.0
        case Dirichlet(value=v, value=w):
            return v
        case Dirichlet(temperature=t):
            return t
        case Side(x):
            return 0.0
    match boundary:
        case Side.LEFT:
            return 0.0
        case _:
            return 1.0


def main() -> None:
    plain: Dirichlet = Periodic()
"
    );
    assert_eq!(
        check_one(&program),
        r"error: a `Neumann` pattern cannot match a value of type `Dirichlet`
  --> main.byn:28:14
   |
28 |         case Neumann(flux):
   |              ^^^^^^^^^^^^^ does not match the subject

error: `Dirichlet` has 1 field(s), so a class pattern takes at most 1 by position
  --> main.byn:30:29
   |
30 |         case Dirichlet(1.0, 2.0):
   |                             ^^^ no field at this position

error: the field `value` is matched twice
  --> main.byn:32:33
   |
32 |         case Dirichlet(value=v, value=w):
   |                                 ^^^^^^^ matched again here
   = note: each field is matched once, by position or by keyword

error: `Dirichlet` has no field `temperature`
  --> main.byn:34:24
   |
34 |         case Dirichlet(temperature=t):
   |                        ^^^^^^^^^^^ not a field of this struct

error: `Side` is not a struct
  --> main.byn:36:14
   |
36 |         case Side(x):
   |              ^^^^ expected a struct
   = note: a class pattern names a struct or an error type, such as `case Dirichlet(value=v):`

error: a pattern of type `Side` cannot match a value of type `Dirichlet | Neumann | Periodic | Robin`
  --> main.byn:39:14
   |
39 |         case Side.LEFT:
   |              ^^^^^^^^^ does not match the subject

error: expected `Dirichlet`, found `Periodic`
  --> main.byn:46:24
   |
46 |     plain: Dirichlet = Periodic()
   |                        ^^^^^^^^^^ the annotation says `Dirichlet`"
    );
}

/// An alias of several error types lists them in `Raises`, as writing them out does.
#[test]
fn an_alias_of_error_types_lists_them_in_raises() {
    let program = r"
class ParseError(Exception):
    line: i64


class ReadError(Exception):
    code: i64


ReadErrors = ParseError | ReadError


def parse(n: i64) -> Raises[i64, ReadErrors]:
    if n < 0:
        raise ParseError(n)
    if n > 9:
        raise ReadError(n)
    return n


def main() -> None:
    try:
        print(parse(3))
    except ParseError as parse_error:
        print(parse_error.line)
    except ReadError as read_error:
        print(read_error.code)
";
    assert_eq!(check_one(program), "");
}

/// A method called on a union must be defined by every member, the same way in each.
#[test]
fn a_method_on_a_union_must_be_shared() {
    let program = r"
class Atom(Struct, Copy):
    m: f64

    def energy(self) -> f64:
        return self.m

    def mass(self) -> f64:
        return self.m


class Dimer(Struct, Copy):
    m: f64

    def energy(self) -> f32:
        return f32(self.m)


Molecule = Atom | Dimer


def main() -> None:
    molecule: Molecule = Atom(1.0)
    print(molecule.energy())
    print(molecule.mass())
";
    assert_eq!(
        check_one(program),
        r"error: `energy` differs between `Atom` and `Dimer`
  --> main.byn:23:11
   |
23 |     print(molecule.energy())
   |           ^^^^^^^^^^^^^^^ not the same method on every member
   = note: a method called on a union must have the same parameters, modes, defaults and return type in every member (decision 0032)

error: `Dimer`, a member of `Atom | Dimer`, has no method `mass`
  --> main.byn:24:11
   |
24 |     print(molecule.mass())
   |           ^^^^^^^^^^^^^ not defined on every member
   = note: a method called on a union must be defined by every member (decision 0032); match on the union to call it on some members"
    );
}
