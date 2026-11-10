//! Dictionaries in the type checker (decision 0035, roadmap 1.13.4): what checks clean and the
//! full text of each error a `dict` can give.
//!
//! These tests live in the driver because they need several implementations, and only an
//! application crate may depend on more than one (decision 0001).

use driver::Session;

/// Checks `main.byn` and returns every diagnostic, rendered and joined by blank lines. An empty
/// string means the program is clean. The program's first newline, which only lets a test start
/// it on its own line, is dropped.
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

/// Every operation decision 0035 keeps for 0.2.0 checks clean: a display typed from its first
/// entry or its annotation, keys of every kind, `d[k]` read, written and updated, `in`, `len`,
/// `get`, `pop`, `copy`, and loops over the keys, values and items.
#[test]
fn dict_operations_check_clean() {
    let program = r#"
class Phase(Enum):
    SOLID = 1
    GAS = 2


def tally(words: list[str]) -> dict[str, int]:
    counts: dict[str, int] = {}
    for word in words:
        if word in counts:
            counts[word] += 1
        else:
            counts[word] = 1
    return counts


def main():
    masses = {"Ar": 39.948, "He": 4.0026}
    masses["Ne"] = 20.180
    grid: dict[tuple[int, int], f64] = {(0, 1): 0.5}
    phases: dict[Phase, bool] = {Phase.GAS: True}
    names: dict[int, str] = {1: "H"}
    total = masses["Ar"] + masses.get("Kr", 0.0) + grid[(0, 1)]
    lost = masses.pop("He")
    name = names.pop(2, "none")
    found = masses.get("Xe")
    print(total, lost, name, found, len(masses), Phase.GAS in phases)
    copied = names.copy()
    for z, symbol in copied.items():
        print(z, symbol)
    for key in masses:
        print(key)
    for mass in masses.values():
        print(mass)
    print(tally(["a", "b", "a"])["a"])
"#;
    assert_eq!(check_one(program), "");
}

/// Float and struct keys, a display with no type, entries of two types, `get` of a value that
/// does not copy, two keys, a slice, a key of the wrong type, a change inside a loop over the
/// dict, a view outside a loop, and a method a `dict` does not have.
#[test]
fn dict_errors_are_reported() {
    let program = r#"
class Point(Struct):
    x: f64


def main():
    bins: dict[f64, int] = {}
    points: dict[Point, int] = {}
    empty = {}
    mixed = {"a": 1, "b": "two"}
    d: dict[str, list[int]] = {}
    print(d.get("a"))
    print(d["a", "b"])
    print(d[1:2])
    print(3 in d)
    for k in d.keys():
        d["x"] = [1]
    v = d.items()
    d.sort()
    print({1.5: 2})
"#;
    assert_eq!(
        check_one(program),
        r#"error: `f64` cannot be a dict key
 --> main.byn:6:16
  |
6 |     bins: dict[f64, int] = {}
  |                ^^^ a float key
  = note: floats are not compared exactly: `0.1 + 0.2` is not `0.3`. Bin the value to an integer first, such as `int(x / width)`

error: `Point` cannot be a dict key
 --> main.byn:7:18
  |
7 |     points: dict[Point, int] = {}
  |                  ^^^^^ a struct key
  = note: use a tuple of its fields as the key

error: an empty dict needs its type written
 --> main.byn:8:13
  |
8 |     empty = {}
  |             ^^ the key and value types are unknown
  = note: for example `counts: dict[str, i64] = {}`

error: dict values must share one type, found `i64` and `str`
 --> main.byn:9:27
  |
9 |     mixed = {"a": 1, "b": "two"}
  |                           ^^^^^ a different type

error: `get` with no default needs values that copy, found `list[i64]`
  --> main.byn:11:11
   |
11 |     print(d.get("a"))
   |           ^^^^^^^^^^ would copy the value out
   = note: ask first, then borrow it: `if k in d:` and `d[k]`

error: a dict takes one key
  --> main.byn:12:11
   |
12 |     print(d["a", "b"])
   |           ^^^^^^^^^^^ more than one key
   = note: a key of several parts is a tuple: `d[(i, j)]`

error: a dict cannot be sliced
  --> main.byn:13:11
   |
13 |     print(d[1:2])
   |           ^^^^^^ a slice
   = note: a dict is looked up by key: `d[k]`

error: expected `str`, found `i64`
  --> main.byn:14:11
   |
14 |     print(3 in d)
   |           ^ the dict's keys are `str`

error: `d` cannot be changed while a loop goes over it
  --> main.byn:16:9
   |
16 |         d["x"] = [1]
   |         ^^^^^^ changed here
  ::: main.byn:15:14
   |
15 |     for k in d.keys():
   |              - the loop goes over `d` here
   = note: loop over a copy instead: `for k in d.copy():`

error: `items()` of a dict is only for a `for` loop
  --> main.byn:17:9
   |
17 |     v = d.items()
   |         ^^^^^^^^^ not in a loop
   = note: loop over it, `for k, v in d.items():`; `len(d)` counts the keys

error: `dict[str, list[i64]]` has no method `sort` taking these arguments
  --> main.byn:18:5
   |
18 |     d.sort()
   |     ^^^^^^^^ not defined
   = note: a dict has `get`, `pop`, `setdefault`, `update`, `clear`, `copy`, and `keys`, `values` and `items` in a `for` loop

error: `f64` cannot be a dict key
  --> main.byn:19:12
   |
19 |     print({1.5: 2})
   |            ^^^ a float key
   = note: floats are not compared exactly: `0.1 + 0.2` is not `0.3`. Bin the value to an integer first, such as `int(x / width)`"#
    );
}

/// The errors of the 0.3.0 operations (roadmap 1.13.14): `del` of anything but a dict entry,
/// `setdefault` of a value that does not copy, `update` from a dict of another type or from the
/// dict itself, `clear` with an argument, a comprehension value that moves a borrowed element, a
/// float key, and a value of the wrong type for the annotation.
#[test]
fn dict_follow_up_errors_are_reported() {
    let program = r#"
def main():
    d: dict[str, list[int]] = {}
    xs = [1, 2]
    other: dict[str, int] = {}
    words = ["a", "b"]
    del xs[0]
    del d
    print(d.setdefault("a", [1]))
    d.update(other)
    d.update(d)
    d.clear(1)
    first = {w: w for w in words}
    floats = {x * 0.5: x for x in range(3)}
    typed: dict[str, int] = {w.copy(): 1.5 for w in words}
"#;
    assert_eq!(
        check_one(program),
        r#"error: `del` is supported only on a dict entry
 --> main.byn:7:5
  |
7 |     del d
  |     ^^^^^
  = note: a value is destroyed when its owner goes out of scope, so nothing else needs `del`
  = note: remove one dict entry with `del d[k]`

error: `del` removes a dict entry, not an element of `list[i64]`
 --> main.byn:6:9
  |
6 |     del xs[0]
  |         ^^^^^ not a dict
  = note: a list shrinks with `xs.pop()`; an array keeps its length

error: `setdefault` needs values that copy, found `list[i64]`
 --> main.byn:8:11
  |
8 |     print(d.setdefault("a", [1]))
  |           ^^^^^^^^^^^^^^^^^^^^^^ would copy the value out
  = note: ask first, then borrow it: `if k in d:` and `d[k]`

error: expected `dict[str, list[i64]]`, found `dict[str, i64]`
 --> main.byn:9:14
  |
9 |     d.update(other)
  |              ^^^^^ `update` takes a dict of the same type, `dict[str, list[i64]]`

error: `d` is changed by this call and also passed to it
  --> main.byn:10:14
   |
10 |     d.update(d)
   |              ^ `d` again
  ::: main.byn:10:5
   |
10 |     d.update(d)
   |     - `d` is passed as `InOut`
   = note: a value this call changes cannot also be read through another argument; pass a copy: `d.copy()`

error: `dict[str, list[i64]]` has no method `clear` taking these arguments
  --> main.byn:11:5
   |
11 |     d.clear(1)
   |     ^^^^^^^^^^ not defined
   = note: a dict has `get`, `pop`, `setdefault`, `update`, `clear`, `copy`, and `keys`, `values` and `items` in a `for` loop

error: `w` borrows an element of `words`, so it cannot be moved
  --> main.byn:12:17
   |
12 |     first = {w: w for w in words}
   |                 ^ moved here
   = note: to keep the element, keep a copy: `w.copy()`

error: `f64` cannot be a dict key
  --> main.byn:13:15
   |
13 |     floats = {x * 0.5: x for x in range(3)}
   |               ^^^^^^^ a float key
   = note: floats are not compared exactly: `0.1 + 0.2` is not `0.3`. Bin the value to an integer first, such as `int(x / width)`

error: expected `i64`, found `f64`
  --> main.byn:14:40
   |
14 |     typed: dict[str, int] = {w.copy(): 1.5 for w in words}
   |                                        ^^^ the dict's values are `i64`"#
    );
}
