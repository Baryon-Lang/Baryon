//! Images on plots (roadmap 3.15.3, decision 0046), built and run: `imshow` and `colorbar`
//! draw the reference files, every colormap colors values as matplotlib 3.11 does, byte for
//! byte, and each image is embedded as a PNG a reader can decode.
//!
//! The colormaps and the PNG encoder are internal to the standard library, so these tests
//! reach them as a program does, through `byplot`, and read the PNG back out of the SVG.

use super::{Scratch, baryon_build, package, panic_report, stderr};
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Output};

/// The directory of recorded colors and reference plots.
fn fixtures() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/plotting"
    ))
}

/// Builds `program` as the package `name`, runs it in the package's directory, and returns
/// what it printed and the directory, which is removed when it is dropped.
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

/// The program that draws the reference images (roadmap 3.15.3), from data that is exact in
/// binary.
const IMAGES: &str = "\
import byplot as plt


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
/// bottom, a NaN left transparent, a title and a labelled colorbar; and `gray_r` stretched over
/// `extent=(0, 8, 0, 3)` and filling the axes with `aspect="auto"`, under a line. Each image is
/// a PNG inside the SVG, so the files also fix the PNG encoder's output.
#[test]
fn images_are_drawn_as_the_reference_files() {
    let (ran, directory) = run("plotting-images", IMAGES).unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(ran.status.code(), Some(0));
    compare(&directory, &["grid.svg", "saddle.svg", "auto.svg"]).unwrap();
}

/// Every colormap gives the bytes matplotlib 3.11 gives, recorded in
/// `matplotlib_colormaps.txt`, at 0 and 1, between them, outside them and at NaN, and so does
/// each one reversed: each is drawn as an image of one row holding those values, from `vmin=0`
/// to `vmax=1`, and the pixels are read back from the PNG inside the SVG.
#[test]
fn colormaps_match_matplotlib() {
    let expected = fs::read_to_string(fixtures().join("matplotlib_colormaps.txt")).unwrap();
    let names: Vec<&str> = expected
        .lines()
        .map(|line| line.split(' ').next().unwrap_or_default())
        .collect();
    let list = names
        .iter()
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let program = format!(
        "\
import byplot as plt


def main() -> Raises[None, OSError | ValueError]:
    values = [-0.5, 0.0, 0.001, 0.1, 0.25, 0.5, 0.7, 0.999, 1.0, 1.5, 0.0 / 0.0]
    strip = Array.zeros((1, 11))
    for k in range(11):
        strip[0, k] = values[k]
    for name in [{list}]:
        ax = plt.axes()
        ax.imshow(strip, cmap=name.copy(), vmin=0.0, vmax=1.0)
        ax.savefig(f\"{{name}}.svg\")
"
    );
    let (ran, directory) = run("plotting-colormaps", &program).unwrap();
    assert_eq!(stderr(&ran), "");
    for line in expected.lines() {
        let name = line.split(' ').next().unwrap_or_default();
        let svg = fs::read_to_string(directory.join(format!("{name}.svg"))).unwrap();
        let (width, height, pixels) = embedded_png(&svg).unwrap();
        assert_eq!((width, height), (11, 1), "{name}");
        let colors: Vec<String> = pixels
            .chunks(4)
            .map(|p| format!("{},{},{},{}", p[0], p[1], p[2], p[3]))
            .collect();
        assert_eq!(format!("{name} {}", colors.join(" ")), line);
    }
}

/// A two-cell image is embedded as a valid PNG: the signature, the header of an 8-bit RGBA
/// image, stored deflate blocks with a correct Adler-32, a correct CRC-32 on every chunk, and
/// the end; `gray` from `vmin` to `vmax` makes its cells black and white.
#[test]
fn images_are_embedded_as_png() {
    let program = "\
import byplot as plt


def main() -> Raises[None, OSError | ValueError]:
    cells = Array.zeros((1, 2))
    cells[0, 1] = 1.0
    ax = plt.axes()
    ax.imshow(cells, cmap=\"gray\")
    ax.savefig(\"cells.svg\")
";
    let (ran, directory) = run("plotting-png", program).unwrap();
    assert_eq!(stderr(&ran), "");
    let svg = fs::read_to_string(directory.join("cells.svg")).unwrap();
    assert_eq!(
        embedded_png(&svg),
        Some((2, 1, vec![0, 0, 0, 255, 255, 255, 255, 255]))
    );
}

/// The width, height and RGBA pixels of the first PNG embedded in `svg` as a `data:` URL,
/// checking every CRC-32 and the Adler-32 on the way, or `None` when `svg` holds no PNG. The
/// encoder writes one row filter of 0 a row and stored deflate blocks only, which is all this
/// reads.
fn embedded_png(svg: &str) -> Option<(u32, u32, Vec<u8>)> {
    let start = svg.find("base64,")? + 7;
    let end = start + svg[start..].find('"')?;
    let bytes = base64(&svg[start..end])?;
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    let mut at = 8;
    let (mut width, mut height) = (0, 0);
    let mut stream = Vec::new();
    while at < bytes.len() {
        let length = usize::try_from(be32(&bytes, at)).ok()?;
        let kind = &bytes[at + 4..at + 8];
        let data = &bytes[at + 8..at + 8 + length];
        let crc = be32(&bytes, at + 8 + length);
        assert_eq!(
            crc32(&bytes[at + 4..at + 8 + length]),
            crc,
            "CRC-32 of a chunk"
        );
        match kind {
            b"IHDR" => {
                width = be32(data, 0);
                height = be32(data, 4);
                assert_eq!(&data[8..], &[8, 6, 0, 0, 0], "8-bit RGBA, not interlaced");
            }
            b"IDAT" => stream.extend_from_slice(data),
            _ => {}
        }
        at += 12 + length;
    }
    assert_eq!(stream[0] & 0x0f, 8, "deflate");
    let mut raw = Vec::new();
    let mut block = 2;
    loop {
        let last = stream[block] & 1;
        assert_eq!(stream[block] >> 1, 0, "a stored block");
        let length = usize::from(u16::from_le_bytes([stream[block + 1], stream[block + 2]]));
        raw.extend_from_slice(&stream[block + 5..block + 5 + length]);
        block += 5 + length;
        if last == 1 {
            break;
        }
    }
    assert_eq!(
        adler32(&raw),
        be32(&stream, block),
        "Adler-32 of the image data"
    );
    let row = 1 + 4 * usize::try_from(width).ok()?;
    let mut pixels = Vec::new();
    for line in raw.chunks(row) {
        assert_eq!(line[0], 0, "no row filter");
        pixels.extend_from_slice(&line[1..]);
    }
    assert_eq!(pixels.len(), 4 * usize::try_from(width * height).ok()?);
    Some((width, height, pixels))
}

/// The big-endian `u32` at `at` in `bytes`, as PNG writes its lengths and checksums.
fn be32(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// `text` decoded from base64, or `None` when it holds a character that is not a base64 digit.
fn base64(text: &str) -> Option<Vec<u8>> {
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bits = 0u32;
    let mut count = 0;
    let mut bytes = Vec::new();
    for character in text.bytes().filter(|&c| c != b'=') {
        let value = alphabet.iter().position(|&a| a == character)?;
        bits = (bits << 6) | u32::try_from(value).ok()?;
        count += 6;
        if count >= 8 {
            count -= 8;
            bytes.push(u8::try_from((bits >> count) & 0xff).ok()?);
        }
    }
    Some(bytes)
}

/// The CRC-32 of `data`, as PNG and zlib compute it, a bit at a time.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// The Adler-32 of `data`, as zlib computes it.
fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

/// An unknown colormap panics at `imshow`, and a colorbar with no image panics at
/// `colorbar`, where matplotlib raises `ValueError` and `RuntimeError`.
#[test]
fn unknown_colormaps_and_lonely_colorbars_panic() {
    let program = "\
import byplot as plt


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
import byplot as plt


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
