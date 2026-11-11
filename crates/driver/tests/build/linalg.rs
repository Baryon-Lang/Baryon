//! The `linalg` module (roadmap 3.10, decision 0040), built and run: each function's results
//! against `NumPy`'s for the same input. Random inputs come from `random.Philox`, which draws
//! `NumPy`'s numbers bit for bit, so `NumPy` solves the very same system. Results are printed to 12
//! significant digits, since `LAPACK`, under `NumPy`, adds in its own order.

use super::examples::prints;
use super::{baryon_build, package, panic_report, stderr};
use std::process::Command;

/// `solve` of a system that needs row swaps, of a 1 × 1 system, imported as `linalg.solve`
/// too, and of a random 50 × 50 system; and `LinAlgError("Singular matrix")` for a zero matrix
/// and for one whose elimination meets a zero pivot. `NumPy` prints the same lines, from
///
/// ```python
/// g = numpy.random.Generator(numpy.random.Philox(key=2026))
/// v = g.random(50 * 51).reshape(50, 51)
/// x = numpy.linalg.solve(v[:, 1:], v[:, 0])
/// ```
#[test]
fn solve_matches_numpy() {
    let program = r#"
import linalg
from linalg import LinAlgError, solve
from random import Philox


def random_system(n: int, seed: int) -> tuple[Array[f64, 2], Array[f64, 1]]:
    generator = Philox(seed)
    a = Array.zeros((n, n))
    b = Array.zeros(n)
    for i in range(n):
        b[i] = generator.uniform()
        for j in range(n):
            a[i, j] = generator.uniform()
    return a, b


def show(x: Array[f64, 1]):
    for value in x:
        print(f"{value:.12g}", end=" ")
    print()


def main() -> Raises[None, LinAlgError]:
    pivoting = Array.zeros((4, 4))
    rows = [[0.0, 2.0, 1.0, -1.0], [3.0, -1.0, 0.5, 2.0], [1.0, 1.0, 1.0, 1.0], [-2.0, 4.0, 0.0, 3.0]]
    for i in range(4):
        for j in range(4):
            pivoting[i, j] = rows[i][j]
    show(solve(pivoting, Array([1.0, -2.0, 0.5, 4.0])))
    one = Array.fill((1, 1), 4.0)
    show(linalg.solve(one, Array([2.0])))
    a, b = random_system(50, 2026)
    x = solve(a, b)
    print(f"{x[0]:.12g} {x[25]:.12g} {x[49]:.12g}")
    residual = a @ x - b
    print(residual.dot(residual) < 1e-24)
    try:
        solve(Array.zeros((3, 3)), Array([1.0, 2.0, 3.0]))
    except LinAlgError as error:
        print(error.message)
    singular = Array.fill((2, 2), 1.0)
    try:
        solve(singular, Array([1.0, 2.0]))
    except LinAlgError as error:
        print(error.message)
"#;
    prints(
        "linalg-solve",
        program.trim_start(),
        "-0.964285714286 0.25 0.857142857143 0.357142857143 \n\
         0.5 \n\
         -0.600335590569 2.50291194375 1.72623643638\n\
         True\n\
         Singular matrix\n\
         Singular matrix\n",
    )
    .unwrap();
}

/// `lu_factor` and `lu_solve`, `det` and `inv` of a random 5 × 5 matrix; `det` of a singular
/// matrix, `0.0`, and of an empty one, `1.0`; and `LinAlgError` from `inv` of a singular matrix.
/// `scipy.linalg.lu_factor`, `lu_solve` and `numpy.linalg.det` and `inv` print the same lines
/// for the matrix
///
/// ```python
/// numpy.random.Generator(numpy.random.Philox(key=11)).random(25).reshape(5, 5) - 0.5
/// ```
#[test]
fn lu_det_and_inv_match_numpy_and_scipy() {
    let program = r#"
from linalg import LinAlgError, det, inv, lu_factor, lu_solve
from random import Philox


def show(x: Array[f64, 1]):
    for value in x:
        print(f"{value:.12g}", end=" ")
    print()


def main() -> Raises[None, LinAlgError]:
    generator = Philox(11)
    n = 5
    a = Array.zeros((n, n))
    for i in range(n):
        for j in range(n):
            a[i, j] = generator.uniform() - 0.5
    lu, piv = lu_factor(a)
    print(piv)
    show(lu[n - 1, :])
    show(lu_solve(lu, piv, Array([1.0, 2.0, 3.0, 4.0, 5.0])))
    print(f"{det(a):.12g}", det(Array.fill((3, 3), 2.0)), det(Array.zeros((0, 0))))
    inverse = inv(a)
    show(inverse[0, :])
    show(inverse[:, 4])
    try:
        inv(Array.fill((2, 2), 1.0))
    except LinAlgError as error:
        print(error.message)
"#;
    prints(
        "linalg-lu",
        program.trim_start(),
        "[1 2 4 4 4]\n\
         -0.0212000958787 0.258045423418 0.249879688839 0.494044049809 -0.383324095346 \n\
         7.67510739253 37.1622007148 -23.6703267158 4.86295616996 -4.12915577324 \n\
         0.00429322797388 0.0 1.0\n\
         -4.87824634096 1.99769052254 -3.55898033704 3.13562267707 1.33848459825 \n\
         1.33848459825 4.19714419578 -2.25016069937 -0.281973760125 0.861864295096 \n\
         Singular matrix\n",
    )
    .unwrap();
}

/// A right-hand side whose length is not the matrix's panics, as mismatched shapes in
/// whole-array arithmetic do, pointing into the library and naming the caller's line.
#[test]
fn solve_with_a_short_right_hand_side_panics() {
    let program = "\
from linalg import LinAlgError, solve


def main() -> Raises[None, LinAlgError]:
    print(solve(Array.zeros((3, 3)), Array.zeros(2)))
";
    let directory = package("linalg-solve-short", &[("main.byn", program)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&executable).output().unwrap();
    assert_eq!(ran.status.code(), Some(101));
    assert_eq!(
        panic_report(&ran),
        "panic: assertion failed: len(b) == a.shape[0], \"solve needs b as long as a has rows\"\n  \
         --> <std>/linalg.byn:93:5\n"
    );
}

/// A library function is compiled only into a program that calls it: a program that never
/// calls `solve` has no `solve` in its module, so the library costs it nothing.
#[test]
fn library_functions_are_compiled_only_when_called() {
    let compiled = |program: &str| {
        let mut session = driver::Session::new();
        session.add_file("main.byn", program).unwrap();
        session.compile(None, driver::Profile::Debug).unwrap()
    };
    let unused = compiled("def main():\n    print(1)\n");
    assert!(!unused.contains("<std>.linalg.solve"), "{unused}");
    let used = compiled(
        "from linalg import LinAlgError, solve\n\n\n\
         def main() -> Raises[None, LinAlgError]:\n    print(solve(Array.fill((1, 1), 2.0), Array([1.0])))\n",
    );
    assert!(used.contains("@\"<std>.linalg.solve\""), "{used}");
}

/// The worked example of `linalg.solve` in the reference's Linear algebra chapter, read from
/// the chapter itself so the two cannot drift apart, prints what the chapter says it prints.
#[test]
fn the_reference_example_of_solve_runs() {
    let chapter = include_str!("../../../../docs/reference/linear-algebra.md");
    let section = &chapter[chapter.find("## `linalg.solve`").unwrap()..];
    let program = section
        .split("```python\n")
        .nth(2)
        .and_then(|block| block.split("```").next())
        .unwrap();
    let printed = section
        .split("```text\n")
        .nth(1)
        .and_then(|block| block.split("```").next())
        .unwrap();
    prints("linalg-reference-solve", program, printed).unwrap();
}
