//! Contour plots (roadmap 3.15.5, decision 0046), built and run: `byplot`'s `contour` and
//! `contourf` choose levels as matplotlib 3.11 does and draw the same lines and bands.
//!
//! The files in `tests/fixtures/plotting` were recorded from matplotlib or written by these
//! programs; its `README.md` says which and how.

use super::{Scratch, baryon_build, package, panic_report, stderr};
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Output};

/// The directory of recorded ticks and reference plots.
fn fixtures() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/plotting"
    ))
}

/// Builds `program` as the package `name`, copies `ranges.txt` beside it, runs it there, and
/// returns what it printed and the package's directory, which is removed when it is dropped.
///
/// # Errors
///
/// Fails when the package cannot be written or the program cannot be run.
fn run(name: &str, program: &str) -> io::Result<(Output, Scratch)> {
    let directory = package(name, &[("main.byn", program)])?;
    fs::copy(fixtures().join("ranges.txt"), directory.join("ranges.txt"))?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&executable).current_dir(&directory).output()?;
    Ok((ran, directory))
}

/// matplotlib picks contour levels with `MaxNLocator(8)` and its default steps, 1, 1.5, 2, 2.5,
/// 3, 4, 5, 6, 8 and 10; for each of 310 ranges, `ticker.MaxNLocator` with those steps gives
/// the same levels, bit for bit, as `matplotlib_contour_ticks.txt` recorded them.
#[test]
fn contour_levels_are_placed_as_matplotlib_does() {
    let program = "\
from ticker import MaxNLocator


def main() -> Raises[None, OSError | ValueError]:
    locator = MaxNLocator(8, [1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0])
    with open(\"ranges.txt\") as f:
        for line in f:
            parts = line.split()
            ticks = locator.tick_values(float(parts[0]), float(parts[1]))
            print(\" \".join([f\"{t}\" for t in ticks]))
";
    let (ran, _directory) = run("plotting-contour-levels", program).unwrap();
    assert_eq!(stderr(&ran), "");
    let expected = fs::read_to_string(fixtures().join("matplotlib_contour_ticks.txt")).unwrap();
    assert_eq!(String::from_utf8_lossy(&ran.stdout), expected);
}

/// Contours drawn as the stored reference files: filled bands with black lines over them, the
/// negative levels dashed, at the levels matplotlib picks for a dipole with a tilt; and a grid
/// of saddles, where each cell whose corners alternate about a level is split as matplotlib
/// splits it, by the mean of its corners, with levels given. Both were checked against
/// matplotlib's drawing of the same grids, and the band colors are its colors to the byte.
#[test]
fn contours_are_drawn_as_the_reference_files() {
    let program = "\
from math import cos, exp
import byplot as plt


def main() -> Raises[None, OSError | ValueError]:
    x = Array.zeros(21)
    y = Array.zeros(16)
    for j in range(21):
        x[j] = -2.0 + 0.2 * f64(j)
    for i in range(16):
        y[i] = -1.5 + 0.2 * f64(i)
    z = Array.zeros((16, 21))
    for i in range(16):
        for j in range(21):
            z[i, j] = x[j] * exp(-x[j] * x[j] - y[i] * y[i]) + 0.1 * x[j] * y[i]
    ax = plt.axes()
    ax.contourf(x, y, z)
    ax.contour(x, y, z, colors=\"k\")
    ax.savefig(\"contours.svg\")
    s = Array.zeros(9)
    for j in range(9):
        s[j] = 0.4 * f64(j)
    w = Array.zeros((9, 9))
    for i in range(9):
        for j in range(9):
            w[i, j] = cos(3.0 * s[j]) * cos(3.0 * s[i]) + 0.1
    b = plt.axes()
    b.contourf(s, s, w, levels=Array([-1.0, -0.5, 0.0, 0.5, 1.0, 1.5]))
    b.contour(s, s, w, levels=Array([-0.5, 0.0, 0.5]), colors=\"k\")
    b.savefig(\"saddles.svg\")
";
    let (ran, directory) = run("plotting-contours", program).unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(ran.status.code(), Some(0));
    for name in ["contours.svg", "saddles.svg"] {
        let drawn = fs::read_to_string(directory.join(name)).unwrap();
        let reference = fs::read_to_string(fixtures().join(name)).unwrap();
        assert_eq!(drawn, reference, "{name} differs from the reference file");
    }
}

/// A grid that does not match its coordinates, levels that do not rise, and `contourf` with
/// one level each panic, where matplotlib raises `TypeError` or `ValueError`.
#[test]
fn wrong_grids_and_levels_panic() {
    let cases = [
        (
            "plotting-contour-grid",
            "plt.axes().contour(Array.zeros(3), Array.zeros(2), Array.zeros((2, 2)))",
            "panic: assertion failed: len(x) == columns, \"contour needs len(x) == z.shape[1]\"",
        ),
        (
            "plotting-contour-order",
            "plt.axes().contour(Array.zeros(2), Array.zeros(2), Array.zeros((2, 2)), \
             levels=Array([1.0, 0.0]))",
            "panic: assertion failed: chosen[k] > chosen[k - 1], \"Contour levels must be",
        ),
        (
            "plotting-contour-bands",
            "plt.axes().contourf(Array.zeros(2), Array.zeros(2), Array.zeros((2, 2)), \
             levels=Array([1.0]))",
            "panic: assertion failed: not filled or len(chosen) >= 2",
        ),
    ];
    for (name, call, report) in cases {
        let program = format!("import byplot as plt\n\n\ndef main():\n    {call}\n");
        let (ran, _directory) = run(name, &program).unwrap();
        assert_eq!(ran.status.code(), Some(101), "{name}");
        assert!(
            panic_report(&ran).starts_with(report),
            "{}",
            panic_report(&ran)
        );
    }
}
