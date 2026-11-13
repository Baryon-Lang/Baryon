//! PNG output (roadmap 3.15.6, decision 0046), built and run: `savefig` with a `.png` name
//! draws the figure into pixels, anti-aliased, with text from the outlines of `DejaVu Sans`, and
//! writes a PNG file 640 by 480 pixels, as matplotlib's Agg backend does at 100 dpi.
//!
//! The reference images in `tests/fixtures/plotting` were written by these programs and checked
//! by eye against matplotlib's; its `README.md` says how. A program reads both PNG files back
//! with `zlib` and counts the colour values more than one level apart, so the test allows the
//! last bit of `sin` and `cos` to differ between platforms but nothing a reader could see.

use super::{Scratch, baryon_build, package, stderr};
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

/// Builds `program` as the package `name`, copies each reference image of `names` beside it as
/// `reference_<name>`, runs it there, and returns what it printed and the package's directory,
/// which is removed when it is dropped.
///
/// # Errors
///
/// Fails when the package cannot be written or the program cannot be run.
fn run(name: &str, program: &str, names: &[&str]) -> io::Result<(Output, Scratch)> {
    let directory = package(name, &[("main.byn", program)])?;
    for image in names {
        fs::copy(
            fixtures().join(image),
            directory.join(format!("reference_{image}")),
        )?;
    }
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&executable).current_dir(&directory).output()?;
    Ok((ran, directory))
}

/// The Baryon functions every program here ends with: `rows` reads a PNG file's rows back,
/// and `compare` prints its size and how many colour values are more than one level from the
/// reference image's.
const COMPARE: &str = "
def rows(path: str) -> Raises[tuple[int, int, Array[u8, 1]], OSError]:
    data = Array.fromfile(path, u8)
    stream: list[u8] = []
    at = 8
    while at < len(data):
        length = 0
        for k in range(4):
            length = 256 * length + int(data[at + k])
        if data[at + 4] == u8(73) and data[at + 5] == u8(68):
            for k in range(at + 8, at + 8 + length):
                stream.append(data[k])
        at += 12 + length
    width = 0
    height = 0
    for k in range(4):
        width = 256 * width + int(data[16 + k])
        height = 256 * height + int(data[20 + k])
    return (width, height, decompress(Array(stream)))


def compare(name: str) -> Raises[None, OSError]:
    width, height, drawn = rows(name)
    unused_width, unused_height, reference = rows(\"reference_\" + name)
    apart = 0
    for k in range(len(drawn)):
        if abs(int(drawn[k]) - int(reference[k])) > 1:
            apart += 1
    print(name, width, height, len(drawn) == len(reference), apart)
";

/// A line plot drawn to PNG is the reference image: solid and dashed lines, markers, a legend,
/// grid lines, and a title with Greek letters, `±` and `°`.
#[test]
fn line_plots_are_drawn_as_the_reference_image() {
    let program = format!(
        "\
import byplot as plt
from math import sin, cos
from zlib import decompress


def main() -> Raises[None, OSError | ValueError]:
    n = 200
    t = Array.zeros(n)
    s = Array.zeros(n)
    c = Array.zeros(n)
    for k in range(n):
        t[k] = 0.05 * float(k)
        s[k] = sin(t[k])
        c[k] = cos(t[k])
    ax = plt.axes()
    ax.plot(t, s, label=\"sin\")
    ax.plot(t, c, linestyle=\"--\", label=\"cos\")
    ax.scatter(t[0:200:20].copy(), s[0:200:20].copy(), label=\"samples\")
    ax.set_title(\"Waves: αβγ ±1 °\")
    ax.set_xlabel(\"time (s)\")
    ax.set_ylabel(\"amplitude\")
    ax.legend()
    ax.grid()
    ax.savefig(\"waves.png\")
    compare(\"waves.png\")

{COMPARE}"
    );
    let (ran, _directory) = run("plotting-png-lines", &program, &["waves.png"]).unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "waves.png 640 480 True 0\n"
    );
}

/// A grid of plots drawn to PNG is the reference image: filled contours under dashed and solid
/// contour lines, an image with a colorbar, error bars on logarithmic axes with mathtext tick
/// labels, and bars under a half-transparent filled area, with text and an axis label in
/// Greek.
#[test]
fn grids_of_plots_are_drawn_as_the_reference_image() {
    let program = format!(
        "\
import byplot as plt
from math import exp
from zlib import decompress


def main() -> Raises[None, OSError | ValueError]:
    n = 60
    x = Array.zeros(n)
    for k in range(n):
        x[k] = -3.0 + 6.0 * float(k) / float(n - 1)
    z = Array.zeros((n, n))
    for i in range(n):
        for j in range(n):
            z[i, j] = exp(-(x[j] - 1.0) ** 2 - x[i] ** 2) - exp(-(x[j] + 1.0) ** 2 - x[i] ** 2)
    fig = plt.subplots(2, 2)
    fig.axes[0].contourf(x, x, z)
    fig.axes[0].contour(x, x, z, colors=\"k\")
    fig.axes[0].set_title(\"dipole\")
    fig.axes[1].imshow(z)
    fig.axes[1].colorbar()
    m = 30
    t = Array.zeros(m)
    v = Array.zeros(m)
    for k in range(m):
        t[k] = 1.0 + float(k)
        v[k] = t[k] ** 2.5
    fig.axes[2].errorbar(t, v, yerr=0.2 * v, fmt=\"o\", capsize=3.0)
    fig.axes[2].set_xscale(\"log\")
    fig.axes[2].set_yscale(\"log\")
    fig.axes[3].bar(Array([1.0, 2.0, 3.0]), Array([3.0, -1.0, 2.0]))
    fig.axes[3].fill_between(t / 10.0, v / 3000.0, alpha=0.3)
    fig.axes[3].text(1.0, 2.5, \"µ = 2.0\", ha=\"center\")
    fig.axes[3].set_xlabel(\"ΔT (K)\")
    fig.savefig(\"grid.png\")
    compare(\"grid.png\")

{COMPARE}"
    );
    let (ran, _directory) = run("plotting-png-grid", &program, &["grid.png"]).unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "grid.png 640 480 True 0\n"
    );
}
