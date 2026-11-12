//! The `zlib` module (roadmap 3.27.1, decision 0049), built and run: `zlib.decompress` reads
//! the streams Python's `zlib` and `gzip` wrote, in every container `wbits` names, `crc32` and
//! `adler32` give Python's checksums, and a corrupt stream raises `OSError` with the kind
//! `INVALID_DATA` and the message Python's `zlib.error` carries.
//!
//! The files in `tests/fixtures/zlib` were written by Python; its `README.md` says how.

use super::{Scratch, baryon_build, package};
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Output};

/// Builds `program` as the package `name` with Python's files beside it, runs it there, and
/// returns what it printed and the package's directory, which is removed when it is dropped.
///
/// # Errors
///
/// Fails when the package cannot be written or the program cannot be run.
fn run(name: &str, program: &str) -> io::Result<(Output, Scratch)> {
    let directory = package(name, &[("main.byn", program)])?;
    let fixtures = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/zlib"));
    for entry in fs::read_dir(fixtures)? {
        let path = entry?.path();
        if let Some(file) = path.file_name() {
            fs::copy(&path, directory.join(file))?;
        }
    }
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable).current_dir(&directory).output()?;
    Ok((run, directory))
}

/// A program decompresses the streams Python wrote at levels 0, 1, 6 and 9, with fixed codes,
/// as a raw deflate stream and as a gzip member, through each `wbits` form, and each gives back
/// `text.txt`; the checksums are Python's, a whole and continued from a first part.
#[test]
fn python_streams_are_decompressed() {
    let program = "\
import zlib
from zlib import adler32, crc32, decompress


def same(a: Array[u8, 1], b: Array[u8, 1]) -> bool:
    if len(a) != len(b):
        return False
    for i in range(len(a)):
        if a[i] != b[i]:
            return False
    return True


def check(path: str, wbits: int) -> Raises[None, OSError]:
    text = Array.fromfile(\"text.txt\", u8)
    print(path, wbits, same(decompress(Array.fromfile(path, u8), wbits), text))


def main() -> Raises[None, OSError]:
    for path in [\"level0.z\", \"level1.z\", \"level6.z\", \"level9.z\", \"fixed.z\"]:
        check(path, 15)
    check(\"level6.z\", 0)
    check(\"level6.z\", 47)
    check(\"raw.deflate\", -15)
    check(\"member.gz\", 31)
    check(\"member.gz\", 47)
    text = Array.fromfile(\"text.txt\", u8)
    print(len(zlib.decompress(Array.fromfile(\"level9.z\", u8), bufsize=16)))
    print(crc32(text), adler32(text))
    print(crc32(text[0:100]), adler32(text[100:], adler32(text[0:100])))
";
    let (output, _) = run("zlib_decompress", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "level0.z 15 True\n\
         level1.z 15 True\n\
         level6.z 15 True\n\
         level9.z 15 True\n\
         fixed.z 15 True\n\
         level6.z 0 True\n\
         level6.z 47 True\n\
         raw.deflate -15 True\n\
         member.gz 31 True\n\
         member.gz 47 True\n\
         11600\n\
         1019181682 1438716371\n\
         2543373900 1438716371\n"
    );
}

/// Corrupt streams, a stream of the wrong container, a window larger than `wbits` allows and a
/// `wbits` Python rejects each raise `OSError` with the kind `INVALID_DATA` and the message of
/// Python's `zlib.error` for the same call.
#[test]
fn corrupt_streams_raise_python_s_messages() {
    let program = "\
from os import ErrorKind
from zlib import decompress


def attempt(path: str, wbits: int) -> Raises[None, OSError]:
    data = Array.fromfile(path, u8)
    try:
        decompress(data, wbits)
        print(\"no error\")
    except OSError as error:
        print(error.kind == ErrorKind.INVALID_DATA, error.strerror)


def main() -> Raises[None, OSError]:
    attempt(\"truncated.z\", 15)
    attempt(\"check.z\", 15)
    attempt(\"block.z\", 15)
    attempt(\"dictionary.z\", 15)
    attempt(\"member.gz\", 15)
    attempt(\"level6.z\", 31)
    attempt(\"level6.z\", 9)
    attempt(\"level6.z\", 7)
";
    let (output, _) = run("zlib_errors", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "True Error -5 while decompressing data: incomplete or truncated stream\n\
         True Error -3 while decompressing data: incorrect data check\n\
         True Error -3 while decompressing data: invalid block type\n\
         True Error 2 while decompressing data\n\
         True Error -3 while decompressing data: incorrect header check\n\
         True Error -3 while decompressing data: incorrect header check\n\
         True Error -3 while decompressing data: invalid window size\n\
         True Error -2 while preparing to decompress data: inconsistent stream state\n"
    );
}

/// The worked example at the top of the reference's Compression chapter, read from the chapter
/// itself, prints what the chapter says it prints and writes the decompressed table.
#[test]
fn the_reference_example_runs() {
    let chapter = include_str!("../../../../docs/reference/compression.md");
    let start = chapter.find("```python\n").unwrap() + "```python\n".len();
    let program = &chapter[start..start + chapter[start..].find("```").unwrap()];
    let (output, directory) = run("zlib_reference", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "11600 bytes, CRC-32 1019181682\n"
    );
    assert_eq!(
        fs::read(directory.join("table.txt")).unwrap(),
        fs::read(directory.join("text.txt")).unwrap()
    );
}
