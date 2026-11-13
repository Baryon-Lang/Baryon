//! Moves, end to end, through every stage up to the type checker: a value that is not a copy
//! type moves when assigned, returned, passed as `Owned` or built into a tuple or list, and
//! the old owner cannot be read until it is assigned again.
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

/// A swap, an `Owned` parameter moved and returned, a variable assigned again after a move,
/// copies, copy types used twice, a field moved out of an owned struct, and a variable moved
/// and assigned again inside a loop.
#[test]
fn moves_that_follow_the_rules_check_clean() {
    let program = r"
class Atoms(Struct):
    positions: Array[f64, 1]
    forces: Array[f64, 1]


class Point(Struct, Copy):
    x: f64
    y: f64


def consume(values: Owned[Array[f64, 1]]) -> f64:
    return values.sum()


def keep(values: Owned[Array[f64, 1]]) -> Array[f64, 1]:
    return values


def positions(atoms: Owned[Atoms]) -> Array[f64, 1]:
    return atoms.positions


def step(u: Array[f64, 1], n: i64) -> f64:
    out = u.copy()
    work = u.copy()
    work, out = out, work
    total = consume(out)
    out = work.copy()
    total += out.sum()
    for i in range(n):
        total += consume(work)
        work = out.copy()
    point = Point(1.0, 2.0)
    other = point
    return total + point.x + other.y + keep(work).sum()
";
    assert_eq!(check_one(program), "");
}

/// A read after a move, after a move on one branch, and after a field moved out; moving a
/// read-only or `InOut` parameter, returning one included; moving out of an element; keeping
/// a slice; and a move in one pass of a loop that the next pass reads.
#[test]
fn moves_that_break_the_rules_are_reported() {
    let program = r"
class Atoms(Struct):
    positions: Array[f64, 1]
    forces: Array[f64, 1]


def consume(values: Owned[Array[f64, 1]]) -> f64:
    return values.sum()


def reads(u: Array[f64, 1], flag: bool) -> f64:
    a = u.copy()
    b = a
    c = u.copy()
    if flag:
        consume(c)
    atoms = Atoms(u.copy(), u.copy())
    x = atoms.positions
    return a.sum() + c.sum() + atoms.forces.sum() + b.sum() + x.sum()


def borrowed(u: Array[f64, 1], v: InOut[Array[f64, 1]]) -> Array[f64, 1]:
    w = v
    return u


def elements(rows: list[Array[f64, 1]], grid: Array[f64, 2]) -> f64:
    first = rows[0]
    column = grid[:, 0]
    return first.sum() + column.sum()


def looped(u: Array[f64, 1], n: i64) -> f64:
    work = u.copy()
    total = 0.0
    for i in range(n):
        total += consume(work)
    return total
";
    assert_eq!(
        check_one(program),
        r"error: `a` was moved
  --> main.byn:18:12
   |
18 |     return a.sum() + c.sum() + atoms.forces.sum() + b.sum() + x.sum()
   |            ^ read after the move
  ::: main.byn:12:9
   |
12 |     b = a
   |         - `a` moved here
   = note: to keep both, move a copy: `a.copy()`

error: `c` was moved
  --> main.byn:18:22
   |
18 |     return a.sum() + c.sum() + atoms.forces.sum() + b.sum() + x.sum()
   |                      ^ read after the move
  ::: main.byn:15:17
   |
15 |         consume(c)
   |                 - `c` moved here
   = note: to keep both, move a copy: `c.copy()`

error: `atoms` was moved
  --> main.byn:18:32
   |
18 |     return a.sum() + c.sum() + atoms.forces.sum() + b.sum() + x.sum()
   |                                ^^^^^ read after the move
  ::: main.byn:17:9
   |
17 |     x = atoms.positions
   |         --------------- `atoms` moved here
   = note: to keep both, move a copy: `atoms.positions.copy()`

error: `v` is borrowed from the caller as `InOut`, so it cannot be moved
  --> main.byn:22:9
   |
22 |     w = v
   |         ^ moved here
   = note: move a copy instead: `v.copy()`, or assign it a new value in place

error: `u` is borrowed from the caller, so it cannot be moved
  --> main.byn:23:12
   |
23 |     return u
   |            ^ moved here
   = note: move a copy instead: `u.copy()`, or take the parameter as `Owned[...]`

error: a value cannot be moved out of an element
  --> main.byn:27:13
   |
27 |     first = rows[0]
   |             ^^^^^^^ moved out of an element
   = note: move a copy instead: `rows[0].copy()`

error: a slice of an array cannot be kept
  --> main.byn:28:14
   |
28 |     column = grid[:, 0]
   |              ^^^^^^^^^^ a view of the array's memory
   = note: a slice can be passed to a call or assigned to, but to keep it, copy it: `grid[:, 0].copy()`

error: `work` is moved in one pass of the loop and read in the next
  --> main.byn:36:26
   |
36 |         total += consume(work)
   |                          ^^^^ moved here, inside a loop
   = note: move a copy, `.copy()`, or assign the variable a new value later in the loop"
    );
}

/// A field of a list element is part of the element, so moving it out is moving out of an
/// element: by assignment, through a nested field, into a list and in a comprehension. Copies
/// and borrows of the same fields check clean. Before, `first = people[0].name` moved `people`
/// as a whole, so the list and `first` both owned one `str` and it was freed twice.
#[test]
fn fields_of_elements_cannot_be_moved_out() {
    let program = r"
class Person(Struct):
    name: str


class Team(Struct):
    lead: Person


def run(people: Owned[list[Person]], teams: Owned[list[Team]]) -> int:
    first = people[0].name
    lead = teams[0].lead.name
    names: list[str] = []
    names.append(people[1].name)
    twice = [people[0].name for _ in range(2)]
    kept = people[0].name.copy()
    copies = [people[i].name.copy() for i in range(len(people))]
    return len(kept) + len(copies) + len(people[0].name)
";
    assert_eq!(
        check_one(program),
        "error: a value cannot be moved out of an element
  --> main.byn:10:13
   |
10 |     first = people[0].name
   |             ^^^^^^^^^^^^^^ moved out of an element
   = note: move a copy instead: `people[0].name.copy()`

error: a value cannot be moved out of an element
  --> main.byn:11:12
   |
11 |     lead = teams[0].lead.name
   |            ^^^^^^^^^^^^^^^^^^ moved out of an element
   = note: move a copy instead: `teams[0].lead.name.copy()`

error: a value cannot be moved out of an element
  --> main.byn:13:18
   |
13 |     names.append(people[1].name)
   |                  ^^^^^^^^^^^^^^ moved out of an element
   = note: move a copy instead: `people[1].name.copy()`

error: a value cannot be moved out of an element
  --> main.byn:14:14
   |
14 |     twice = [people[0].name for _ in range(2)]
   |              ^^^^^^^^^^^^^^ moved out of an element
   = note: move a copy instead: `people[0].name.copy()`"
    );
}

/// A loop over a temporary list, such as a call's result, borrows each element just as a loop
/// over a variable does: the loop owns the temporary and frees it, elements included, when it
/// ends. So the loop variable cannot be moved into a list, a field or a return value, or
/// changed, and the fix is the same `.copy()`. A copy is fine, and so is keeping a character of
/// a string literal, which is never freed (decision 0022). Before, these checked clean,
/// and the program read and freed each string again after the loop had freed it (roadmap
/// 1.9.23).
#[test]
fn elements_of_a_temporary_list_are_borrowed_too() {
    let program = r#"
class Box(Struct):
    name: str


def words(n: int) -> list[str]:
    return [f"w{k}" for k in range(n)]


def keep(kept: InOut[list[str]], box: InOut[Box]) -> str:
    for word in words(3):
        kept.append(word)
        box.name = word
        kept.append(word.copy())
    rows: list[list[int]] = []
    for row in [[1], [2]]:
        rows.append(row)
        row[0] = 2
    again = [w for w in words(2)]
    letters = [c for c in "ab"]
    for i, text in enumerate(words(2)):
        return text
    return "none"
"#;
    assert_eq!(
        check_one(program),
        "error: `word` borrows an element of `words(3)`, so it cannot be moved
  --> main.byn:11:21
   |
11 |         kept.append(word)
   |                     ^^^^ moved here
   = note: to keep the element, keep a copy: `word.copy()`

error: `word` borrows an element of `words(3)`, so it cannot be moved
  --> main.byn:12:20
   |
12 |         box.name = word
   |                    ^^^^ moved here
   = note: to keep the element, keep a copy: `word.copy()`

error: `row` borrows an element of `[[1], [2]]`, so it cannot be moved
  --> main.byn:16:21
   |
16 |         rows.append(row)
   |                     ^^^ moved here
   = note: to keep the element, keep a copy: `row.copy()`

error: `row` borrows an element of `[[1], [2]]`, so it cannot be changed
  --> main.byn:17:9
   |
17 |         row[0] = 2
   |         ^^^^^^ changed here
   = note: work on a copy: `row.copy()`

error: `w` borrows an element of `words(2)`, so it cannot be moved
  --> main.byn:18:14
   |
18 |     again = [w for w in words(2)]
   |              ^ moved here
   = note: to keep the element, keep a copy: `w.copy()`

error: `text` borrows an element of `enumerate(words(2))`, so it cannot be moved
  --> main.byn:21:16
   |
21 |         return text
   |                ^^^^ moved here
   = note: to keep the element, keep a copy: `text.copy()`"
    );
}

/// A conditional expression that is only lent, to a read-only parameter, a builtin, a method
/// or `print`, cannot have a branch naming a variable's value of a type that moves (roadmap
/// 1.9.22): lowering computes it into a temporary that the statement frees, which freed the
/// variable too. Nested conditionals, lists and fields are reported the same way. Branches that
/// build new values, copies, literals, and conditionals that are kept are fine.
#[test]
fn a_conditional_cannot_lend_a_variable() {
    let program = r#"
class Probe(Struct):
    name: str


def size(text: str) -> int:
    return len(text)


def measure(name: str, long: bool) -> int:
    return size(name[0:1] if long else name)


def report(a: str, b: str, xs: list[int], probe: Probe, long: bool):
    print(size(a if long else b))
    print(len(xs[0:1] if long else xs))
    print((a + "!" if long else (b if len(b) > 2 else "short")).upper())
    print(probe.name if long else "none")


def fine(name: str, long: bool) -> int:
    kept = name.copy() if long else name + "s"
    print(size(name[0:1] if long else name.copy()), kept)
    return size("a" if long else "b")
"#;
    assert_eq!(
        check_one(program),
        "\
error: a conditional expression cannot lend `name` yet
  --> main.byn:10:40
   |
10 |     return size(name[0:1] if long else name)
   |                                        ^^^^ freed with the conditional's value
   = note: lend a copy, `name.copy()`, or choose with an `if` statement and pass `name` itself

error: a conditional expression cannot lend `a` yet
  --> main.byn:14:16
   |
14 |     print(size(a if long else b))
   |                ^ freed with the conditional's value
   = note: lend a copy, `a.copy()`, or choose with an `if` statement and pass `a` itself

error: a conditional expression cannot lend `xs` yet
  --> main.byn:15:36
   |
15 |     print(len(xs[0:1] if long else xs))
   |                                    ^^ freed with the conditional's value
   = note: lend a copy, `xs.copy()`, or choose with an `if` statement and pass `xs` itself

error: a conditional expression cannot lend `b` yet
  --> main.byn:16:34
   |
16 |     print((a + \"!\" if long else (b if len(b) > 2 else \"short\")).upper())
   |                                  ^ freed with the conditional's value
   = note: lend a copy, `b.copy()`, or choose with an `if` statement and pass `b` itself

error: a conditional expression cannot lend `probe.name` yet
  --> main.byn:17:11
   |
17 |     print(probe.name if long else \"none\")
   |           ^^^^^^^^^^ freed with the conditional's value
   = note: lend a copy, `probe.name.copy()`, or choose with an `if` statement and pass `probe.name` itself"
    );
}
