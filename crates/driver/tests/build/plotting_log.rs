//! Logarithmic axes (roadmap 3.15.4, decision 0046), built and run: `ticker`'s `LogLocator` and
//! `LogFormatterSciNotation` place and label ticks as matplotlib 3.11 does, to within the last
//! bit of a tick, and `pyplot` draws log axes, with minor ticks and raised exponents, as the
//! reference files.
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

/// Builds `program` as the package `name`, copies `log_ranges.txt` beside it, runs it there,
/// and returns what it printed and the package's directory, which is removed when it is
/// dropped.
///
/// # Errors
///
/// Fails when the package cannot be written or the program cannot be run.
fn run(name: &str, program: &str) -> io::Result<(Output, Scratch)> {
    let directory = package(name, &[("main.byn", program)])?;
    fs::copy(
        fixtures().join("log_ranges.txt"),
        directory.join("log_ranges.txt"),
    )?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&executable).current_dir(&directory).output()?;
    Ok((ran, directory))
}

/// For each of 176 ranges, from a tenth of a decade to sixty decades wide, `LogLocator` gives
/// the major and minor ticks matplotlib gives, bit for bit, with room for 9 ticks and for 3,
/// and `LogFormatterSciNotation` the same mathtext labels, `""` for a tick left unlabelled.
/// Each range takes five lines: major ticks, their labels, minor ticks, their labels, and major
/// ticks with room for 3. A tick may differ from matplotlib's in the last bit: `NumPy` raises 10
/// to a power with a vectorized `pow` on some processors, which is not always correctly
/// rounded, so matplotlib's own ticks differ by machine.
#[test]
fn log_ticks_are_placed_and_labelled_as_matplotlib_does() {
    let program = "\
from ticker import LogFormatterSciNotation, LogLocator


def main() -> Raises[None, OSError | ValueError]:
    with open(\"log_ranges.txt\") as f:
        for line in f:
            parts = line.split()
            low = float(parts[0])
            high = float(parts[1])
            formatter = LogFormatterSciNotation(low, high)
            for auto in [False, True]:
                subs: list[f64] = []
                if not auto:
                    subs.append(1.0)
                ticks = LogLocator(subs, 9).tick_values(low, high)
                print(\" \".join([f\"{t}\" for t in ticks]))
                print(\"|\".join(formatter.format_ticks(ticks)))
            print(\" \".join([f\"{t}\" for t in LogLocator([1.0], 3).tick_values(low, high)]))
";
    let (ran, _directory) = run("plotting-log-ticks", program).unwrap();
    assert_eq!(stderr(&ran), "");
    let expected = fs::read_to_string(fixtures().join("matplotlib_log_ticks.txt")).unwrap();
    let printed = String::from_utf8_lossy(&ran.stdout);
    for (number, (got, want)) in printed.lines().zip(expected.lines()).enumerate() {
        if number % 5 == 1 || number % 5 == 3 || want.is_empty() {
            assert_eq!(got, want, "line {} of matplotlib_log_ticks.txt", number + 1);
            continue;
        }
        let ours: Vec<f64> = got.split(' ').map(|t| t.parse().unwrap()).collect();
        let theirs: Vec<f64> = want.split(' ').map(|t| t.parse().unwrap()).collect();
        assert_eq!(
            ours.len(),
            theirs.len(),
            "line {} of matplotlib_log_ticks.txt",
            number + 1
        );
        for (a, b) in ours.iter().zip(&theirs) {
            let apart = (a.to_bits().cast_signed() - b.to_bits().cast_signed()).abs();
            assert!(apart <= 1, "line {}: {a} and {b}", number + 1);
        }
    }
    assert_eq!(printed.lines().count(), expected.lines().count());
}

/// Logarithmic axes drawn as the stored reference files: a line through five decades of x and
/// ten of y on `set_xscale("log")` and `set_yscale("log")`, ranges found as matplotlib finds
/// them, with every other power of ten labelled up the side and minor ticks along the bottom;
/// and a range inside one decade, where the minor ticks carry the labels `2×10⁰`, `3×10⁰` and
/// `4×10⁰` beside an even y axis.
#[test]
fn log_axes_are_drawn_as_the_reference_file() {
    let program = "\
import pyplot as plt


def main() -> Raises[None, OSError | ValueError]:
    x = Array.zeros(51)
    for i in range(51):
        x[i] = 10.0 ** (-2.0 + 0.1 * f64(i))
    ax = plt.axes()
    ax.plot(x, x * x)
    ax.set_xscale(\"log\")
    ax.set_yscale(\"log\")
    ax.set_xlabel(\"x\")
    ax.savefig(\"logaxes.svg\")
    narrow = plt.axes()
    narrow.plot(Array([2.0, 3.0, 5.0]), Array([1.0, 2.0, 3.0]))
    narrow.set_xscale(\"log\")
    narrow.savefig(\"lognarrow.svg\")
";
    let (ran, directory) = run("plotting-log-axes", program).unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(ran.status.code(), Some(0));
    for name in ["logaxes.svg", "lognarrow.svg"] {
        let drawn = fs::read_to_string(directory.join(name)).unwrap();
        let reference = fs::read_to_string(fixtures().join(name)).unwrap();
        assert_eq!(drawn, reference, "{name} differs from the reference file");
    }
}

/// A scale other than `"linear"` and `"log"` panics at `set_xscale`, where matplotlib raises
/// `ValueError`.
#[test]
fn unknown_scales_panic() {
    let program = "\
import pyplot as plt


def main():
    ax = plt.axes()
    ax.set_xscale(\"symlog\")
";
    let (ran, _directory) = run("plotting-scale", program).unwrap();
    assert_eq!(ran.status.code(), Some(101));
    assert!(
        panic_report(&ran)
            .starts_with("panic: assertion failed: known, f\"{value!r} is not a scale"),
        "{}",
        panic_report(&ran)
    );
}
