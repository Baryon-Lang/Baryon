//! The private Snappy codec (roadmap 3.27.5, decision 0049 point 6), built and run:
//! `snappy_decompress` reads the raw blocks Google's library wrote and one put together by
//! hand, a block the library refuses raises `OSError` with the kind `INVALID_DATA` and the
//! message `python-snappy` gives, and `snappy_compress` writes blocks `snappy_decompress`
//! reads back.
//!
//! The codec is `<std>/parquet_snappy.byn`, which no program can import, and nothing in the
//! library calls it until the Parquet reader does (roadmap 3.14.6.3). So each test puts that
//! same file into its package, where the program imports it as its own module.
//!
//! The blocks in `tests/fixtures/snappy` were written by Google's library; its `README.md`
//! says how. The table they hold is `tests/fixtures/zlib/text.txt`.

use super::{Scratch, baryon_build, package};
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Output};

/// The codec's source, the file the standard library holds.
const CODEC: &str = include_str!("../../library/parquet_snappy.byn");

/// Builds `program` as the package `name` with the codec as a module of the package and the
/// recorded blocks and the table beside it, runs it there, and returns what it printed and
/// the package's directory, which is removed when it is dropped.
///
/// # Errors
///
/// Fails when the package cannot be written or the program cannot be run.
fn run(name: &str, program: &str) -> io::Result<(Output, Scratch)> {
    let files = [("main.byn", program), ("parquet_snappy.byn", CODEC)];
    let directory = package(name, &files)?;
    let fixtures = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"));
    for entry in fs::read_dir(fixtures.join("snappy"))? {
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
/// `zlib.crc32` gives: the table, forty copies of it in several pieces, a literal with a
/// two-byte length, copies that reach into the bytes they write, an empty block, and the
/// block made by hand whose last copy has a four-byte distance.
#[test]
fn the_library_s_blocks_are_decompressed() {
    let program = "\
from parquet_snappy import snappy_decompress
from zlib import crc32


def main() -> Raises[None, OSError]:
    for name in [\"text\", \"long\", \"noise\", \"runs\", \"empty\", \"far\"]:
        data = snappy_decompress(Array.fromfile(f\"{name}.snappy\", u8))
        print(name, len(data), crc32(data))
    table = Array.fromfile(\"text.txt\", u8)
    print((snappy_decompress(Array.fromfile(\"text.snappy\", u8)) == table).all())
";
    let (output, _) = run("snappy_blocks", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "text 11600 1019181682\n\
         long 464000 787520434\n\
         noise 300 1459503370\n\
         runs 100000 4261876081\n\
         empty 0 0\n\
         far 70000 3880973371\n\
         True\n"
    );
}

/// The blocks the library refuses raise `OSError` with the kind `INVALID_DATA` and the
/// message `python-snappy` gives for the same block: one for a length that cannot be read,
/// another for every other fault. The block that says 4 GiB is refused before anything is
/// allocated.
#[test]
fn refused_blocks_raise_the_library_s_messages() {
    let program = "\
from os import ErrorKind
from parquet_snappy import snappy_decompress


def main() -> Raises[None, OSError]:
    names = [
        \"truncated\", \"short\", \"long_length\", \"zero_distance\", \"before_start\",
        \"varint\", \"huge\", \"none\",
    ]
    for name in names:
        try:
            snappy_decompress(Array.fromfile(f\"{name}.snappy\", u8))
            print(name, \"no error\")
        except OSError as error:
            print(name, error.kind == ErrorKind.INVALID_DATA, error.strerror)
";
    let (output, _) = run("snappy_errors", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "truncated True Error while decompressing: invalid input\n\
         short True Error while decompressing: invalid input\n\
         long_length True Error while decompressing: invalid input\n\
         zero_distance True Error while decompressing: invalid input\n\
         before_start True Error while decompressing: invalid input\n\
         varint True Can not calculate uncompressed length\n\
         huge True Error while decompressing: invalid input\n\
         none True Can not calculate uncompressed length\n"
    );
}

/// `snappy_compress` writes blocks `snappy_decompress` reads back: the table, forty copies of
/// it, bytes that do not compress, one byte many times, inputs around the 15 bytes under
/// which nothing is searched and around the 65,536 bytes of a piece, and nothing. The sizes
/// are Baryon's own (decision 0049); for the table, the run and the noise they are the
/// sizes Google's library gives, and its decoder read every one of these blocks back
/// (checked when the test was written; the tests do not run it).
#[test]
fn compressed_blocks_are_read_back() {
    let program = "\
from parquet_snappy import snappy_compress, snappy_decompress


def check(name: str, data: Array[u8, 1]) -> Raises[None, OSError]:
    block = snappy_compress(data)
    print(name, len(data), len(block), (snappy_decompress(block) == data).all())


def main() -> Raises[None, OSError]:
    table = Array.fromfile(\"text.txt\", u8)
    check(\"table\", table)
    many = Array.zeros(40 * len(table), u8)
    for i in range(len(many)):
        many[i] = table[i % len(table)]
    check(\"many\", many)
    check(\"noise\", snappy_decompress(Array.fromfile(\"noise.snappy\", u8)))
    check(\"runs\", Array.fill(100000, u8(120)))
    for count in [0, 1, 14, 15, 16, 65535, 65536, 65537]:
        check(\"cut\", many[:count].copy())
";
    let (output, _) = run("snappy_compress", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "table 11600 7349 True\n\
         many 464000 72906 True\n\
         noise 300 305 True\n\
         runs 100000 4696 True\n\
         cut 0 1 True\n\
         cut 1 3 True\n\
         cut 14 16 True\n\
         cut 15 17 True\n\
         cut 16 18 True\n\
         cut 65535 9875 True\n\
         cut 65536 9875 True\n\
         cut 65537 9877 True\n"
    );
}
