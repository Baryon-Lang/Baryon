//! Plots (roadmap 3.15.1, decision 0046), built and run: `ticker` places and labels ticks as
//! matplotlib 3.11 does, to the last bit, and `pyplot` draws the same SVG as the reference files.
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
import pyplot as plt


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

/// Two plots are drawn as the stored reference files, byte for byte: three lines in the cycle's
/// colors, a fixed color, dashes and dots, a NaN that breaks a line, labels and a title with
/// characters XML escapes; and a reversed x axis, with y ticks that need an offset and a power
/// of ten, `1e−7+1e3`, as matplotlib writes it.
#[test]
fn plots_are_drawn_as_the_reference_files() {
    let (ran, directory) = run("plotting-drawing", DRAWING).unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(ran.status.code(), Some(0));
    for name in ["polynomials.svg", "offset.svg"] {
        let drawn = fs::read_to_string(directory.join(name)).unwrap();
        let reference = fs::read_to_string(fixtures().join(name)).unwrap();
        assert_eq!(drawn, reference, "{name} differs from the reference file");
    }
}

/// `savefig` raises `ValueError` with matplotlib's message for a format it does not write, and
/// writes nothing; a dot in a directory's name is not an extension.
#[test]
fn other_formats_raise_value_error() {
    let program = "\
from pathlib import Path
import pyplot as plt


def main() -> Raises[None, OSError]:
    ax = plt.axes()
    for name in [\"plot.png\", \"plot\", \"runs.d/plot\", \"plot.SVG.pdf\"]:
        try:
            ax.savefig(name)
        except ValueError as error:
            print(error.message)
    print(Path(\"plot.png\").exists())
";
    let (ran, _directory) = run("plotting-formats", program).unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "Format 'png' is not supported (supported formats: svg)\n\
         Format '' is not supported (supported formats: svg)\n\
         Format '' is not supported (supported formats: svg)\n\
         Format 'pdf' is not supported (supported formats: svg)\n\
         False\n"
    );
}

/// `plot` of arrays of different lengths panics, where matplotlib raises `ValueError`: the
/// program is wrong, not its data (decision 0046).
#[test]
fn plotting_arrays_of_different_lengths_panics() {
    let program = "\
import pyplot as plt


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
