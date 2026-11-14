//! `gzip.open` (roadmap 3.27.4, decision 0049), built and run: a `GzipFile` reads the members
//! Python wrote, whole or in pieces, and a file of several members and many 64 KiB blocks; it
//! writes files Python reads back, adds members with `"ab"`, and raises Python's errors for a
//! bad file, a bad mode and a closed file.
//!
//! The gzip files in `tests/fixtures/zlib` were written by Python; its `README.md` says how.

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

/// The helper both programs use: whether two byte arrays hold the same bytes.
const SAME: &str = "\
def same(data: Array[u8, 1], text: Array[u8, 1]) -> bool:
    if len(data) != len(text):
        return False
    for i in range(len(text)):
        if data[i] != text[i]:
            return False
    return True
";

/// Python's files read back whole and 1,000 bytes at a time, across the boundary between two
/// members. A file of 4.5 MB written at level 0 holds two members and 70 blocks of 64 KiB, and
/// reads back, whole and in pieces of 100,000 bytes, as the bytes written; `"ab"` adds a member
/// after it. `with` closes the file, and `close` a second time does nothing.
#[test]
fn gzip_files_are_streamed() {
    let program = format!(
        "\
import gzip


{SAME}

def pieces(path: str, size: int) -> Raises[int, OSError | ValueError]:
    count = 0
    with gzip.open(path) as f:
        piece = f.read(size)
        while len(piece) > 0:
            count += 1
            piece = f.read(size)
    return count


def main() -> Raises[None, OSError | ValueError]:
    text = Array.fromfile(\"text.txt\", u8)
    for path in [\"member.gz\", \"members.gz\", \"fields.gz\"]:
        with gzip.open(path, \"rb\") as f:
            print(path, same(f.read(), text), len(f.read()))
        print(pieces(path, 1000))
    data = Array.zeros(4500000, u8)
    state = 1
    for i in range(len(data)):
        state = (state * 1103515245 + 12345) & 2147483647
        data[i] = u8(state >> 23)
    with gzip.open(\"large.gz\", \"wb\", compresslevel=0) as g:
        g.write(data[0:1000].copy())
        print(g.write(data[1000:].copy()))
    with gzip.open(\"large.gz\") as f:
        print(same(f.read(), data))
    print(pieces(\"large.gz\", 100000))
    with gzip.open(\"large.gz\", \"a\") as g:
        g.write(text)
    h = gzip.open(\"large.gz\", \"r\")
    print(same(h.read(len(data)), data), same(h.read(), text))
    h.close()
    h.close()
"
    );
    let (output, directory) = run("gzip_open", &program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "member.gz True 0\n\
         12\n\
         members.gz True 0\n\
         12\n\
         fields.gz True 0\n\
         12\n\
         4499000\n\
         True\n\
         45\n\
         True True\n"
    );
    let size = fs::metadata(directory.join("large.gz")).unwrap().len();
    assert!(size > 4_500_000, "{size}");
}

/// A file that is not gzip, one cut short and one that is missing raise `OSError` with
/// Python's messages; a text mode, an unknown mode, `"xb"` on a file that exists, and reading a
/// closed file or one opened to write raise errors with Python's words.
#[test]
fn gzip_open_raises_python_s_errors() {
    let program = "\
from os import ErrorKind
import gzip


def attempt(path: str, mode: str):
    try:
        with gzip.open(path, mode) as f:
            print(len(f.read()))
    except OSError as error:
        print(mode, error.kind == ErrorKind.INVALID_DATA, error.strerror)
    except ValueError as problem:
        print(mode, problem.message)


def main() -> Raises[None, OSError | ValueError]:
    packed = Array.fromfile(\"member.gz\", u8)
    packed[0:100].copy().tofile(\"cut.gz\")
    attempt(\"text.txt\", \"rb\")
    attempt(\"cut.gz\", \"rb\")
    attempt(\"missing.gz\", \"rb\")
    attempt(\"member.gz\", \"rt\")
    attempt(\"member.gz\", \"rw\")
    attempt(\"member.gz\", \"xb\")
    attempt(\"member.gz\", \"wb\")
    h = gzip.open(\"cut.gz\")
    h.close()
    try:
        h.read()
    except ValueError as problem:
        print(problem.message)
";
    let (output, _) = run("gzip_open_errors", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "rb True Not a gzipped file (b'  ')\n\
         rb True Compressed file ended before the end-of-stream marker was reached\n\
         rb False No such file or directory\n\
         rt gzip.open's text mode 'rt' is not built yet; open with \"rb\" or \"wb\"\n\
         rw invalid mode: 'rw'\n\
         xb False File exists\n\
         wb False read() on write-only GzipFile object\n\
         I/O operation on closed file\n"
    );
}

/// The example in the `gzip.open` section of the compression reference, as written there.
#[test]
fn the_reference_example_prints_what_it_says() {
    let program = "\
import gzip


def main() -> Raises[None, OSError | ValueError]:
    with gzip.open(\"table.txt.gz\", \"wb\") as f:
        f.write(Array.fromfile(\"text.txt\", u8))
    total = 0
    with gzip.open(\"table.txt.gz\") as f:
        piece = f.read(1000)
        while len(piece) > 0:
            total += len(piece)
            piece = f.read(1000)
    print(total, \"bytes\")
";
    let (output, _) = run("gzip_open_example", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "11600 bytes\n");
}
