//! The `lzma` module (roadmap 3.27.10, decision 0049), built and run: `decompress` reads the
//! xz and `.lzma` files Python's `lzma` wrote, with each kind of check, one stream or several,
//! and a file Python refuses raises `OSError` with the kind `INVALID_DATA` and the message
//! Python's `LZMAError` carries.
//!
//! The files in `tests/fixtures/xz` were written by Python; its `README.md` says how. The
//! table they hold is `tests/fixtures/zlib/text.txt`.

use super::zlib::chapter_example;
use super::{Scratch, baryon_build, package};
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Output};

/// Builds `program` as the package `name` with Python's files and the table beside it, runs
/// it there, and returns what it printed and the package's directory, which is removed when it
/// is dropped.
///
/// # Errors
///
/// Fails when the package cannot be written or the program cannot be run.
fn run(name: &str, program: &str) -> io::Result<(Output, Scratch)> {
    let directory = package(name, &[("main.byn", program)])?;
    let fixtures = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"));
    for entry in fs::read_dir(fixtures.join("xz"))? {
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

/// xz files with a CRC-64, CRC-32, SHA-256 or no check, a `.lzma` file, two streams one after
/// the other, and a stream followed by bytes that start no other each decompress to the table,
/// or to forty copies of it, with the CRC-32 Python's `zlib.crc32` gives.
#[test]
fn python_files_are_decompressed() {
    let program = "\
import lzma
from zlib import crc32


def main() -> Raises[None, OSError]:
    for path in [\"level6.xz\", \"crc32.xz\", \"sha256.xz\", \"unchecked.xz\"]:
        data = lzma.decompress(Array.fromfile(path, u8))
        print(path, len(data), crc32(data))
    for path in [\"alone.lzma\", \"streams.xz\", \"trailing.xz\"]:
        data = lzma.decompress(Array.fromfile(path, u8))
        print(path, len(data), crc32(data))
";
    let (output, _) = run("lzma_files", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "level6.xz 11600 1019181682\n\
         crc32.xz 464000 787520434\n\
         sha256.xz 11600 1019181682\n\
         unchecked.xz 11600 1019181682\n\
         alone.lzma 11600 1019181682\n\
         streams.xz 11600 1019181682\n\
         trailing.xz 11600 1019181682\n"
    );
}

/// Empty input and the files Python refuses raise `OSError` with the kind `INVALID_DATA` and
/// the message of Python's `LZMAError` for the same call; a block with the x86 filter, which
/// Baryon does not decode yet, raises the message for options it cannot decode.
#[test]
fn refused_files_raise_python_s_messages() {
    let program = "\
from lzma import decompress
from os import ErrorKind


def attempt(data: Array[u8, 1]) -> Raises[None, OSError]:
    try:
        decompress(data)
        print(\"no error\")
    except OSError as error:
        print(error.kind == ErrorKind.INVALID_DATA, error.strerror)


def main() -> Raises[None, OSError]:
    attempt(Array.zeros(0, u8))
    for path in [\"truncated.xz\", \"check.xz\", \"junk.xz\", \"x86.xz\"]:
        attempt(Array.fromfile(path, u8))
";
    let (output, _) = run("lzma_errors", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "True Compressed data ended before the end-of-stream marker was reached\n\
         True Compressed data ended before the end-of-stream marker was reached\n\
         True Corrupt input data\n\
         True Input format not supported by decoder\n\
         True Invalid or unsupported options\n"
    );
}

/// The Compression chapter's xz example prints what the chapter says it prints.
#[test]
fn the_reference_s_lzma_example_runs() {
    let (output, _) = run("lzma_reference", chapter_example(7)).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "11600 bytes, CRC-32 1019181682\n"
    );
}
