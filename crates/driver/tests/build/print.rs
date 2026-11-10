//! `print` of every value built and run: `None`, optional values, tuples, enum members,
//! structs, lists and dicts as Python writes them, and the `sep=` and `end=` keywords (roadmap
//! 1.10.12, 1.13.11).

use super::{baryon_build, package, stderr};
use std::io;
use std::process::Command;

/// Builds `program` as `main.byn` of a package named `name`, runs it, and returns what it
/// printed to standard output.
fn printed(name: &str, program: &str) -> io::Result<String> {
    let directory = package(name, &[("main.byn", program)])?;
    let (output, program) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output()?;
    assert!(ran.status.success(), "{}", stderr(&ran));
    Ok(String::from_utf8_lossy(&ran.stdout).into_owned())
}

/// Each kind of value prints as Python's `print` writes it. Inside a tuple or a struct a `str`
/// is quoted, as Python's `repr` writes it; a generic struct prints without its type arguments.
#[test]
fn every_value_prints_as_python_does() {
    let program = "\
class Phase(Enum):
    SOLID = 1
    LIQUID = 2
    GAS = 3


class Point(Struct, Copy):
    x: f64
    y: f64


class Sample(Struct):
    label: str
    phase: Phase
    at: Point


class Pair[T](Struct, Copy):
    first: T
    second: T


def lookup(found: bool) -> i64 | None:
    if found:
        return 42
    return None


def main():
    print(None)
    print(lookup(True), lookup(False))
    print((1, 2.5), (True,), ())
    print((\"heat\", (Phase.GAS, 2)))
    print(Phase.SOLID, Phase.LIQUID, Phase.GAS)
    print(Point(1.0, -2.5))
    print(Sample(\"it's\", Phase.LIQUID, Point(0.0, 1e16)))
    print(Sample(\"run\", Phase.GAS, Point(3.0, 4.0)))
    print(Pair(1, 2))
    name: str | None = \"argon\"
    print(name)
";
    assert_eq!(
        printed("values", program).unwrap(),
        "\
None
42 None
(1, 2.5) (True,) ()
('heat', (Phase.GAS, 2))
Phase.SOLID Phase.LIQUID Phase.GAS
Point(x=1.0, y=-2.5)
Sample(label=\"it's\", phase=Phase.LIQUID, at=Point(x=0.0, y=1e+16))
Sample(label='run', phase=Phase.GAS, at=Point(x=3.0, y=4.0))
Pair(first=1, second=2)
argon
"
    );
}

/// `sep=` goes between the values and `end=` after the last, a literal or a `str` variable;
/// `None` keeps the default, as in Python. Output that does not end its line still reaches
/// standard output when the program ends.
#[test]
fn sep_and_end_replace_the_space_and_line_end() {
    let program = "\
def main():
    comma = \", \"
    print(1, 2, 3, sep=comma)
    print(\"a\", \"b\", sep=\"\", end=\"|\")
    print(\"c\", end=None, sep=None)
    print(\"x\", \"y\", sep=\"-\", end=\"\")
    print(end=\"!\")
";
    assert_eq!(
        printed("separators", program).unwrap(),
        "1, 2, 3\nab|c\nx-y!"
    );
}

/// A list prints as `[1, 2, 3]` and a dict as `{'a': 1}`, as Python's `print` writes them, with
/// each element, key and value written as Python's `repr` writes it: a `str` quoted, a nested
/// list or tuple the same way. A dict lists its entries in insertion order, so a key popped and
/// one added later print where Python prints them.
#[test]
fn lists_and_dicts_print_as_python_does() {
    let program = "\
class Point(Struct, Copy):
    x: f64
    y: f64


def main():
    print([1, 2, 3], [\"a\", \"it's\"])
    print([[1.5, 2.0], [], [3.25]])
    empty: list[i64] = []
    print(empty, [True, False], [(1, \"z\")], [Point(1.0, 2.0)])
    names = [\"argon\"]
    names.append(\"neon\")
    print(names, len(names))
    masses = {\"Ar\": 39.948, \"He\": 4.0026}
    print(masses)
    series: dict[str, list[f64]] = {}
    print(series)
    series[\"x\"] = [0.1, 1e20]
    series[\"y\"] = []
    print(series)
    counts = {1: \"one\", 2: \"two\"}
    counts.pop(1)
    counts[3] = \"three\"
    print(counts)
";
    assert_eq!(
        printed("collections", program).unwrap(),
        "\
[1, 2, 3] ['a', \"it's\"]
[[1.5, 2.0], [], [3.25]]
[] [True, False] [(1, 'z')] [Point(x=1.0, y=2.0)]
['argon', 'neon'] 2
{'Ar': 39.948, 'He': 4.0026}
{}
{'x': [0.1, 1e+20], 'y': []}
{2: 'two', 3: 'three'}
"
    );
}
