//! Values that own memory passed where a read-only parameter takes them or `None`, built and run
//! (roadmap 1.9.29): `yerr=errors` for `yerr: Array[f64, 1] | None = None`, as `byplot`'s
//! `errorbar` takes it.

use super::{baryon_build, package, stderr};
use std::process::Command;

/// An `Array`, a `str`, a list and a member of a union of structs, each passed as a variable,
/// a new value and `None` where an optional or the union is borrowed. Each is wrapped without
/// moving it, so the caller's value is printed whole afterwards and freed once; under Valgrind
/// (the ownership sweep's site `optional_borrow`) nothing is read after a free or leaked.
/// Before, lowering reported "borrowing a value that is not a copy type as an optional".
#[test]
fn values_that_own_memory_are_borrowed_as_optionals() {
    let program = "\
class Probe(Struct):
    name: str


class Hit(Struct):
    energy: f64


Event = Probe | Hit


def total(values: Array[f64, 1], errors: Array[f64, 1] | None = None) -> f64:
    if errors is None:
        return values.sum()
    return values.sum() + errors.sum()


def label(name: str | None) -> str:
    if name is None:
        return \"none\"
    return name + \"!\"


def count(xs: list[str] | None) -> int:
    if xs is None:
        return 0
    return len(xs)


def kind(event: Event) -> str:
    match event:
        case Probe():
            return \"probe\"
        case Hit():
            return \"hit\"


def main():
    v = Array([1.0, 2.0])
    e = Array([0.5, 0.5])
    print(total(v), total(v, e), total(v, None), total(v, Array([1.0, 1.0])))
    print(total(v, errors=e), e)
    word = \"ab\" + \"cd\"
    print(label(word), label(None), label(\"x\" + \"y\"), word)
    names = [\"a\", \"b\"]
    print(count(names), count(None), names)
    p = Probe(\"pr\" + \"obe\")
    print(kind(p), p.name)
";
    let directory = package("optional_arguments", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "3.0 4.0 3.0 5.0\n4.0 [0.5 0.5]\nabcd! none xy! abcd\n2 0 ['a', 'b']\nprobe probe\n"
    );
}
