//! The private LZ4 codec (roadmap 3.27.6, decision 0049 point 6), built and run:
//! `lz4_decompress` reads the raw blocks the LZ4 library wrote, a block that is not valid or
//! does not hold the bytes asked for raises `OSError` with the kind `INVALID_DATA` and the
//! message the `lz4` package for Python gives, and `lz4_compress` writes blocks
//! `lz4_decompress` reads back.
//!
//! The codec is `<std>/parquet_lz4.byn`, which no program can import, and nothing in the
//! library calls it until the Parquet reader does (roadmap 3.14.6.3). So each test puts that
//! same file into its package, where the program imports it as its own module.
//!
//! The blocks in `tests/fixtures/lz4` were written by the LZ4 library; its `README.md` says
//! how. The table they hold is `tests/fixtures/zlib/text.txt`.

use super::{Scratch, baryon_build, package};
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Output};

/// The codec's source, the file the standard library holds.
const CODEC: &str = include_str!("../../library/parquet_lz4.byn");

/// Builds `program` as the package `name` with the codec as a module of the package and the
/// recorded blocks and the table beside it, runs it there, and returns what it printed and
/// the package's directory, which is removed when it is dropped.
///
/// # Errors
///
/// Fails when the package cannot be written or the program cannot be run.
fn run(name: &str, program: &str) -> io::Result<(Output, Scratch)> {
    let files = [("main.byn", program), ("parquet_lz4.byn", CODEC)];
    let directory = package(name, &files)?;
    let fixtures = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"));
    for entry in fs::read_dir(fixtures.join("lz4"))? {
        let path = entry?.path();
        if let Some(file) = path.file_name() {
            fs::copy(&path, directory.join(file))?;
        }
    }
    fs::copy(fixtures.join("zlib/text.txt"), directory.join("text.txt"))?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable).current_dir(&directory).output()?;
    Ok((run, directory))
}

/// The library's blocks decompress to what they were made from, with the CRC-32 Python's
/// `zlib.crc32` gives: the table, forty copies of it, literals and a match whose counts go on
/// past the token, a match that reaches into the bytes it writes, and an empty block.
#[test]
fn the_library_s_blocks_are_decompressed() {
    let program = "\
from parquet_lz4 import lz4_decompress
from zlib import crc32


def show(name: str, size: int) -> Raises[None, OSError]:
    data = lz4_decompress(Array.fromfile(f\"{name}.lz4\", u8), size)
    print(name, len(data), crc32(data))


def main() -> Raises[None, OSError]:
    show(\"text\", 11600)
    show(\"long\", 464000)
    show(\"noise\", 300)
    show(\"runs\", 100000)
    show(\"empty\", 0)
    table = Array.fromfile(\"text.txt\", u8)
    print((lz4_decompress(Array.fromfile(\"text.lz4\", u8), 11600) == table).all())
";
    let (output, _) = run("lz4_blocks", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "text 11600 1019181682\n\
         long 464000 787520434\n\
         noise 300 1459503370\n\
         runs 100000 4261876081\n\
         empty 0 0\n\
         True\n"
    );
}

/// A block that is not valid raises `OSError` with the kind `INVALID_DATA` and the message
/// of the `lz4` package, without its error code: a block cut short, a match from 0 back and
/// from before the first byte, and no bytes at all. So does a good block asked for one byte
/// more or fewer than it holds, a negative size, and a size no block of its length could
/// reach, which is refused before anything is allocated.
#[test]
fn refused_blocks_raise_the_library_s_message() {
    let program = "\
from os import ErrorKind
from parquet_lz4 import lz4_decompress


def attempt(name: str, size: int):
    try:
        lz4_decompress(Array.fromfile(f\"{name}.lz4\", u8), size)
        print(name, size, \"no error\")
    except OSError as error:
        print(name, size, error.kind == ErrorKind.INVALID_DATA, error.strerror)


def main():
    attempt(\"truncated\", 11600)
    attempt(\"zero_distance\", 13)
    attempt(\"before_start\", 13)
    attempt(\"none\", 0)
    attempt(\"text\", 11599)
    attempt(\"text\", 11601)
    attempt(\"text\", -1)
    attempt(\"empty\", 1000000000000)
";
    let (output, _) = run("lz4_errors", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    let message = "Decompression failed: corrupt input or insufficient space in destination buffer";
    let names = [
        "truncated 11600",
        "zero_distance 13",
        "before_start 13",
        "none 0",
        "text 11599",
        "text 11601",
        "text -1",
        "empty 1000000000000",
    ];
    let expected = names.map(|name| [name, " True ", message, "\n"].concat());
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected.concat());
}

/// `lz4_compress` writes blocks `lz4_decompress` reads back: the table, forty copies of it,
/// bytes that do not compress, one byte many times, inputs around the 13 bytes under which
/// nothing is searched, and nothing. The sizes are Baryon's own (decision 0049), within a
/// tenth of the LZ4 library's on these inputs, and its decoder read every one of these
/// blocks back (checked when the test was written; the tests do not run it).
#[test]
fn compressed_blocks_are_read_back() {
    let program = "\
from parquet_lz4 import lz4_compress, lz4_decompress


def check(name: str, data: Array[u8, 1]) -> Raises[None, OSError]:
    block = lz4_compress(data)
    print(name, len(data), len(block), (lz4_decompress(block, len(data)) == data).all())


def main() -> Raises[None, OSError]:
    table = Array.fromfile(\"text.txt\", u8)
    check(\"table\", table)
    many = Array.zeros(40 * len(table), u8)
    for i in range(len(many)):
        many[i] = table[i % len(table)]
    check(\"many\", many)
    check(\"noise\", lz4_decompress(Array.fromfile(\"noise.lz4\", u8), 300))
    check(\"runs\", Array.fill(100000, u8(120)))
    for count in [0, 1, 12, 13, 14, 300]:
        check(\"cut\", many[:count].copy())
";
    let (output, _) = run("lz4_compress", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "table 11600 7519 True\n\
         many 464000 9349 True\n\
         noise 300 303 True\n\
         runs 100000 403 True\n\
         cut 0 1 True\n\
         cut 1 2 True\n\
         cut 12 13 True\n\
         cut 13 14 True\n\
         cut 14 15 True\n\
         cut 300 164 True\n"
    );
}
