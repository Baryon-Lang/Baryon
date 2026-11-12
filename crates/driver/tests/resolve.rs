//! Name resolution, end to end: packages of small programs go through the parser, the syntax
//! check and the resolver, and the rendered diagnostics are compared in full.
//!
//! These tests live in the driver because they need two implementations, the parser and the
//! resolver, and only an application crate may depend on both (decision 0001).

use driver::Session;

/// Checks a package of `(path, text)` files and returns every diagnostic, rendered and joined
/// by blank lines. An empty string means the package is clean. A file that cannot be added
/// returns the reason instead, which fails the test's comparison.
fn check(files: &[(&str, &str)]) -> String {
    let mut session = Session::new();
    for (path, text) in files {
        if let Err(error) = session.add_file(*path, *text) {
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

/// Checks a package of one file, `main.byn`.
fn check_one(text: &str) -> String {
    check(&[("main.byn", text)])
}

#[test]
fn locals_parameters_and_module_names_resolve() {
    let program = r#"
from math import sqrt

SCALE: Final = 2.0
Vec3 = Fixed[f64, (3,)]


class Particle(Struct):
    mass: f64
    position: Vec3

    def energy(self, speed: f64) -> f64:
        return 0.5 * self.mass * speed**2


def norm(v: Vec3) -> f64:
    total = 0.0
    for i in range(3):
        total += v[i] * v[i]
    return sqrt(total) * SCALE


def classify(value: f64) -> str:
    if value < 0.0:
        label = "negative"
    else:
        label = "positive"
    squares = [k * k for k in range(4) if k != value]
    match len(squares):
        case 0:
            other = "none"
        case count:
            other = str(count)
    return label + other


def main():
    with unsafe:
        print(norm(Vec3(1.0, 2.0, 3.0)), classify(-1.0))
"#;
    assert_eq!(check_one(program), "");
}

#[test]
fn an_undefined_name_is_reported_where_it_is_read() {
    let program = "def area(radius: f64) -> f64:\n    return PI * radius**2\n";
    assert_eq!(
        check_one(program),
        "error: undefined name `PI`\n --> main.byn:2:12\n  |\n2 |     return PI * radius**2\n  |            ^^ not defined in this scope"
    );
}

#[test]
fn sorted_and_reversed_are_built_in_names() {
    let program = "def main():\n    xs = [3, 1, 2]\n    ys = sorted(xs)\n    for x in reversed(xs):\n        print(x)\n";
    assert_eq!(check_one(program), "");
}

#[test]
fn a_method_does_not_see_its_class_body() {
    let program = "class Box(Struct):\n    size: f64\n\n    def volume(self) -> f64:\n        return size**3\n";
    assert_eq!(
        check_one(program),
        "error: undefined name `size`\n --> main.byn:5:16\n  |\n5 |         return size**3\n  |                ^^^^ not defined in this scope"
    );
}

#[test]
fn a_comprehension_variable_stays_in_the_comprehension() {
    let program = "def squares() -> int:\n    values = [k * k for k in range(4)]\n    return k\n";
    assert_eq!(
        check_one(program),
        "error: undefined name `k`\n --> main.byn:3:12\n  |\n3 |     return k\n  |            ^ not defined in this scope"
    );
}

#[test]
fn names_resolve_across_modules() {
    let nbody = "G: Final = 6.674e-11\n\n\ndef step(dt: f64) -> f64:\n    return dt * G\n";
    let main = "import physics.nbody\nimport physics.nbody as nb\nfrom physics.nbody import step, G\nfrom physics import nbody\n\n\ndef main():\n    print(physics.nbody.step(0.1), nb.G, step(G), nbody.step(1.0))\n";
    assert_eq!(
        check(&[("physics/nbody.byn", nbody), ("main.byn", main)]),
        ""
    );
}

/// A name whose import failed is reported once, at the import, not again at each use.
#[test]
fn a_missing_module_or_member_is_reported() {
    let nbody =
        "def step(dt: f64) -> f64:\n    return dt\n\n\ndef _kick() -> f64:\n    return 1.0\n";
    let main = "import physics.nbdy\nfrom physics.nbody import stepp\nfrom physics.nbody import _kick\n\n\ndef main():\n    print(physics.nbody.leap(1.0))\n";
    assert_eq!(
        check(&[("physics/nbody.byn", nbody), ("main.byn", main)]),
        [
            "error: no module named `physics.nbdy`\n --> main.byn:1:8\n  |\n1 | import physics.nbdy\n  |        ^^^^^^^^^^^^ no such module",
            "error: module `physics.nbody` has no `stepp`\n --> main.byn:2:27\n  |\n2 | from physics.nbody import stepp\n  |                           ^^^^^ not defined there",
            "error: `_kick` is private to module `physics.nbody`\n --> main.byn:3:27\n  |\n3 | from physics.nbody import _kick\n  |                           ^^^^^ private name\n  = note: a name starting with `_` can only be used in its own module",
        ]
        .join("\n\n")
    );
}

#[test]
fn a_member_read_through_a_module_is_checked() {
    let nbody = "def step(dt: f64) -> f64:\n    return dt\n";
    let main = "import physics.nbody as nb\n\n\ndef main():\n    print(nb.leap(1.0))\n";
    assert_eq!(
        check(&[("physics/nbody.byn", nbody), ("main.byn", main)]),
        "error: module `physics.nbody` has no `leap`\n --> main.byn:5:14\n  |\n5 |     print(nb.leap(1.0))\n  |              ^^^^ not defined there"
    );
}

#[test]
fn a_name_imported_twice_over_resolves_to_its_definition() {
    let constants = "C: Final = 299792458.0\n";
    let units = "from constants import C\n";
    let main = "from units import C\n\n\ndef main():\n    print(C)\n";
    assert_eq!(
        check(&[
            ("constants.byn", constants),
            ("units.byn", units),
            ("main.byn", main)
        ]),
        ""
    );
}

#[test]
fn imports_in_a_circle_are_reported() {
    let first = "from second import x\n";
    let second = "from first import x\n";
    let rendered = check(&[("first.byn", first), ("second.byn", second)]);
    assert!(
        rendered.contains("error: module `second` imports `x`, but that import fails"),
        "{rendered}"
    );
}

#[test]
fn standard_library_modules_are_accepted_by_name() {
    let nbody = "def step() -> f64:\n    return 1.0\n";
    let forces = "def total() -> f64:\n    return 1.0\n";
    let program = "import math\nimport random\nfrom math import sqrt\nimport math.special\nimport physics.nbody\nimport physics.forces\n\n\ndef main():\n    print(math.pi, sqrt(2.0), random.Philox, physics.forces.total())\n";
    assert_eq!(
        check(&[
            ("physics/nbody.byn", nbody),
            ("physics/forces.byn", forces),
            ("main.byn", program)
        ]),
        "error: the standard library module `math` has no submodules\n --> main.byn:4:8\n  |\n4 | import math.special\n  |        ^^^^^^^^^^^^ no such module"
    );
}

/// Python 3.14's `compression.zstd` is a module inside a package; `from compression.zstd import
/// x` and `from compression import zstd` both find it (decision 0049).
#[test]
fn a_standard_module_inside_a_package_is_found() {
    let program = "from compression import zstd\nfrom compression.zstd import decompress\n\n\ndef main() -> Raises[None, OSError]:\n    print(len(zstd.decompress(Array.zeros(0, u8))), len(decompress(Array.zeros(0, u8))))\n";
    assert_eq!(check_one(program), "");
}

#[test]
fn a_relative_import_is_reported() {
    let program = "from . import helpers\n";
    assert_eq!(
        check_one(program),
        "error: relative imports are not supported\n --> main.byn:1:15\n  |\n1 | from . import helpers\n  |               ^^^^^^^ imported relative to this module\n  = note: write the module's full name from the package root, such as `from physics.nbody import step`"
    );
}

#[test]
fn a_top_level_name_defined_twice_is_reported() {
    let program = "def step() -> f64:\n    return 1.0\n\n\ndef step() -> f64:\n    return 2.0\n";
    assert_eq!(
        check_one(program),
        "error: `step` is defined twice in this module\n --> main.byn:5:5\n  |\n5 | def step() -> f64:\n  |     ^^^^ defined again here\n ::: main.byn:1:5\n  |\n1 | def step() -> f64:\n  |     ---- first defined here\n  = note: one name means one thing in a module; rename one of them"
    );
}

#[test]
fn type_parameters_are_visible_in_the_signature_and_body() {
    let program =
        "def first[T: Copy](values: list[T]) -> T:\n    result: T = values[0]\n    return result\n";
    assert_eq!(check_one(program), "");
}

#[test]
fn a_file_with_syntax_errors_reports_only_those() {
    let program = "def main(:\n    print(missing)\n";
    let rendered = check_one(program);
    assert!(!rendered.contains("undefined name"), "{rendered}");
}

#[test]
fn every_example_program_resolves() {
    let directory = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/examples");
    let mut session = Session::new();
    let mut count = 0;
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|extension| extension == "byn") {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            session
                .add_file(name, std::fs::read_to_string(&path).unwrap())
                .unwrap();
            count += 1;
        }
    }
    assert!(count >= 5);
    let rendered: Vec<String> = session
        .check()
        .iter()
        .map(|diagnostic| diagnostics::render(diagnostic, &session).to_string())
        .collect();
    assert!(rendered.is_empty(), "{}", rendered.join("\n\n"));
}
