//! Dictionaries (decision 0035, roadmap 1.13.4 and 1.13.14), built and run: displays, `d[k]`
//! read, written and updated, `in`, `len`, `get` (of `str` values too), `pop`, `setdefault`,
//! `update`, `clear`, `copy`, `del d[k]`, dict comprehensions, loops in insertion order, and the
//! panic for a missing key.
//! Python prints the same lines for every program here.

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

/// The 0.3.0 operations (roadmap 1.13.14): `setdefault`, `update`, `clear` and `del d[k]`,
/// dict comprehensions with one and two clauses and a condition, a comprehension whose key is
/// read again by its value, and loops and list comprehensions that borrow `list` values.
#[test]
fn dict_follow_ups_build_and_run() {
    let program = r#"
def main():
    counts: dict[str, int] = {}
    for w in ["a", "b", "a", "c", "a"]:
        counts[w] = counts.setdefault(w, 0) + 1
    print(counts.setdefault("a", 100), counts.setdefault("z", 7), len(counts))
    more: dict[str, int] = {"b": 20, "y": 9}
    counts.update(more)
    for k, v in counts.items():
        print(k, v)
    del counts["a"]
    print("a" in counts, len(counts))
    counts.clear()
    print(len(counts))
    counts["q"] = 1
    print(counts["q"])
    squares = {i: i * i for i in range(5) if i != 2}
    for n, sq in squares.items():
        print(n, sq)
    pairs = {(i, j): i * 10 + j for i in range(2) for j in range(2)}
    print(pairs[(1, 0)], len(pairs))
    names: dict[int, str] = {1: "H", 2: "He"}
    inverse = {sq: n for n, sq in squares.items()}
    print(inverse[9])
    lengths = {name: len(name) for name in ["Ar", "Ne", "Kr"]}
    print(lengths)
    lists: dict[str, list[f64]] = {"a": [1.0, 2.0], "b": [3.0]}
    for key, values in lists.items():
        print(key, len(values))
    for values in lists.values():
        print(values[0])
    sizes = [len(v) for v in lists.values()]
    print(sizes)
    print([len(n) for n in names.values()])
"#;
    let (printed, status, report) = run("dict-follow-ups", program).unwrap();
    assert_eq!(report, "");
    assert_eq!(
        printed,
        "3 7 4\na 3\nb 20\nc 1\nz 7\ny 9\nFalse 4\n0\n1\n0 0\n1 1\n3 9\n4 16\n10 4\n3\n{'Ar': 2, 'Ne': 2, 'Kr': 2}\na 2\nb 1\n1.0\n3.0\n[2, 1]\n[1, 2]\n"
    );
    assert_eq!(status, Some(0));
}

/// `get` and `setdefault` of `str` values and of tuples holding one copy the value out, so
/// the copy outlives a later change to the entry; an unused default that owns a string is
/// dropped.
#[test]
fn getting_a_str_value_copies_it() {
    let program = r#"
def main():
    names: dict[int, str] = {1: "H", 2: "He"}
    first = names.get(1)
    missing = names.get(9)
    print(first, missing)
    fallback = "none"
    print(names.get(2, "x"), names.get(7, "unknown"), names.get(8, fallback), names.get(1, "?"))
    print(names.setdefault(3, "Li"), names.setdefault(1, "hydrogen"), len(names))
    for z, n in names.items():
        print(z, n)
    pairs: dict[str, tuple[str, int]] = {"a": ("x", 1)}
    print(pairs.get("a", ("y", 0)), pairs.get("b", ("y", 0)))
    copy = names.get(2)
    names[2] = "helium"
    print(copy, names[2])
    spare = f"s{1}"
    print(names.get(1, spare))
    other = f"o{2}"
    print(names.setdefault(1, other), names.setdefault(5, f"B{5}"))
"#;
    let (printed, status, report) = run("dict-get-str", program).unwrap();
    assert_eq!(report, "");
    assert_eq!(
        printed,
        "H None\nHe unknown none H\nLi H 3\n1 H\n2 He\n3 Li\n('x', 1) ('y', 0)\nHe helium\nH\nH B5\n"
    );
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
        (
            "    d: dict[str, int] = {\"a\": 1}\n    del d[\"zz\"]\n",
            "panic: key 'zz' is not in the dict\n  --> main.byn:8:9\n",
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
