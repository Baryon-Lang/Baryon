//! Element-wise comparisons of arrays (roadmap 3.13.1), built and run: `a > b` gives an array
//! of `bool`s, `&`, `|`, `^` and `~` combine such arrays, and every line prints what `NumPy`
//! prints for the same program.

use super::{baryon_build, package, panic_report, stderr};
use std::io;
use std::process::Command;

/// Builds `program` as the package `name`, runs it, and returns its exit code, standard output
/// and panic report.
fn run(name: &str, program: &str) -> io::Result<(Option<i32>, String, String)> {
    let directory = package(name, &[("main.byn", program)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output()?;
    Ok((
        run.status.code(),
        String::from_utf8_lossy(&run.stdout).into_owned(),
        panic_report(&run),
    ))
}

/// An array against a number, against an array of another element type, combined with `&`,
/// `|`, `^` and `~`, broadcast against a row, on a `Fixed` array, an expression on each side,
/// and `~` of integers: every line is what `NumPy` 2.5 prints for the same steps.
#[test]
fn comparisons_print_what_numpy_prints() {
    let program = "\
Vec3 = Fixed[f64, (3,)]


def main():
    prices = Array([189.5, 410.0, 190.25, 171.0])
    volume = Array([100, 250, 300, 50])
    print(volume > 75)
    both = (prices > 180.0) & (volume >= 250)
    print(both)
    print(~both)
    print(volume > 99.5)
    print(prices == 410.0)
    print((prices > 180.0) | ~(volume != 50))
    print((prices < 180.0) ^ (volume < 200))
    print(prices * 2.0 <= volume + 100)
    print(100 < volume)
    grid: Array[f64, 2] = Array.zeros((2, 3))
    grid[1, 2] = 400.0
    print(grid > prices[:3])
    print(Vec3(1.0, 5.0, 3.0) > 2.0)
    print(~volume)
";
    let (code, stdout, report) = run("comparisons-numpy", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(
        stdout,
        "\
[ True  True  True False]
[False  True  True False]
[ True False False  True]
[ True  True  True False]
[False  True False False]
[ True  True  True  True]
[ True False False False]
[False False  True False]
[False  True  True False]
[[False False False]
 [False False  True]]
[False  True  True]
[-101 -251 -301  -51]
"
    );
}

/// Arrays whose shapes do not broadcast panic at the comparison, as whole-array arithmetic
/// does.
#[test]
fn a_comparison_of_shapes_that_do_not_broadcast_panics() {
    let program = "\
def main():
    a = Array([1.0, 2.0, 3.0])
    b = Array([1.0, 2.0])
    print(a > b)
";
    let (code, stdout, report) = run("comparisons-shapes", program).unwrap();
    assert_eq!((code, stdout.as_str()), (Some(101), ""));
    assert_eq!(
        report,
        "panic: array lengths differ in dimension 0: 3 and 2\n  --> main.byn:4:11\n"
    );
}

/// `a[mask]` and `a[positions]` are new arrays of the chosen elements, of numbers, of structs
/// and in a generic function; a choice from a choice, from a slice, as an argument, as a loop's
/// sequence and assigned back to its own array; and `a[mask] = value` and
/// `a[positions] = value` set the chosen elements. Every line is what `NumPy` 2.5 prints.
#[test]
fn chosen_elements_print_what_numpy_prints() {
    let program = "\
class Point(Struct, Copy):
    x: f64
    y: f64


def pick[T: Copy](a: Array[T, 1], rows: Array[i64, 1]) -> Array[T, 1]:
    return a[rows]


def clip(a: InOut[Array[f64, 1]]):
    a[a < 0.0] = 0


def main():
    prices = Array([189.5, 410.0, 190.25, 171.0])
    volume = Array([100, 250, 300, 50])
    print(prices[volume > 75])
    order = Array([3, 0, 0, 2])
    print(prices[order])
    print(volume[order][volume[order] > 60])
    print(len(volume[volume > 1000]))
    print(pick(volume, order))
    points = Array([Point(1.0, 2.0), Point(3.0, 4.0)])
    kept = points[Array([False, True])]
    print(len(kept), kept[0].x)
    print(prices[prices > 180.0].sum())
    print(volume[1:][Array([True, False, True])])
    for v in volume[volume > 60]:
        print(v)
    prices = prices[prices < 200.0]
    print(prices)
    signed = Array([1.0, -2.0, 3.0, -4.0])
    clip(signed)
    print(signed)
    signed[Array([0, 3])] = signed[2] * 2.0
    print(signed)
";
    let (code, stdout, report) = run("comparisons-chosen", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(
        stdout,
        "\
[189.5  410.   190.25]
[171.   189.5  189.5  190.25]
[100 100 300]
0
[ 50 100 100 300]
1 3.0
789.75
[250  50]
100
250
300
[189.5  190.25 171.  ]
[1. 0. 3. 0.]
[6. 0. 3. 6.]
"
    );
}

/// A position past the end, a negative position and a mask of another length panic, each
/// naming what was out of range.
#[test]
fn a_choice_out_of_range_panics() {
    let cases = [
        (
            "Array([5])",
            "panic: index 5 is out of range for dimension 0 of length 4\n  --> main.byn:3:13\n",
        ),
        (
            "Array([0, -1])",
            "panic: index -1 is out of range for dimension 0 of length 4\n  --> main.byn:3:13\n",
        ),
        (
            "Array([True, False])",
            "panic: array lengths differ in dimension 0: 4 and 2\n  --> main.byn:3:13\n",
        ),
    ];
    for (index, (choice, expected)) in cases.into_iter().enumerate() {
        let program =
            format!("def main():\n    a = Array([1.0, 2.0, 3.0, 4.0])\n    print(a[{choice}])\n");
        let name = format!("comparisons-range-{index}");
        let (code, stdout, report) = run(&name, &program).unwrap();
        assert_eq!((code, stdout.as_str()), (Some(101), ""));
        assert_eq!(report, expected);
    }
}
