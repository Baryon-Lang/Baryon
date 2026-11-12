//! Plots (roadmap 3.15.1 and 3.15.2, decision 0046), built and run: `ticker` places and labels ticks as
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

/// The program that draws the reference plots of scatter, bars and histograms (roadmap
/// 3.15.2), from data that is exact in binary, and prints what `hist` returns.
const SHAPES: &str = "\
import pyplot as plt


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

/// A color name `colors.to_hex` does not know panics when it is given, not when the plot is
/// saved, so the backtrace points at the call that is wrong.
#[test]
fn unknown_colors_panic() {
    let program = "\
import pyplot as plt


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
        stderr(&ran).contains("pyplot.Axes.bar at"),
        "{}",
        stderr(&ran)
    );
}

/// Equal limits are widened by 5% each way, as matplotlib's `set_xlim` widens them, so the
/// file holds only finite coordinates and the ticks from 0.96 to 1.04 that matplotlib draws.
#[test]
fn equal_limits_are_widened() {
    let program = "\
import pyplot as plt


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
import pyplot as plt


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

/// The program that draws the reference images (roadmap 3.15.3), from data that is exact in
/// binary.
const IMAGES: &str = "\
import pyplot as plt


def main() -> Raises[None, OSError | ValueError]:
    grid = Array.zeros((3, 4))
    for i in range(3):
        for j in range(4):
            grid[i, j] = f64(4 * i + j)
    ax = plt.axes()
    ax.imshow(grid)
    ax.colorbar()
    ax.savefig(\"grid.svg\")
    rows = 24
    columns = 40
    field = Array.zeros((rows, columns))
    for i in range(rows):
        for j in range(columns):
            x = 0.125 * f64(j - 20)
            y = 0.125 * f64(i - 12)
            field[i, j] = x * x - y * y
    field[3, 5] = 0.0 / 0.0
    saddle = plt.axes()
    saddle.imshow(field, cmap=\"RdBu_r\", vmin=-4.0, vmax=4.0, origin=\"lower\")
    saddle.colorbar(label=\"height (m)\")
    saddle.set_title(\"A saddle\")
    saddle.set_xlabel(\"column\")
    saddle.savefig(\"saddle.svg\")
    wide = plt.axes()
    wide.imshow(grid, cmap=\"gray_r\", aspect=\"auto\", extent=(0.0, 8.0, 0.0, 3.0))
    wide.plot(Array([0.0, 8.0]), Array([0.0, 3.0]), color=\"tab:red\")
    wide.savefig(\"auto.svg\")
";

/// Three images are drawn as the stored reference files: `viridis` with a colorbar and square
/// cells, row 0 at the top; a saddle in `RdBu_r` with fixed `vmin` and `vmax`, row 0 at the
/// bottom, a NaN left transparent, a title and a labelled colorbar; and `gray_r` stretched
/// over `extent=(0, 8, 0, 3)` and filling the axes with `aspect="auto"`, under a line. Each image is a PNG inside the SVG, so the files
/// also fix the PNG encoder's output.
#[test]
fn images_are_drawn_as_the_reference_files() {
    let (ran, directory) = run("plotting-images", IMAGES).unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(ran.status.code(), Some(0));
    compare(&directory, &["grid.svg", "saddle.svg", "auto.svg"]).unwrap();
}

/// Every colormap gives the bytes matplotlib 3.11 gives, recorded in
/// `matplotlib_colormaps.txt`, at 0 and 1, between them, outside them and at NaN, and so does
/// each one reversed.
#[test]
fn colormaps_match_matplotlib() {
    let program = "\
from cm import get_cmap


def main() -> Raises[None, OSError]:
    values = [-0.5, 0.0, 0.001, 0.1, 0.25, 0.5, 0.7, 0.999, 1.0, 1.5, 0.0 / 0.0]
    with open(\"matplotlib_colormaps.txt\") as f:
        for line in f:
            name = line.split()[0].copy()
            cmap = get_cmap(name)
            colors: list[str] = []
            for x in values:
                red, green, blue, alpha = cmap.to_rgba_bytes(x)
                colors.append(f\"{red},{green},{blue},{alpha}\")
            print(name, \" \".join(colors))
";
    let directory = package("plotting-colormaps", &[("main.byn", program)]).unwrap();
    fs::copy(
        fixtures().join("matplotlib_colormaps.txt"),
        directory.join("matplotlib_colormaps.txt"),
    )
    .unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&executable)
        .current_dir(&directory)
        .output()
        .unwrap();
    assert_eq!(stderr(&ran), "");
    let expected = fs::read_to_string(fixtures().join("matplotlib_colormaps.txt")).unwrap();
    assert_eq!(String::from_utf8_lossy(&ran.stdout), expected);
}

/// A two-pixel image is embedded as the PNG a PNG reader decodes to black then red: the
/// signature, the header, one stored deflate block with its Adler-32, each chunk's CRC-32, and
/// the end, checked once with Pillow and kept here byte for byte in base64.
#[test]
fn images_are_embedded_as_png() {
    let program = "\
from backend_svg import RendererSVG


def main():
    pixels: Array[u8, 3] = Array.zeros((1, 2, 4))
    pixels[0, 0, 3] = u8(255)
    pixels[0, 1, 0] = u8(255)
    pixels[0, 1, 3] = u8(255)
    renderer = RendererSVG(2.0, 1.0, [])
    renderer.draw_image(0.0, 0.0, 2.0, 1.0, pixels, False, False)
    print(renderer.elements[0])
";
    let (ran, _directory) = run("plotting-png", program).unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "<image x=\"0.000\" y=\"0.000\" width=\"2.000\" height=\"1.000\" \
         preserveAspectRatio=\"none\" style=\"image-rendering:pixelated\" \
         href=\"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAAFElEQVR4AQEJAPb/\
         AAAAAP//AAD/Cf8C/vNM74sAAAAASUVORK5CYII=\"/>\n"
    );
}

/// An unknown colormap panics at `imshow`, and a colorbar with no image panics at
/// `colorbar`, where matplotlib raises `ValueError` and `RuntimeError`.
#[test]
fn unknown_colormaps_and_lonely_colorbars_panic() {
    let program = "\
import pyplot as plt


def main():
    ax = plt.axes()
    ax.imshow(Array.zeros((2, 2)), cmap=\"jet\")
";
    let (ran, _directory) = run("plotting-cmap", program).unwrap();
    assert_eq!(ran.status.code(), Some(101));
    assert!(
        panic_report(&ran).starts_with("panic: assertion failed: known, ("),
        "{}",
        panic_report(&ran)
    );
    let program = "\
import pyplot as plt


def main():
    ax = plt.axes()
    ax.colorbar()
";
    let (ran, _directory) = run("plotting-colorbar", program).unwrap();
    assert_eq!(ran.status.code(), Some(101));
    assert!(
        panic_report(&ran).starts_with(
            "panic: assertion failed: len(self.images) > 0, \
             \"colorbar needs an image: call imshow first\""
        ),
        "{}",
        panic_report(&ran)
    );
}
