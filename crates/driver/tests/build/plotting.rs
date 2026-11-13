//! Plots (roadmap 3.15.1 and 3.15.2, decision 0046), built and run: `ticker` places and labels
//! ticks as matplotlib 3.11 does, to the last bit, and `byplot` draws the same SVG as the
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

/// For each of 310 ranges, `MaxNLocator.tick_values` gives the positions matplotlib gives, bit
/// for bit, and `ScalarFormatter` the same labels and offset text: printed with `f"{t}"`, which
/// writes the shortest text that reads back as the same `f64`, as Python's `repr` does.
#[test]
fn ticks_are_placed_and_labelled_as_matplotlib_does() {
    let program = "\
from ticker import MaxNLocator, ScalarFormatter


def main() -> Raises[None, OSError | ValueError]:
    locator = MaxNLocator(9, [1.0, 2.0, 2.5, 5.0, 10.0])
    with open(\"ranges.txt\") as f:
        for line in f:
            parts = line.split()
            low = float(parts[0])
            high = float(parts[1])
            ticks = locator.tick_values(low, high)
            print(\" \".join([f\"{t}\" for t in ticks]))
            formatter = ScalarFormatter(low, high)
            print(\"|\".join(formatter.format_ticks(ticks)), formatter.get_offset(ticks))
";
    let (ran, _directory) = run("plotting-ticks", program).unwrap();
    assert_eq!(stderr(&ran), "");
    let expected = fs::read_to_string(fixtures().join("matplotlib_ticks.txt")).unwrap();
    let printed = String::from_utf8_lossy(&ran.stdout);
    for (number, (got, want)) in printed.lines().zip(expected.lines()).enumerate() {
        assert_eq!(got, want, "line {} of matplotlib_ticks.txt", number + 1);
    }
    assert_eq!(printed.lines().count(), expected.lines().count());
}

/// The program that draws the reference plots, from data that is exact in binary, so every
/// machine computes the same points.
const DRAWING: &str = "\
import byplot as plt


def main() -> Raises[None, OSError | ValueError]:
    n = 41
    x = Array.zeros(n)
    cubic = Array.zeros(n)
    square = Array.zeros(n)
    for i in range(n):
        x[i] = 0.25 * f64(i) - 5.0
        cubic[i] = 0.125 * x[i] * x[i] * x[i] - 2.0 * x[i]
        square[i] = 0.5 * x[i] * x[i] - 4.0
    square[20] = 0.0 / 0.0
    ax = plt.axes()
    ax.plot(x, cubic, label=\"cubic\")
    ax.plot(x, square, linestyle=\"--\", label=\"square, with a gap\")
    ax.plot(x, 0.5 * cubic, color=\"k\", linestyle=\":\", linewidth=1.0)
    ax.set_title(\"Polynomials & <gaps>\")
    ax.set_xlabel(\"x (m)\")
    ax.set_ylabel(\"y (J)\")
    ax.savefig(\"polynomials.svg\")
    near = plt.axes()
    near.plot(x, 1.0e-7 * cubic + 1000.0, color=\"tab:green\")
    near.set_xlim(5.0, -5.0)
    near.savefig(\"offset.svg\")
";

/// The program that draws the reference plots of scatter, bars and histograms (roadmap
/// 3.15.2), from data that is exact in binary, and prints what `hist` returns.
const SHAPES: &str = "\
import byplot as plt


def main() -> Raises[None, OSError | ValueError]:
    n = 32
    x = Array.zeros(n)
    y = Array.zeros(n)
    for i in range(n):
        x[i] = 0.25 * f64(i)
        y[i] = 0.5 * x[i] + 0.125 * f64(i % 5) - 0.25
    ax = plt.axes()
    ax.scatter(x, y, label=\"measured\")
    ax.plot(x, 0.5 * x, linestyle=\"--\", marker=\"o\", markersize=3.0, label=\"fit\")
    ax.bar(Array([2.0, 5.0, 7.0]), Array([1.0, -0.5, 2.0]), width=0.6, label=\"bins\")
    ax.grid()
    ax.legend()
    ax.set_title(\"Scatter, line & bars\")
    ax.savefig(\"shapes.svg\")
    m = plt.axes()
    markers = [\"o\", \".\", \"s\", \"^\", \"v\", \"D\", \"+\", \"x\"]
    for k in range(len(markers)):
        m.scatter(Array([f64(k)]), Array([f64(k % 3)]), color=f\"C{k}\", marker=markers[k])
    m.plot(Array([0.0, 7.0]), Array([2.5, 2.5]), linestyle=\"\", marker=\"s\", label=\"markers only\")
    m.legend(loc=\"center\")
    m.savefig(\"markers.svg\")
    data = Array.zeros(200)
    for i in range(200):
        data[i] = 0.25 * f64((i * i) % 17)
    h = plt.axes()
    counts, edges = h.hist(data, bins=8, label=\"counts\")
    h.legend(loc=\"upper left\")
    h.set_xlabel(\"value\")
    h.savefig(\"histogram.svg\")
    print(counts)
    print(edges)
";

/// Two plots are drawn as the stored reference files, byte for byte: three lines in the cycle's
/// colors, a fixed color, dashes and dots, a NaN that breaks a line, labels and a title with
/// characters XML escapes; and a reversed x axis, with y ticks that need an offset and a power
/// of ten, `1e−7+1e3`, as matplotlib writes it.
#[test]
fn plots_are_drawn_as_the_reference_files() {
    let (ran, directory) = run("plotting-drawing", DRAWING).unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(ran.status.code(), Some(0));
    compare(&directory, &["polynomials.svg", "offset.svg"]).unwrap();
}

/// Scatter, bars and a histogram are drawn as the stored reference files: markers of every
/// shape from the patch cycle, a line with markers and one with markers only, bars above and
/// below 0, grid lines, and legends placed by `"best"`, by name and in the centre. `hist`
/// returns the counts and edges `NumPy` gives for the same data, and its bars start at 0 as
/// matplotlib's do.
#[test]
fn shapes_are_drawn_as_the_reference_files() {
    let (ran, directory) = run("plotting-shapes", SHAPES).unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "[35. 24. 23.  0. 47.  0. 24. 47.]\n[0.  0.5 1.  1.5 2.  2.5 3.  3.5 4. ]\n"
    );
    compare(&directory, &["shapes.svg", "markers.svg", "histogram.svg"]).unwrap();
}

/// Asserts that each file of `names` in `directory` is the reference file of that name.
///
/// # Errors
///
/// Fails when a drawn file or a reference file cannot be read.
fn compare(directory: &Path, names: &[&str]) -> io::Result<()> {
    for name in names {
        let drawn = fs::read_to_string(directory.join(name))?;
        let reference = fs::read_to_string(fixtures().join(name))?;
        assert_eq!(drawn, reference, "{name} differs from the reference file");
    }
    Ok(())
}

/// `savefig` raises `ValueError` with matplotlib's message for a format it does not write, and
/// writes nothing; a dot in a directory's name is not an extension.
#[test]
fn other_formats_raise_value_error() {
    let program = "\
from pathlib import Path
import byplot as plt


def main() -> Raises[None, OSError]:
    ax = plt.axes()
    for name in [\"plot.jpg\", \"plot\", \"runs.d/plot\", \"plot.SVG.pdf\"]:
        try:
            ax.savefig(name)
        except ValueError as error:
            print(error.message)
    print(Path(\"plot.jpg\").exists())
";
    let (ran, _directory) = run("plotting-formats", program).unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "Format 'jpg' is not supported (supported formats: png, svg)\n\
         Format '' is not supported (supported formats: png, svg)\n\
         Format '' is not supported (supported formats: png, svg)\n\
         Format 'pdf' is not supported (supported formats: png, svg)\n\
         False\n"
    );
}

/// `plot` of arrays of different lengths panics, where matplotlib raises `ValueError`: the
/// program is wrong, not its data (decision 0046).
#[test]
fn plotting_arrays_of_different_lengths_panics() {
    let program = "\
import byplot as plt


def main():
    ax = plt.axes()
    ax.plot(Array.zeros(3), Array.zeros(4))
";
    let (ran, _directory) = run("plotting-lengths", program).unwrap();
    assert_eq!(ran.status.code(), Some(101));
    assert!(
        panic_report(&ran).starts_with(
            "panic: assertion failed: len(x) == len(y), \"plot needs x and y of the same length\""
        ),
        "{}",
        panic_report(&ran)
    );
}

/// A color name `colors.to_hex` does not know panics when it is given, not when the plot is
/// saved, so the backtrace points at the call that is wrong.
#[test]
fn unknown_colors_panic() {
    let program = "\
import byplot as plt


def main():
    ax = plt.axes()
    ax.bar(Array.zeros(2), Array.zeros(2), color=\"navy\")
";
    let (ran, _directory) = run("plotting-colors", program).unwrap();
    assert_eq!(ran.status.code(), Some(101));
    assert!(
        panic_report(&ran).starts_with("panic: assertion failed: False, f\"{c!r} is not a color"),
        "{}",
        panic_report(&ran)
    );
    assert!(
        stderr(&ran).contains("byplot.Axes.bar at"),
        "{}",
        stderr(&ran)
    );
}

/// Equal limits are widened by 5% each way, as matplotlib's `set_xlim` widens them, so the
/// file holds only finite coordinates and the ticks from 0.96 to 1.04 that matplotlib draws.
#[test]
fn equal_limits_are_widened() {
    let program = "\
import byplot as plt


def main() -> Raises[None, OSError | ValueError]:
    ax = plt.axes()
    ax.plot(Array([0.0, 1.0]), Array([0.0, 1.0]))
    ax.set_xlim(1.0, 1.0)
    ax.set_ylim(0.0, 0.0)
    ax.savefig(\"equal.svg\")
    with open(\"equal.svg\") as f:
        text = f.read()
    print(\"nan\" in text, \"inf\" in text, \">0.96<\" in text, \">1.04<\" in text)
    print(\">−0.04<\" in text, \">0.04<\" in text)
";
    let (ran, _directory) = run("plotting-equal-limits", program).unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "False False True True\nTrue True\n"
    );
}

/// A NaN or infinite limit panics at `set_xlim` or `set_ylim`, with matplotlib's message,
/// rather than deep inside `savefig`.
#[test]
fn limits_that_are_not_finite_panic() {
    let program = "\
import byplot as plt


def main():
    ax = plt.axes()
    ax.set_ylim(0.0 / 0.0, 2.0)
";
    let (ran, _directory) = run("plotting-nan-limits", program).unwrap();
    assert_eq!(ran.status.code(), Some(101));
    assert!(
        panic_report(&ran).starts_with(
            "panic: assertion failed: isfinite(bottom) and isfinite(top), \
             \"Axis limits cannot be NaN or Inf\""
        ),
        "{}",
        panic_report(&ran)
    );
}
