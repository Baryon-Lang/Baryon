//! Dictionaries (decision 0035, roadmap 1.13.4), built and run: displays, `d[k]` read, written
//! and updated, `in`, `len`, `get`, `pop`, `copy`, loops in insertion order, and the panic for
//! a missing key. Python prints the same lines for every program here.

use super::{baryon_build, package, panic_report, stderr};
use std::io;
use std::process::Command;

/// Builds the one-file package `name` from `text`, runs it, and returns what it printed, its
/// exit code and its panic report, after checking that the build succeeded.
///
/// # Errors
///
/// When writing the package or starting `baryon` or the program fails.
fn run(name: &str, text: &str) -> io::Result<(String, Option<i32>, String)> {
    let directory = package(name, &[("main.byn", text)])?;
    let (output, program) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output()?;
    Ok((
        String::from_utf8_lossy(&ran.stdout).into_owned(),
        ran.status.code(),
        panic_report(&ran),
    ))
}

/// A `str` to `f64` table: setting a key that is there keeps its place and replaces the value,
/// a new key goes last, `pop` takes an entry out, and loops see the entries in insertion order.
/// `counts[k] += 1` updates the entry in place.
#[test]
fn dict_operations_build_and_run() {
    let program = r#"
def main():
    masses: dict[str, f64] = {"Ar": 39.948, "He": 4.0026}
    masses["Ne"] = 20.180
    masses["He"] = 4.003
    print(len(masses))
    print(masses["Ne"])
    print("Ar" in masses, "Xe" in masses, "Xe" not in masses)
    print(masses.get("Kr", 83.798))
    print(masses.pop("Ar"))
    for name, mass in masses.items():
        print(name, mass)
    for name in masses:
        print(name)
    counts: dict[int, int] = {}
    for i in range(10):
        k = i % 3
        if k in counts:
            counts[k] += 1
        else:
            counts[k] = 1
    for k, n in counts.items():
        print(k, n)
"#;
    let (printed, status, report) = run("dict-operations", program).unwrap();
    assert_eq!(report, "");
    assert_eq!(
        printed,
        "3\n20.18\nTrue False True\n83.798\n39.948\nHe 4.003\nNe 20.18\nHe\nNe\n0 4\n1 3\n2 3\n"
    );
    assert_eq!(status, Some(0));
}

/// Values and keys that own memory: `str` values are replaced and popped, `pop` with a default
/// gives the default for a missing key, a copy is a new table, a `list` value is changed in
/// place through `d[k]`, and tuple and enum keys hash by their parts.
#[test]
fn dicts_of_owned_values_and_compound_keys_build_and_run() {
    let program = r#"
class Phase(Enum):
    SOLID = 1
    GAS = 2


def main():
    names: dict[int, str] = {1: "H", 2: "He"}
    names[3] = "Li"
    names[1] = "hydrogen"
    for z, n in names.items():
        print(z, n)
    print(names.pop(2), names.pop(9, "none"))
    other = names.copy()
    other[4] = "Be"
    print(len(names), len(other))
    grid: dict[tuple[int, int], f64] = {(0, 1): 0.5}
    grid[(2, 3)] = 1.5
    print(grid[(2, 3)], (0, 1) in grid)
    lists: dict[str, list[f64]] = {}
    lists["a"] = [1.0, 2.0]
    lists["a"].append(3.0)
    print(len(lists["a"]))
    phases: dict[Phase, int] = {Phase.SOLID: 1}
    for v in phases.values():
        print(v)
"#;
    let (printed, status, report) = run("dict-owned", program).unwrap();
    assert_eq!(report, "");
    assert_eq!(
        printed,
        "1 hydrogen\n2 He\n3 Li\nHe none\n2 3\n1.5 True\n3\n1\n"
    );
    assert_eq!(status, Some(0));
}

/// The way out of the loop rule the checker gives: a loop over a copy may change the original,
/// removing and replacing entries, because the copy is its own table.
#[test]
fn a_loop_over_a_copy_changes_the_original() {
    let program = r#"
def main():
    d: dict[str, int] = {"a": 1, "b": 2, "c": 3}
    for k in d.copy():
        if d[k] % 2 == 1:
            d.pop(k)
    for k, v in d.items():
        print(k, v)
    for k in d.copy():
        d[k] = 10
    print(d["b"], len(d))
"#;
    let (printed, status, report) = run("dict-copy-loop", program).unwrap();
    assert_eq!(report, "");
    assert_eq!(printed, "b 2\n10 1\n");
    assert_eq!(status, Some(0));
}

/// `d[k] = v` leaves the key's variable usable, as in Python: a `str` key, or a tuple holding
/// one, read from a variable is copied into the table rather than moved, so the variable and the
/// table each free their own string.
#[test]
fn storing_a_key_from_a_variable_copies_it() {
    let program = r#"
def main():
    n = f"x{1}"
    d: dict[str, int] = {}
    d[n] = 1
    d[n] += 1
    pair = (f"p{2}", 3)
    t: dict[tuple[str, int], str] = {}
    t[pair] = "v"
    t[pair] = "w"
    print(n, d[n], pair[0], t[pair], len(t))
"#;
    let (printed, status, report) = run("dict-stored-key", program).unwrap();
    assert_eq!(report, "");
    assert_eq!(printed, "x1 2 p2 w 1\n");
    assert_eq!(status, Some(0));
}

/// A missing key panics, naming the key the way Python's `repr` writes it: a `str` quoted, a
/// tuple in parentheses, an enum member by its class and name.
#[test]
fn a_missing_key_panics_naming_the_key() {
    let cases = [
        (
            "    d: dict[str, int] = {\"Ar\": 18}\n    print(d[\"Xe\"])\n",
            "panic: key 'Xe' is not in the dict\n  --> main.byn:8:11\n",
        ),
        (
            "    d: dict[tuple[int, bool], int] = {}\n    print(d[(3, True)])\n",
            "panic: key (3, True) is not in the dict\n  --> main.byn:8:11\n",
        ),
        (
            "    d: dict[Phase, int] = {Phase.SOLID: 1}\n    print(d[Phase.GAS])\n",
            "panic: key Phase.GAS is not in the dict\n  --> main.byn:8:11\n",
        ),
        (
            "    d: dict[int, str] = {}\n    print(d.pop(-4))\n",
            "panic: key -4 is not in the dict\n  --> main.byn:8:11\n",
        ),
    ];
    for (index, (body, expected)) in cases.into_iter().enumerate() {
        let program =
            format!("class Phase(Enum):\n    SOLID = 1\n    GAS = 2\n\n\ndef main():\n{body}");
        let (printed, status, report) = run(&format!("dict-panic-{index}"), &program).unwrap();
        assert_eq!(printed, "");
        assert_eq!(status, Some(101), "{report}");
        assert_eq!(report, expected);
    }
}
