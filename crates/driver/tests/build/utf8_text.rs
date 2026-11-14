//! UTF-8 text from bytes and back (decision 0064, roadmap 1.13.18), built and run:
//! `text.encode()` and `data.decode()`, with Python's messages for bytes that are not UTF-8, and
//! the text modes of `gzip.open` and `lzma.open`.
//!
//! Each program prints what the same program prints in Python 3.13: the same lines, the same
//! `"\r\n"` and `"\r"` turned into `"\n"`, and the same messages. The gzip and xz files
//! Python wrote are in `tests/fixtures/zlib` and `tests/fixtures/xz`.

use super::zlib::chapter_example;
use super::{Scratch, baryon_build, package};
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Output};

/// Builds `program` as the package `name` with Python's gzip and xz files of the table beside
/// it, runs it there, and returns what it printed and the package's directory, which is
/// removed when it is dropped.
///
/// # Errors
///
/// Fails when the package cannot be written or the program cannot be run.
fn run(name: &str, program: &str) -> io::Result<(Output, Scratch)> {
    let directory = package(name, &[("main.byn", program)])?;
    let fixtures = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"));
    for file in ["zlib/member.gz", "zlib/text.txt", "xz/level6.xz"] {
        let path = fixtures.join(file);
        if let Some(name) = path.file_name() {
            fs::copy(&path, directory.join(name))?;
        }
    }
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable).current_dir(&directory).output()?;
    Ok((run, directory))
}

/// `encode` gives the text's UTF-8 bytes, `decode` gives them back, from a strided slice too,
/// and bytes that are not UTF-8 raise `ValueError` with Python's message: a byte no character
/// starts with, and a character cut short.
#[test]
fn text_turns_into_utf_8_and_back() {
    let program = "\
def attempt(data: Array[u8, 1]):
    try:
        print(data.decode())
    except ValueError as problem:
        print(problem.message)


def main() -> Raises[None, ValueError]:
    data = \"π ≈ 3.14159 €\".encode()
    print(len(data), data[0], data[1], data.decode())
    print(\"abcdef\".encode()[::2].decode())
    bad = \"abc!\".encode()
    bad[3] = u8(255)
    attempt(bad)
    attempt(\"é\".encode()[0:1].copy())
";
    let (output, _) = run("utf8_bytes", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "18 207 128 π ≈ 3.14159 €\n\
         ace\n\
         'utf-8' codec can't decode byte 0xff in position 3: invalid start byte\n\
         'utf-8' codec can't decode byte 0xc3 in position 0: unexpected end of data\n"
    );
}

/// `gzip.open` writes text with `"wt"` and `"at"` and reads it back line by line, with
/// `readline`, `readlines` and `read`, turning `"\r\n"` and `"\r"` into `"\n"`; it reads
/// Python's file as the table. A line of 30,000 three-byte characters, whose character at byte
/// 65,536 is cut by the 64 KiB block, and a `"\r\n"` cut by the next block, read whole.
/// A byte that is not UTF-8 raises Python's message, `"xt"` on a file that exists raises
/// `OSError`, and `lzma.open` reads Python's xz file as the table.
#[test]
fn gzip_and_lzma_read_and_write_text() {
    let program = "\
import gzip
import lzma


def lines_of(path: str) -> Raises[list[str], OSError | ValueError]:
    with gzip.open(path, \"rt\") as f:
        return f.readlines()


def main() -> Raises[None, OSError | ValueError]:
    with gzip.open(\"notes.gz\", \"wt\") as out:
        print(out.write(\"first é\\n\"), out.write(\"second\\r\\nthird\\rfourth\"))
    with gzip.open(\"notes.gz\", \"at\") as out:
        out.write(\"\\nfifth\\n\")
    for line in gzip.open(\"notes.gz\", mode=\"rt\", encoding=\"utf-8\"):
        print(len(line), line, end=\"\")
    f = gzip.open(\"notes.gz\", \"rt\")
    print(f.readline(), f.readlines(), len(f.read()))
    f.close()
    table = Array.fromfile(\"text.txt\", u8).decode()
    with gzip.open(\"member.gz\", \"rt\") as f:
        print(f.read() == table)
    euros = \"\".join([\"€\" for i in range(30000)])
    ayes = \"\".join([\"a\" for i in range(65535)])
    with gzip.open(\"long.gz\", \"wt\") as out:
        out.write(f\"{euros}\\n{ayes}\\r\\nb\")
    print([len(line) for line in lines_of(\"long.gz\")])
    bad = \"abc!\".encode()
    bad[3] = u8(255)
    with gzip.open(\"bad.gz\", \"wb\") as raw:
        raw.write(bad)
    try:
        print(lines_of(\"bad.gz\"))
    except ValueError as problem:
        print(problem.message)
    try:
        gzip.open(\"notes.gz\", \"xt\")
    except OSError as error:
        print(error.strerror)
    with lzma.open(\"level6.xz\", \"rt\") as x:
        print(x.read() == table)
    x = lzma.open(\"level6.xz\", \"rt\")
    count = 0
    for line in x:
        count += 1
    print(count, x.readline() == \"\")
    x.close()
";
    let (output, _) = run("utf8_files", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "8 20\n\
         8 first é\n\
         7 second\n\
         6 third\n\
         7 fourth\n\
         6 fifth\n\
         first é\n ['second\\n', 'third\\n', 'fourth\\n', 'fifth\\n'] 0\n\
         True\n\
         [30001, 65536, 1]\n\
         'utf-8' codec can't decode byte 0xff in position 3: invalid start byte\n\
         File exists\n\
         True\n\
         400 True\n"
    );
}

/// The example in the text-modes section of the compression reference, read from the chapter
/// itself, prints what the chapter says it prints.
#[test]
fn the_reference_example_prints_what_it_says() {
    let (output, _) = run("utf8_example", chapter_example(5)).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "T = 293.15 K\nÅ = 1e-10 m\n"
    );
}
