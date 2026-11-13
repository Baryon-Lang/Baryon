//! PDF output (roadmap 3.15.7, decision 0046), built and run: `savefig` with a `.pdf` name
//! writes the figure as one vector page with its text in an embedded Type 3 font, as
//! matplotlib's PDF backend does.
//!
//! The reference page in `tests/fixtures/plotting` was written by this program and checked by
//! eye, drawn by poppler, against the PNG of the same figure; its `README.md` says how. The
//! program reads its own file back with `zlib`, so the page's drawing operators can be
//! compared as text, and the test checks the file's table of objects byte by byte.

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

/// Builds `program` as the package `name`, runs it in its directory, and returns what it
/// printed and the directory, which is removed when it is dropped.
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

/// A grid of plots: filled contours under dashed and solid contour lines, an image with a
/// colorbar, error bars on logarithmic axes, and bars under a half-transparent filled area,
/// with text in Greek. It writes `grid.pdf`, then the page's operators, the first stream of
/// the file, as `page.txt`.
const GRID: &str = "\
import byplot as plt
from math import exp
from zlib import decompress


def main() -> Raises[None, OSError | ValueError]:
    n = 30
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
    fig.axes[0].set_title(\"AVA dipole\")
    fig.axes[1].imshow(z)
    fig.axes[1].colorbar()
    t = Array([1.0, 2.0, 4.0, 8.0, 16.0, 32.0])
    fig.axes[2].errorbar(t, t * t, yerr=0.2 * t * t, fmt=\"o\", capsize=3.0)
    fig.axes[2].set_xscale(\"log\")
    fig.axes[2].set_yscale(\"log\")
    fig.axes[3].bar(Array([1.0, 2.0, 3.0]), Array([3.0, -1.0, 2.0]))
    fig.axes[3].fill_between(t / 10.0, t / 16.0, alpha=0.3)
    fig.axes[3].text(1.0, 2.5, \"µ = 2.0 (Ω)\", ha=\"center\")
    fig.axes[3].set_ylabel(\"ΔT (K)\")
    fig.savefig(\"grid.pdf\")
    data = Array.fromfile(\"grid.pdf\", u8)
    at = 0
    while not (data[at] == u8(115) and data[at + 1] == u8(116) and data[at + 5] == u8(109)):
        at += 1
    length = 0
    k = 0
    while k < at:
        if data[k] == u8(47) and data[k + 1] == u8(76) and data[k + 2] == u8(101):
            length = 0
            k += 8
            while data[k] != u8(32):
                length = 10 * length + int(data[k]) - 48
                k += 1
        k += 1
    stream: list[u8] = []
    for k in range(at + 7, at + 7 + length):
        stream.append(data[k])
    decompress(Array(stream)).tofile(\"page.txt\")
";

/// The figure's page draws as the reference page, its text kerned (`AVA`) and in Greek, and
/// the file is a well-formed PDF 1.4 file: every entry of its table of objects gives where
/// that object starts, and its trailer counts them and names where the table is.
#[test]
fn figures_are_written_as_pdf_pages() {
    let (ran, directory) = run("plotting-pdf", GRID).unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(ran.status.code(), Some(0));
    let page = fs::read_to_string(directory.join("page.txt")).unwrap();
    let reference = fs::read_to_string(fixtures().join("grid_page.txt")).unwrap();
    assert_eq!(page, reference, "the page differs from grid_page.txt");
    let file = fs::read(directory.join("grid.pdf")).unwrap();
    assert!(file.starts_with(b"%PDF-1.4\n"));
    let tail = String::from_utf8_lossy(&file[file.len() - 32..]);
    let start: usize = tail
        .rsplit("startxref\n")
        .next()
        .and_then(|tail| tail.lines().next())
        .and_then(|line| line.parse().ok())
        .unwrap();
    let table = std::str::from_utf8(&file[start..]).unwrap();
    let mut lines = table.lines();
    assert_eq!(lines.next(), Some("xref"));
    let count: usize = lines
        .next()
        .and_then(|line| line.strip_prefix("0 "))
        .and_then(|line| line.parse().ok())
        .unwrap();
    assert_eq!(lines.next(), Some("0000000000 65535 f "));
    for object in 1..count {
        let entry = lines.next().unwrap();
        let offset: usize = entry[..10].parse().unwrap();
        assert!(
            file[offset..].starts_with(format!("{object} 0 obj\n").as_bytes()),
            "object {object} is not where the table says"
        );
    }
    assert!(table.contains(&format!("trailer\n<< /Size {count} /Root 1 0 R >>")));
}

/// `savefig` writes no PDF for a file it cannot create, and raises `OSError`, as it does for
/// SVG and PNG.
#[test]
fn unwritable_pdf_files_raise_os_error() {
    let program = "\
import byplot as plt


def main() -> None:
    ax = plt.axes()
    for name in [\"missing/plot.pdf\", \"missing/plot.png\"]:
        try:
            ax.savefig(name)
        except OSError:
            print(\"OSError\", name)
        except ValueError:
            print(\"ValueError\")
";
    let (ran, _directory) = run("plotting-pdf-missing", program).unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "OSError missing/plot.pdf\nOSError missing/plot.png\n"
    );
}
