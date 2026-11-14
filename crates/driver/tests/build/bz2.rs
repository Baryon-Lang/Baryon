//! The `bz2` module (roadmap 3.27.9, decision 0049), built and run: `decompress` reads the
//! streams Python's `bz2` wrote, one or several, and a stream Python refuses raises `OSError`
//! with the kind `INVALID_DATA` and the message Python's error carries; `compress` writes
//! streams `decompress` reads back.
//!
//! The streams in `tests/fixtures/bz2` were written by Python; its `README.md` says how. The
//! table they hold is `tests/fixtures/zlib/text.txt`.

use super::zlib::chapter_example;
use super::{Scratch, baryon_build, package};
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Output};

/// Builds `program` as the package `name` with Python's streams and the table beside it, runs
/// it there, and returns what it printed and the package's directory, which is removed when it
/// is dropped.
///
/// # Errors
///
/// Fails when the package cannot be written or the program cannot be run.
fn run(name: &str, program: &str) -> io::Result<(Output, Scratch)> {
    let directory = package(name, &[("main.byn", program)])?;
    let fixtures = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"));
    for entry in fs::read_dir(fixtures.join("bz2"))? {
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

/// Streams at levels 9 and 1, the second in several blocks, two streams one after the other,
/// and a stream followed by bytes that start no other each decompress to the table, or to
/// forty copies of it, with the CRC-32 Python's `zlib.crc32` gives.
#[test]
fn python_streams_are_decompressed() {
    let program = "\
import bz2
from zlib import crc32


def main() -> Raises[None, OSError]:
    for path in [\"level9.bz2\", \"level1.bz2\", \"streams.bz2\", \"trailing.bz2\"]:
        data = bz2.decompress(Array.fromfile(path, u8))
        print(path, len(data), crc32(data))
    print(len(bz2.decompress(Array.zeros(0, u8))))
";
    let (output, _) = run("bz2_streams", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "level9.bz2 11600 1019181682\n\
         level1.bz2 464000 787520434\n\
         streams.bz2 11600 1019181682\n\
         trailing.bz2 11600 1019181682\n\
         0\n"
    );
}

/// The streams Python refuses raise `OSError` with the kind `INVALID_DATA` and the message of
/// Python's error for the same call.
#[test]
fn refused_streams_raise_python_s_messages() {
    let program = "\
from bz2 import decompress
from os import ErrorKind


def main() -> Raises[None, OSError]:
    for path in [\"truncated.bz2\", \"crc.bz2\", \"junk.bz2\"]:
        try:
            decompress(Array.fromfile(path, u8))
            print(\"no error\")
        except OSError as error:
            print(error.kind == ErrorKind.INVALID_DATA, error.strerror)
";
    let (output, _) = run("bz2_errors", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "True Compressed data ended before the end-of-stream marker was reached\n\
         True Invalid data stream\n\
         True Invalid data stream\n"
    );
}

/// The Compression chapter's bzip2 example prints what the chapter says it prints.
#[test]
fn the_reference_s_bz2_example_runs() {
    let (output, _) = run("bz2_reference", chapter_example(6)).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "3918 bytes, back to 11600\n"
    );
}

/// `bz2.compress` writes streams that `decompress` reads back at levels 1 and 9, forty copies
/// of the table at level 1 in several blocks, and nothing from empty input; the sizes are
/// Baryon's own (decision 0049). Python's `bz2.decompress` reads every one of these streams
/// back (checked when the test was written; the tests do not run Python). A level outside 1
/// to 9 raises `ValueError` with Python's message.
#[test]
fn streams_are_compressed_at_every_level() {
    let program = "\
import bz2


def same(data: Array[u8, 1], text: Array[u8, 1]) -> bool:
    if len(data) != len(text):
        return False
    for i in range(len(text)):
        if data[i] != text[i]:
            return False
    return True


def main() -> Raises[None, OSError | ValueError]:
    text = Array.fromfile(\"text.txt\", u8)
    for level in [1, 9]:
        stream = bz2.compress(text, level)
        print(level, len(stream), same(bz2.decompress(stream), text))
    many = Array.zeros(40 * len(text), u8)
    for i in range(len(many)):
        many[i] = text[i % len(text)]
    stream = bz2.compress(many, 1)
    print(len(stream), same(bz2.decompress(stream), many))
    print(len(bz2.compress(Array.zeros(0, u8))), len(bz2.decompress(bz2.compress(Array.zeros(0, u8)))))
    for level in [0, 10]:
        try:
            bz2.compress(text, level)
        except ValueError as error:
            print(error.message)
";
    let (output, _) = run("bz2_compress", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "1 3918 True\n\
         9 3918 True\n\
         43399 True\n\
         14 0\n\
         compresslevel must be between 1 and 9\n\
         compresslevel must be between 1 and 9\n"
    );
}
