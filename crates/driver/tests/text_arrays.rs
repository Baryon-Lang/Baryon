//! Text arrays, `Array[str, 1]` (roadmap 3.13.3, decision 0045), through every stage up to
//! the type checker: the type, building one from a list, what reads it, and the mistakes and
//! the uses not built yet that it reports.
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

/// A program that uses a text array every way the checker knows: `uses` holds one mistake
/// or one use not built yet on each line, and `fine` only what is built.
const PROGRAM: &str = r#"
class Tag(Struct):
    label: str
    names: Array[str, 1]


def uses(other: Array[str, 1], counts: Array[i64, 1]):
    names = Array(["a", "b"])
    grid: Array[str, 2] = Array(["a"])
    wrong: Array[str, 1] = Array([1.0])
    names[0] = "q"
    kept = names[0]
    part = names[Array([1.5])]
    both = names[0, 1]
    joined = names + other
    same = names == other
    found = "a" in names
    total = names.sum()
    shape = names.shape
    least = min(names)
    print(names)
    text = f"{names}"
    numbers: Array[i64, 1] = names.copy()
    texts: Array[str, 1] = counts.copy()
    zeros = Array.zeros(3, str)
    moved = names
    again = len(names)


def fine(names: Array[str, 1]) -> Array[str, 1]:
    count = len(names)
    first = names[0].copy()
    for name in names:
        count += len(name)
    for position, name in enumerate(reversed(names)):
        count += position
    empty: Array[str, 1] = Array([])
    part = names[1:]
    chosen = part[Array([True, False])]
    taken = names[Array([0, 0])][::-1]
    return names.copy()
"#;

/// What the checker reports for [`PROGRAM`], in source order.
const EXPECTED: &str = r#"error: a struct field cannot hold an array of `str` yet
 --> main.byn:3:12
  |
3 |     names: Array[str, 1]
  |            ^^^^^^^^^^^^^ holds an array of `str`
  = note: keep the array in a variable and pass it beside the struct, or hold a `list[str]`; like a frame in a struct field, this is roadmap idea I.49

error: an array of `str` has one dimension
 --> main.byn:8:22
  |
8 |     grid: Array[str, 2] = Array(["a"])
  |                      ^ not 1
  = note: write `Array[str, 1]`; for a grid of text use a `list` of them

error: list items must share one type, found `str` and `f64`
 --> main.byn:9:35
  |
9 |     wrong: Array[str, 1] = Array([1.0])
  |                                   ^^^ a different type

error: an element of an array of `str` cannot be assigned yet
  --> main.byn:10:5
   |
10 |     names[0] = "q"
   |     ^^^^^^^^ assigned here
   = note: the elements are stored end to end, so one cannot change length in place; build a new array from a list, `Array(words)`

error: a value cannot be moved out of an element
  --> main.byn:11:12
   |
11 |     kept = names[0]
   |            ^^^^^^^^ moved out of an element
   = note: move a copy instead: `names[0].copy()`

error: `Array[str, 1]` cannot be indexed by `Array[f64, 1]`
  --> main.byn:12:12
   |
12 |     part = names[Array([1.5])]
   |            ^^^^^^^^^^^^^^^^^^^ not a choice of elements
   = note: a rank-1 `Array` takes one rank-1 array: a mask of `bool`s, `a[a > 0.0]`, or `i64` positions, `a[order]`

error: an array of `str` has one dimension, so it takes one index
  --> main.byn:13:12
   |
13 |     both = names[0, 1]
   |            ^^^^^^^^^^^ more than one index
   = note: for example `names[i]`

error: `+` is not built for an array of `str` yet
  --> main.byn:14:14
   |
14 |     joined = names + other
   |              ^^^^^^^^^^^^^ an array of `str`
   = note: an array of `str` has `len`, indexing, slices, masks, `for` loops, `copy()`, `unique()` and `nunique()` today; the rest comes with roadmap item 3.13.3

error: `==` is not built for an array of `str` yet
  --> main.byn:15:17
   |
15 |     same = names == other
   |                 ^^^^^^^^^ an array of `str`
   = note: an array of `str` has `len`, indexing, slices, masks, `for` loops, `copy()`, `unique()` and `nunique()` today; the rest comes with roadmap item 3.13.3

error: `in` is not built for an array of `str` yet
  --> main.byn:16:16
   |
16 |     found = "a" in names
   |                ^^^^^^^^^ an array of `str`
   = note: an array of `str` has `len`, indexing, slices, masks, `for` loops, `copy()`, `unique()` and `nunique()` today; the rest comes with roadmap item 3.13.3

error: the method `sum` is not built for an array of `str` yet
  --> main.byn:17:13
   |
17 |     total = names.sum()
   |             ^^^^^^^^^^^ an array of `str`
   = note: an array of `str` has `len`, indexing, slices, masks, `for` loops, `copy()`, `unique()` and `nunique()` today; the rest comes with roadmap item 3.13.3

error: `shape` is not built for an array of `str` yet
  --> main.byn:18:13
   |
18 |     shape = names.shape
   |             ^^^^^^^^^^^ an array of `str`
   = note: an array of `str` has `len`, indexing, slices, masks, `for` loops, `copy()`, `unique()` and `nunique()` today; the rest comes with roadmap item 3.13.3

error: `min` is not built for an array of `str` yet
  --> main.byn:19:17
   |
19 |     least = min(names)
   |                 ^^^^^ an array of `str`
   = note: an array of `str` has `len`, indexing, slices, masks, `for` loops, `copy()`, `unique()` and `nunique()` today; the rest comes with roadmap item 3.13.3

error: an array of `str` cannot be shown yet
  --> main.byn:20:11
   |
20 |     print(names)
   |           ^^^^^ an array of `str`
   = note: show its elements one at a time: `for name in names: print(name)`

error: an array of `str` cannot be shown yet
  --> main.byn:21:15
   |
21 |     text = f"{names}"
   |               ^^^^^ an array of `str`
   = note: show its elements one at a time: `for name in names: print(name)`

error: expected `Array[i64, 1]`, found `Array[str, 1]`
  --> main.byn:22:30
   |
22 |     numbers: Array[i64, 1] = names.copy()
   |                              ^^^^^^^^^^^^ the annotation says `Array[i64, 1]`

error: expected `Array[str, 1]`, found `Array[i64, 1]`
  --> main.byn:23:28
   |
23 |     texts: Array[str, 1] = counts.copy()
   |                            ^^^^^^^^^^^^^ the annotation says `Array[str, 1]`

error: `Array.zeros` cannot make an array of `str` yet
  --> main.byn:24:13
   |
24 |     zeros = Array.zeros(3, str)
   |             ^^^^^^^^^^^^^^^^^^^ an array of `str`
   = note: make one from a list of strings: `Array(words)`

error: `names` was moved
  --> main.byn:26:17
   |
26 |     again = len(names)
   |                 ^^^^^ read after the move
  ::: main.byn:25:13
   |
25 |     moved = names
   |             ----- `names` moved here
   = note: to keep both, move a copy: `names.copy()`"#;

/// A text array is built from a list of `str`, counted, indexed, looped over, copied,
/// sliced and chosen from by a mask or by positions, the new arrays kept, without a
/// diagnostic, as `fine` does. Everything else is reported: a struct field that holds one,
/// more than one dimension, a list of another type, an element assigned or moved out, an
/// index that is an array of `f64`, two indices, operators, methods, attributes and
/// built-ins not built yet,
/// showing one, an array of numbers where one of text is expected and the other way round,
/// `Array.zeros` of `str`, and a read after a move, since a text array moves as any array.
#[test]
fn text_arrays_are_typed_and_report_what_is_not_built() {
    assert_eq!(check_one(PROGRAM), EXPECTED);
}
