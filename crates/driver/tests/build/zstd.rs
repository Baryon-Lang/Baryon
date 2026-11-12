//! The `compression.zstd` module (roadmap 3.27.7, decision 0049), built and run:
//! `decompress` reads the frames Python 3.14's `compression.zstd` wrote, one or several, with
//! and without a content size and a checksum, and a frame Python refuses raises `OSError` with
//! the kind `INVALID_DATA` and the message Python's `ZstdError` carries.
//!
//! The frames in `tests/fixtures/zstd` were written by Python; its `README.md` says how. The
//! table they hold is `tests/fixtures/zlib/text.txt`.

use super::zlib::chapter_example;
use super::{Scratch, baryon_build, package};
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Output};

/// Builds `program` as the package `name` with Python's frames and the table beside it, runs
/// it there, and returns what it printed and the package's directory, which is removed when it
/// is dropped.
///
/// # Errors
///
/// Fails when the package cannot be written or the program cannot be run.
fn run(name: &str, program: &str) -> io::Result<(Output, Scratch)> {
    let directory = package(name, &[("main.byn", program)])?;
    let fixtures = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"));
    for entry in fs::read_dir(fixtures.join("zstd"))? {
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

/// Frames at levels 3, 19 and -5, a frame with a checksum, one with no content size, and two
/// frames around a skippable frame each decompress to the table, or to forty copies of it,
/// with the CRC-32 Python's `zlib.crc32` gives.
#[test]
fn python_frames_are_decompressed() {
    let program = "\
from compression import zstd
from zlib import crc32


def main() -> Raises[None, OSError]:
    for path in [\"level3.zst\", \"level19.zst\", \"fast.zst\", \"checked.zst\", \"unsized.zst\", \"frames.zst\"]:
        data = zstd.decompress(Array.fromfile(path, u8))
        print(path, len(data), crc32(data))
";
    let (output, _) = run("zstd_frames", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "level3.zst 11600 1019181682\n\
         level19.zst 11600 1019181682\n\
         fast.zst 11600 1019181682\n\
         checked.zst 464000 787520434\n\
         unsized.zst 464000 787520434\n\
         frames.zst 11600 1019181682\n"
    );
}

/// The frames Python refuses, and empty input, raise `OSError` with the kind `INVALID_DATA`
/// and the message of Python's `ZstdError` for the same call.
#[test]
fn refused_frames_raise_python_s_messages() {
    let program = "\
from compression.zstd import decompress
from os import ErrorKind


def attempt(data: Array[u8, 1]) -> Raises[None, OSError]:
    try:
        decompress(data)
        print(\"no error\")
    except OSError as error:
        print(error.kind == ErrorKind.INVALID_DATA, error.strerror)


def main() -> Raises[None, OSError]:
    attempt(Array.zeros(0, u8))
    for path in [\"checksum.zst\", \"truncated.zst\", \"junk.zst\", \"dictionary.zst\", \"reserved.zst\", \"window.zst\"]:
        attempt(Array.fromfile(path, u8))
";
    let (output, _) = run("zstd_errors", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "True Compressed data ended before the end-of-stream marker was reached\n\
         True Unable to decompress Zstandard data: Restored data doesn't match checksum\n\
         True Compressed data ended before the end-of-stream marker was reached\n\
         True Unable to decompress Zstandard data: Unknown frame descriptor\n\
         True Unable to decompress Zstandard data: Dictionary mismatch\n\
         True Unable to decompress Zstandard data: Unsupported frame parameter\n\
         True Unable to decompress Zstandard data: Frame requires too much memory for decoding\n"
    );
}

/// The Compression chapter's Zstandard example prints what the chapter says it prints.
#[test]
fn the_reference_s_zstd_example_runs() {
    let (output, _) = run("zstd_reference", chapter_example(4)).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "11600 bytes, CRC-32 1019181682\n"
    );
}
