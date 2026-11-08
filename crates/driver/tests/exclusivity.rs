//! Exclusivity and read-only parameters, end to end, through every stage up to the type
//! checker: no argument of a call refers to a value the call changes or moves, and a
//! read-only parameter that is not a copy type is never changed.
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

/// Disjoint fields, the same place read twice, a temporary next to a changed place, copy
/// types passed `Owned` twice, an `InOut` method on an `InOut` parameter, a read-only
/// copy-type parameter changed locally, and the `copy` every struct and `str` has.
#[test]
fn calls_that_keep_their_arguments_apart_check_clean() {
    let program = r"
class Atoms(Struct):
    positions: Array[f64, 1]
    forces: Array[f64, 1]

    def clear(self: InOut[Self]):
        self.forces[0] = 0.0


class Point(Struct, Copy):
    x: f64
    y: f64


def integrate(positions: InOut[Array[f64, 1]], forces: Array[f64, 1]):
    positions[0] += forces[0]


def dot(a: Array[f64, 1], b: Array[f64, 1]) -> f64:
    return a[0] * b[0]


def scale(values: InOut[Array[f64, 1]], factor: f64):
    values[0] *= factor


def step(atoms: InOut[Atoms], n: i64) -> f64:
    integrate(atoms.positions, atoms.forces)
    scale(atoms.forces, atoms.positions[0] + 1.0)
    atoms.clear()
    n = abs(n)
    point = Point(1.0, 2.0)
    point = Point(point.x, point.x)
    snapshot = atoms.copy()
    label = 'step'
    kept = label.copy()
    return dot(atoms.forces, snapshot.forces) + point.y + f64(n) + f64(len(kept))
";
    assert_eq!(check_one(program), "");
}

/// A struct changed by a call that also reads its field, the same array passed twice, two
/// elements of one array (indices are not compared), and a value moved by a call that also
/// reads it.
#[test]
fn arguments_that_overlap_a_changed_or_moved_one_are_reported() {
    let program = r"
class Atoms(Struct):
    positions: Array[f64, 1]
    forces: Array[f64, 1]


def integrate(atoms: InOut[Atoms], forces: Array[f64, 1]):
    pass


def axpy(y: InOut[Array[f64, 1]], x: Array[f64, 1]):
    pass


def swap(a: InOut[f64], b: InOut[f64]):
    pass


def keep(values: Owned[Array[f64, 1]], other: Array[f64, 1]):
    pass


def calls(atoms: InOut[Atoms], u: InOut[Array[f64, 1]]):
    integrate(atoms, atoms.forces)
    axpy(u, u)
    swap(u[0], u[1])
    work = u.copy()
    keep(work, work)
";
    assert_eq!(
        check_one(program),
        r"error: `atoms` is changed by this call and also passed to it
  --> main.byn:23:22
   |
23 |     integrate(atoms, atoms.forces)
   |                      ^^^^^^^^^^^^ `atoms.forces` is part of `atoms`
  ::: main.byn:23:15
   |
23 |     integrate(atoms, atoms.forces)
   |               ----- `atoms` is passed as `InOut`
   = note: a value this call changes cannot also be read through another argument; pass a copy: `atoms.forces.copy()`

error: `u` is changed by this call and also passed to it
  --> main.byn:24:13
   |
24 |     axpy(u, u)
   |             ^ `u` again
  ::: main.byn:24:10
   |
24 |     axpy(u, u)
   |          - `u` is passed as `InOut`
   = note: a value this call changes cannot also be read through another argument; pass a copy: `u.copy()`

error: `u[0]` is changed by this call and also passed to it
  --> main.byn:25:16
   |
25 |     swap(u[0], u[1])
   |                ^^^^ `u[1]` may be the same as `u[0]`: indices are not compared
  ::: main.byn:25:10
   |
25 |     swap(u[0], u[1])
   |          ---- `u[0]` is passed as `InOut`
   = note: a value this call changes cannot also be read through another argument; read it into a variable first: `value = u[1]`

error: `work` is moved by this call and also passed to it
  --> main.byn:27:16
   |
27 |     keep(work, work)
   |                ^^^^ `work` again
  ::: main.byn:27:10
   |
27 |     keep(work, work)
   |          ---- `work` is passed as `Owned`
   = note: a value this call moves cannot also be read through another argument; pass a copy: `work.copy()`"
    );
}

/// A read-only parameter assigned, added to, passed as `InOut`, appended to, given to a
/// method that changes it, and assigned a new value; and `self` changed by a read-only method.
#[test]
fn changes_to_read_only_parameters_are_reported() {
    let program = r"
class Atoms(Struct):
    positions: Array[f64, 1]
    forces: Array[f64, 1]

    def clear(self: InOut[Self]):
        self.forces[0] = 0.0

    def total(self) -> f64:
        self.forces[0] = 1.0
        return self.forces.sum()


def axpy(y: InOut[Array[f64, 1]], x: Array[f64, 1]):
    pass


def changes(u: Array[f64, 1], values: list[f64], atoms: Atoms):
    u[0] = 1.0
    u += 1.0
    axpy(u, u.copy())
    values.append(1.0)
    atoms.clear()
    u = Array(values)
";
    assert_eq!(
        check_one(program),
        r"error: `self` is a read-only parameter, so it cannot be changed
 --> main.byn:9:9
  |
9 |         self.forces[0] = 1.0
  |         ^^^^^^^^^^^^^^ changed here
  = note: to change the struct, declare the method's first parameter `self: InOut[Self]`

error: `u` is a read-only parameter, so it cannot be changed
  --> main.byn:18:5
   |
18 |     u[0] = 1.0
   |     ^^^^ changed here
   = note: take it as `InOut[...]` to change the caller's value, or change a copy: `work = u.copy()`

error: `u` is a read-only parameter, so it cannot be changed
  --> main.byn:19:5
   |
19 |     u += 1.0
   |     ^ changed here
   = note: take it as `InOut[...]` to change the caller's value, or change a copy: `work = u.copy()`

error: `u` is a read-only parameter, so it cannot be changed
  --> main.byn:20:10
   |
20 |     axpy(u, u.copy())
   |          ^ changed here
   = note: take it as `InOut[...]` to change the caller's value, or change a copy: `work = u.copy()`

error: `values` is a read-only parameter, so it cannot be changed
  --> main.byn:21:5
   |
21 |     values.append(1.0)
   |     ^^^^^^ changed here
   = note: take it as `InOut[...]` to change the caller's value, or change a copy: `work = values.copy()`

error: `atoms` is a read-only parameter, so it cannot be changed
  --> main.byn:22:5
   |
22 |     atoms.clear()
   |     ^^^^^ changed here
   = note: take it as `InOut[...]` to change the caller's value, or change a copy: `work = atoms.copy()`

error: `u` is a read-only parameter, so it cannot be changed
  --> main.byn:23:5
   |
23 |     u = Array(values)
   |     ^ changed here
   = note: take it as `InOut[...]` to change the caller's value, or change a copy: `work = u.copy()`"
    );
}

/// A loop over the indices that changes elements, a loop that reads non-copy elements and
/// copies one to keep, a loop over one field that changes another, and a loop over a
/// temporary.
#[test]
fn loops_that_leave_their_collection_alone_check_clean() {
    let program = r"
class Atoms(Struct):
    positions: Array[f64, 1]
    forces: Array[f64, 1]


def run(atoms: InOut[Atoms], rows: InOut[list[Array[f64, 1]]], n: i64) -> f64:
    total = 0.0
    for i in range(len(rows)):
        rows[i][0] = 0.0
    kept: list[Array[f64, 1]] = []
    for row in rows:
        total += row.sum()
        kept.append(row.copy())
    for x in atoms.positions:
        atoms.forces[0] += x
    for k in range(n):
        rows.append(atoms.forces.copy())
    return total
";
    assert_eq!(check_one(program), "");
}

/// Appending to, assigning an element of, and moving the collection a loop goes over, from
/// its body and from a nested loop's body; and changing and moving a loop variable that
/// borrows a non-copy element.
#[test]
fn loops_that_change_their_collection_are_reported() {
    let program = r"
def keep(values: Owned[list[Array[f64, 1]]]):
    pass


def run(rows: InOut[list[Array[f64, 1]]], values: InOut[list[f64]]):
    for v in values:
        values.append(v * 2.0)
        values[0] = 1.0
    for row in rows:
        for v in values:
            rows.append(row.copy())
        row[0] = 1.0
        kept = row
    for row in rows:
        keep(rows)
";
    assert_eq!(
        check_one(program),
        r"error: `values` cannot be changed while a loop goes over it
 --> main.byn:7:9
  |
7 |         values.append(v * 2.0)
  |         ^^^^^^ changed here
 ::: main.byn:6:14
  |
6 |     for v in values:
  |              ------ the loop goes over `values` here
  = note: loop over the indices instead: `for i in range(len(values)):`

error: `values` cannot be changed while a loop goes over it
 --> main.byn:8:9
  |
8 |         values[0] = 1.0
  |         ^^^^^^^^^ changed here
 ::: main.byn:6:14
  |
6 |     for v in values:
  |              ------ the loop goes over `values` here
  = note: loop over the indices instead: `for i in range(len(values)):`

error: `rows` cannot be changed while a loop goes over it
  --> main.byn:11:13
   |
11 |             rows.append(row.copy())
   |             ^^^^ changed here
  ::: main.byn:9:16
   |
 9 |     for row in rows:
   |                ---- the loop goes over `rows` here
   = note: loop over the indices instead: `for i in range(len(rows)):`

error: `row` borrows an element of `rows`, so it cannot be changed
  --> main.byn:12:9
   |
12 |         row[0] = 1.0
   |         ^^^^^^ changed here
   = note: work on a copy, `row.copy()`, or loop over the indices to change the element: `for i in range(len(rows)):`

error: `row` borrows an element of `rows`, so it cannot be moved
  --> main.byn:13:16
   |
13 |         kept = row
   |                ^^^ moved here
   = note: to keep the element, keep a copy: `row.copy()`

error: `rows` cannot be moved while a loop goes over it
  --> main.byn:15:14
   |
15 |         keep(rows)
   |              ^^^^ moved here
  ::: main.byn:14:16
   |
14 |     for row in rows:
   |                ---- the loop goes over `rows` here
   = note: loop over the indices instead: `for i in range(len(rows)):`"
    );
}
