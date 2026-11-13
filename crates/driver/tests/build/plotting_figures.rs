//! Figures of several axes, error bars, filled areas and text (roadmap 3.15.4, decision 0046),
//! built and run: `byplot.subplots` lays a grid out as matplotlib's default subplot parameters
//! do, and `errorbar`, `fill_between` and `text` draw as matplotlib 3.11 draws them.
//!
//! The reference file in `tests/fixtures/plotting` was written by this program and checked
//! against matplotlib's drawing of the same figure; its `README.md` says how.

use super::{Scratch, baryon_build, package, panic_report, stderr};
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Output};

/// The directory of reference plots.
fn fixtures() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/plotting"
    ))
}

/// Builds `program` as the package `name`, runs it there, and returns what it printed and the
/// package's directory, which is removed when it is dropped.
///
/// # Errors
///
/// Fails when the package cannot be written or the program cannot be run.
fn run(name: &str, program: &str) -> io::Result<(Output, Scratch)> {
    let directory = package(name, &[("main.byn", program)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&executable).current_dir(&directory).output()?;
    Ok((ran, directory))
}

/// A two by two grid drawn as the stored reference file: error bars in y with caps under round
/// markers and a legend that shows them together, error bars in x and y on a line with square
/// markers, an area filled at 30% opacity between two curves with one of them drawn over it,
/// and bars with text aligned to its point every way `text` aligns it. Each axes takes its
/// colors from its own cycle, as matplotlib's do.
#[test]
fn figures_are_drawn_as_the_reference_file() {
    let program = "\
import byplot as plt


def main() -> Raises[None, OSError | ValueError]:
    fig = plt.subplots(2, 2)
    x = Array([1.0, 2.0, 3.0, 4.0])
    y = Array([2.0, 3.5, 3.0, 5.0])
    errors = Array([0.5, 0.3, 0.8, 0.4])
    fig.axes[0].errorbar(x, y, yerr=errors, fmt=\"o\", capsize=3.0, label=\"data\")
    fig.axes[0].legend()
    across = Array([0.3, 0.1, 0.2, 0.4])
    fig.axes[1].errorbar(x, y, yerr=Array.fill(4, 0.2), xerr=across, fmt=\"s-\")
    t = Array.zeros(21)
    for i in range(21):
        t[i] = 0.1 * f64(i)
    fig.axes[2].fill_between(t, t * t, t, alpha=0.3)
    fig.axes[2].plot(t, t * t)
    fig.axes[3].bar(x, y)
    fig.axes[3].text(2.0, 4.0, \"peak\", ha=\"center\", va=\"bottom\")
    fig.axes[3].text(0.5, 1.0, \"left top\", va=\"top\")
    fig.axes[3].text(4.5, 2.0, \"right\", ha=\"right\", fontsize=14.0, va=\"center\")
    fig.savefig(\"figures.svg\")
";
    let (ran, directory) = run("plotting-figures", program).unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(ran.status.code(), Some(0));
    let drawn = fs::read_to_string(directory.join("figures.svg")).unwrap();
    let reference = fs::read_to_string(fixtures().join("figures.svg")).unwrap();
    assert_eq!(
        drawn, reference,
        "figures.svg differs from the reference file"
    );
}

/// A grid of no rows, an `errorbar` format that is not one, and a `text` alignment that is not
/// one each panic at the call, where matplotlib raises `ValueError`.
#[test]
fn wrong_grids_formats_and_alignments_panic() {
    let cases = [
        (
            "plotting-grid",
            "fig = plt.subplots(0, 2)",
            "panic: assertion failed: nrows >= 1, f\"Number of rows must be a positive integer",
        ),
        (
            "plotting-format",
            "plt.axes().errorbar(Array.zeros(2), Array.zeros(2), fmt=\"bogus\")",
            "panic: assertion failed: known, f\"{fmt!r} is not a format",
        ),
        (
            "plotting-alignment",
            "plt.axes().text(0.0, 0.0, \"x\", ha=\"middle\")",
            "panic: assertion failed: across, f\"{ha!r} is not a horizontal alignment",
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
