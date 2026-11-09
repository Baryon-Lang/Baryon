//! The example programs in `docs/examples` built and run, each compared with what it prints, so
//! an example that builds today keeps building. Roadmap item 1.9.12 adds each example here as
//! lowering learns what it needs; `docs/reference/status.md` lists the ones still missing.

use super::{baryon_build, package, panic_report, stderr};
use std::io;
use std::process::Command;

/// Builds the program `text` as the `main.byn` of the package `name`, runs it, and checks that
/// it prints `expected`, nothing else, and exits with status 0.
///
/// # Errors
///
/// When writing the package or starting `baryon` or the program fails.
pub(super) fn prints(name: &str, text: &str, expected: &str) -> io::Result<()> {
    let directory = package(name, &[("main.byn", text)])?;
    let (output, program) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output()?;
    assert_eq!(stderr(&ran), "");
    assert_eq!(String::from_utf8_lossy(&ran.stdout), expected);
    assert_eq!(ran.status.code(), Some(0));
    Ok(())
}

/// The 3D heat equation: a stencil in a `prange` loop, `copy()`, a swap by unpacking and
/// `sum()`. `NumPy`, running the same steps, prints the same total.
#[test]
fn heat_equation_builds_and_runs() {
    prints(
        "example-heat-equation",
        include_str!("../../../../docs/examples/heat_equation.byn"),
        "heat remaining after 100 steps: 1000.000000\n",
    )
    .unwrap();
}

/// Unpacking a tuple, a tuple display and a `Fixed` vector; a copy that changes apart from its
/// original; and sums of a 2D array, an integer array, a `Fixed` vector and an empty array.
#[test]
fn unpacking_copy_and_sum_build_and_run() {
    let program = "\
Vec3 = Fixed[f64, (3,)]


def main():
    a = Array.zeros((2, 3))
    a[0, 0] = 1.0
    a[1, 2] = 4.5
    b = a.copy()
    b[0, 1] = 10.0
    print(a.sum(), b.sum())
    short = Array.ones((2,))
    a_row = Array.zeros((5,))
    short, a_row = a_row, short
    print(len(short), len(a_row))
    x, y = 3, 4.5
    print(x, y)
    p, q, r = Vec3(1.0, 2.0, 3.0)
    print(p, q, r, Vec3(1.0, 2.0, 3.0).sum())
    pair = (Array.fill((4,), 2), 7)
    twos, n = pair
    print(twos.sum(), n, Array.zeros((0,)).sum())
";
    prints(
        "unpacking-copy-sum",
        program,
        "5.5 15.5\n5 2\n3 4.5\n1.0 2.0 3.0 6.0\n8 7 0.0\n",
    )
    .unwrap();
}

/// `**` on floats, `**=`, and `dot` and `round` of a `Fixed` vector, through `llvm.pow` and
/// `llvm.roundeven`: ties round to even, so -1.25 gives -1.0 and 0.75 gives 1.0.
#[test]
fn powers_dot_and_round_build_and_run() {
    let program = "\
Vec3 = Fixed[f64, (3,)]


def main():
    v = Vec3(1.5, -2.5, 3.0)
    w = (v / 2.0).round()
    x = 2.0
    print(x ** 3, 2.0 ** 0.5, v.dot(v), w[0], w[1], w[2], Vec3(2.5, 3.5, -0.5).round()[0])
    x **= 2.0
    print(x)
";
    prints(
        "powers-dot-round",
        program,
        "8.0 1.4142135623730951 17.5 1.0 -1.0 2.0 2.0\n4.0\n",
    )
    .unwrap();
}

/// Lennard-Jones molecular dynamics: `Fixed` vector arithmetic, `dot`, `round`, a `prange`
/// force loop and `math.sqrt`. An independent `NumPy` run of the same steps prints the same
/// energies, and the total stays constant to four digits, the standard check of the integrator.
#[test]
fn lennard_jones_builds_and_runs() {
    prints(
        "example-lennard-jones",
        include_str!("../../../../docs/examples/lennard_jones.byn"),
        "\
step   50  kinetic    10.8151  potential -1991.7744  total -1980.9594  temperature 0.0141  rms speed 0.2055
step  100  kinetic     3.6545  potential -1984.6125  total -1980.9581  temperature 0.0048  rms speed 0.1195
step  150  kinetic     7.8174  potential -1988.7601  total -1980.9426  temperature 0.0102  rms speed 0.1747
step  200  kinetic     6.0588  potential -1987.0009  total -1980.9420  temperature 0.0079  rms speed 0.1538
",
    )
    .unwrap();
}

/// The `math` functions, called as `math.exp` and imported as `sqrt`, give the bits Python's
/// `math` module gives. An integer argument converts to `f64`, and an `f32` stays an `f32`.
#[test]
fn math_functions_build_and_run() {
    let program = "\
import math
from math import sqrt


def main():
    n = 2
    x: f32 = 2.0
    print(sqrt(n), math.exp(1.0), math.log(10), math.sin(0.5), math.cos(0.5))
    print(math.pow(2, 10), math.pow(x, 0.5), sqrt(x), math.log(0.0), math.sqrt(-1.0))
";
    prints(
        "math-functions",
        program,
        "1.4142135623730951 2.718281828459045 2.302585092994046 0.479425538604203 \
         0.8775825618903728\n1024.0 1.4142135 1.4142135 -inf nan\n",
    )
    .unwrap();
}

/// The built-in `round` gives an `int`, rounding ties to even as Python does, so 2.5 gives 2
/// and -0.5 gives 0. An integer is returned as it is.
#[test]
fn round_builds_and_runs() {
    let program = "\
def main():
    print(round(2.5), round(3.5), round(-0.5), round(-1.5), round(7), round(10.0 / 0.001))
";
    prints("round", program, "2 4 0 -2 7 10000\n").unwrap();
}

/// `a.sum()` of floats adds in `NumPy`'s pairwise tree, so it prints the bits `numpy.sum` gives:
/// the harmonic sum to 1000 is `7.485470860550345` both ways, where adding one element at a time
/// gives `7.485470860550343`. `f32` sums keep `f32` arithmetic, as `NumPy` does.
#[test]
fn float_sums_match_numpy() {
    let program = "\
def main():
    a = Array.zeros((1000,))
    b: Array[f32, 1] = Array.zeros((300,))
    for i in range(1000):
        a[i] = 1.0 / f64(i + 1)
    for i in range(300):
        b[i] = f32(1.0) / f32(i + 1)
    print(a.sum(), b.sum())
";
    prints("float-sums", program, "7.485470860550345 6.282665\n").unwrap();
}

/// The Lorenz system with fourth-order Runge-Kutta: a function passed as an argument, compiled
/// into one copy of the generic step per function (decision 0026), and `VectorSpace`
/// arithmetic on a `Fixed` vector. The trajectory is chaotic, so it amplifies any difference in
/// rounding; a `NumPy` run of the same steps in the same order prints the same digits.
#[test]
fn lorenz_rk4_builds_and_runs() {
    prints(
        "example-lorenz-rk4",
        include_str!("../../../../docs/examples/lorenz_rk4.byn"),
        "\
t =  10.0  x =   -4.9027  y =   -3.7439  z =   24.6909
t =  20.0  x =   13.7932  y =   12.9518  z =   34.9016
t =  30.0  x =    6.6853  y =    1.3161  z =   31.1718
t =  40.0  x =    5.2599  y =    1.7159  z =   28.1218
",
    )
    .unwrap();
}

/// Functions passed as arguments: by name, through a local that holds one, and to a generic
/// function. The LLVM backend refuses a call through a function value, so building at all shows
/// each call became a direct one.
#[test]
fn functions_as_arguments_build_and_run() {
    let program = "\
def square(x: f64) -> f64:
    return x * x


def cube(x: f64) -> f64:
    return x * x * x


def apply_twice(f: Callable[[f64], f64], x: f64) -> f64:
    return f(f(x))


def scaled[T: Float](f: Callable[[T], T], x: T, k: T) -> T:
    return k * f(x)


def main():
    g = cube
    print(apply_twice(square, 3.0), apply_twice(g, 2.0), scaled(square, 3.0, 0.5))
";
    prints("functions-as-arguments", program, "81.0 512.0 4.5\n").unwrap();
}

/// Two `Callable` parameters of one signature each call the function passed to them, not both
/// the first: the instances for `(square, half)` and `(half, square)` are distinct functions.
#[test]
fn callable_parameters_of_one_signature_stay_apart() {
    let program = "\
def square(x: f64) -> f64:
    return x * x


def half(x: f64) -> f64:
    return x / 2.0


def compose(f: Callable[[f64], f64], g: Callable[[f64], f64], x: f64) -> f64:
    return g(f(x))


def main():
    print(compose(square, half, 3.0), compose(half, square, 3.0))
";
    prints("callables-of-one-signature", program, "4.5 2.25\n").unwrap();
}

/// A `VectorSpace` function at a `Fixed` vector and at `f64`: scaling by an `f64` applies to
/// every element of the vector, as `NumPy` broadcasting does.
#[test]
fn vector_space_arithmetic_builds_and_runs() {
    let program = "\
Vec3 = Fixed[f64, (3,)]


def step[S: VectorSpace](state: S, h: f64) -> S:
    return state + h / 2 * state - state


def main():
    v = step(Vec3(1.0, 2.0, 3.0), 0.5)
    print(v[0], v[1], v[2], step(2.0, 0.5))
";
    prints("vector-space", program, "0.25 0.5 0.75 0.5\n").unwrap();
}

/// Conjugate gradient on the 5-point Laplacian of a 64 x 64 grid: a matrix built from lists in
/// compressed sparse row form, a struct method with a `prange` loop, and a function that can
/// raise. A `NumPy` run of the same steps converges after 142 iterations to the same value.
#[test]
fn conjugate_gradient_builds_and_runs() {
    prints(
        "example-conjugate-gradient",
        include_str!("../../../../docs/examples/conjugate_gradient.byn"),
        "converged; value at the centre: 311.078468\n",
    )
    .unwrap();
}

/// Lists: `append` grows the buffer past its first 4 elements, a display converts its items
/// to the element type, an element can be read from the end and written in place, and
/// `Array(xs)` copies a list, an empty one too, into an array.
#[test]
fn lists_build_and_run() {
    let program = "\
def main():
    squares: list[i64] = []
    for i in range(10):
        squares.append(i * i)
    print(len(squares), squares[0], squares[9], squares[-1])
    weights: list[f64] = [1, 2.5]
    weights.append(4)
    weights[0] = 0.5
    total = Array(weights).sum()
    print(len(weights), weights[0], total, Array(squares).sum())
    empty: list[f32] = []
    print(len(empty), len(Array(empty)))
    flags = [True, False]
    flags.append(True)
    print(flags[2], len(flags))
";
    prints("lists", program, "10 0 81 81\n3 0.5 7.0 285\n0 0\nTrue 3\n").unwrap();
}

/// Lists of values that own memory (roadmap 1.13.3): `str`s, structs holding a `str` and an
/// array, arrays and lists, built by a display or `append`, returned from a function, read by
/// index, and written in place, which drops the element replaced. Dropping a list drops each
/// element; a leak checker such as Valgrind finds nothing lost.
#[test]
fn lists_of_owned_values_build_and_run() {
    let program = r#"
class Sample(Struct):
    name: str
    values: Array[f64, 1]


def names(n: int) -> list[str]:
    result: list[str] = []
    for i in range(n):
        result.append(f"probe{i}")
    return result


def main():
    xs = names(3)
    print(len(xs), xs[0], xs[2])
    xs[1] = "replaced"
    print(xs[1], len(xs[1]))
    words = ["alpha", "beta"]
    words.append(xs[0].copy())
    print(words[2], len(words))
    samples: list[Sample] = []
    samples.append(Sample("a", Array.zeros(4)))
    samples.append(Sample("b", Array.ones(2)))
    print(samples[1].name, samples[1].values.sum(), samples[0].values[3])
    grid = [Array.ones(3), Array.zeros(5)]
    print(grid[0].sum(), len(grid[1]))
    nested = [[1, 2], [3]]
    nested[1].append(4)
    print(nested[1][1], len(nested))
"#;
    prints(
        "owned-lists",
        program,
        "3 probe0 probe2\nreplaced 8\nprobe0 3\nb 2.0 0.0\n3.0 5\n4 2\n",
    )
    .unwrap();
}

/// `in`, `not in` and `copy()` on lists (roadmap 1.13.3): `in` converts the value to the element
/// type, so `2 in xs` finds `2.0`, and works on numbers, `str`s and enums; a copy is a new list,
/// so changing one leaves the other alone. Python prints the same lines.
#[test]
fn list_membership_and_copies_build_and_run() {
    let program = r#"

class Phase(Enum):
    SOLID = 1
    GAS = 2


def main():
    xs = [1.0, 2.0, 3.0]
    ys = xs.copy()
    ys[0] = 9.0
    print(2 in xs, 5.0 not in ys, 9.0 in xs, 9.0 in ys, xs[0], len(ys))
    names = ["argon", "neon"]
    copies = names.copy()
    names[0] = "xenon"
    print("neon" in names, "argon" in names, "argon" in copies, "krypton" not in copies)
    phases = [Phase.SOLID]
    empty: list[int] = []
    print(Phase.GAS in phases, Phase.SOLID in phases, 0 in empty, len(empty.copy()))
"#;
    prints(
        "list-membership",
        program,
        "True True False True 1.0 3\nTrue False True True\nFalse True False 0\n",
    )
    .unwrap();
}

/// Monte Carlo pi: 1,024 chunks of a `prange` loop, each drawing from its own `Philox` stream.
/// `NumPy`, drawing from `Generator(Philox(key=2026).jumped(chunk))` for each chunk, counts the
/// same 80,420,174 hits, so the estimate matches to every digit, on any number of cores.
#[test]
fn monte_carlo_pi_builds_and_runs() {
    prints(
        "example-monte-carlo-pi",
        include_str!("../../../../docs/examples/monte_carlo_pi.byn"),
        "pi is about 3.141413 (error -0.000180, expected 0.000162)\n",
    )
    .unwrap();
}

/// `Philox` draws the floats `NumPy`'s `Generator(Philox(key=2026))` draws, across the end of
/// its first block of four, and stream 5 starts where `jumped(5)` does. The constants of `math`
/// print as Python prints them.
#[test]
fn philox_matches_numpy() {
    let program = "\
import math
from random import Philox


def main():
    generator = Philox(2026)
    print(generator.uniform(), generator.uniform(), generator.uniform(), generator.uniform())
    print(generator.uniform(), generator.uniform())
    fifth = Philox(2026, stream=5)
    print(fifth.uniform())
    print(math.pi, math.e, math.tau, math.inf, -math.inf, math.nan)
";
    prints(
        "philox",
        program,
        "0.422034180346073 0.34249750953111324 0.11181606179138992 0.956212079298963\n\
         0.5517286838670085 0.7040319383561433\n\
         0.1452474629941255\n\
         3.141592653589793 2.718281828459045 6.283185307179586 inf -inf nan\n",
    )
    .unwrap();
}

/// A negative seed panics where `Philox` is called, as `NumPy` rejects a negative key.
#[test]
fn a_negative_philox_seed_panics() {
    let directory = package(
        "philox-negative",
        &[(
            "main.byn",
            "from random import Philox\n\n\ndef main():\n    seed = -1\n    Philox(seed)\n",
        )],
    )
    .unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(ran.status.code(), Some(101));
    assert_eq!(
        panic_report(&ran),
        "panic: a `Philox` seed and stream must not be negative\n  --> main.byn:6:5\n"
    );
}
