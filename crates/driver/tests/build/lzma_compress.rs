//! `lzma.compress` (roadmap 3.27.11, decision 0049), built and run: it writes xz streams that
//! `lzma.decompress` reads back at every preset, stores data that does not shrink, codes a long
//! run across several LZMA2 chunks, and raises Python's message for a preset outside 0 to 9.
//!
//! The streams for empty data and for one byte are byte for byte the ones Python 3.13's
//! `lzma.compress` writes, given here as hex. Python's `lzma.decompress` reads every stream
//! these programs write (checked by hand; Python does not run in the tests).

use super::{Scratch, baryon_build, package};
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Output};

/// Builds `program` as the package `name` with the table beside it, runs it there, and
/// returns what it printed and the package's directory, which is removed when it is dropped.
///
/// # Errors
///
/// Fails when the package cannot be written or the program cannot be run.
fn run(name: &str, program: &str) -> io::Result<(Output, Scratch)> {
    let directory = package(name, &[("main.byn", program)])?;
    let fixtures = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"));
    fs::copy(fixtures.join("zlib/text.txt"), directory.join("text.txt"))?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable).current_dir(&directory).output()?;
    Ok((run, directory))
}

/// The table at every preset, 200,000 bytes of noise, which are stored, and 5,000,000 zeros,
/// which take three LZMA2 chunks, read back as written; empty data and one byte give Python's
/// own streams, and presets 10 and -1 raise.
#[test]
fn lzma_compress_round_trips() {
    let program = "\
import lzma
from zlib import crc32


def hex_of(data: Array[u8, 1]) -> str:
    digits = \"0123456789abcdef\"
    pieces: list[str] = []
    for i in range(len(data)):
        value = int(data[i])
        pieces.append(digits[value >> 4] + digits[value & 15])
    return \"\".join(pieces)


def same(data: Array[u8, 1], other: Array[u8, 1]) -> bool:
    if len(data) != len(other):
        return False
    for i in range(len(data)):
        if data[i] != other[i]:
            return False
    return True


def main() -> Raises[None, OSError]:
    table = Array.fromfile(\"text.txt\", u8)
    sizes: list[int] = []
    for preset in range(10):
        packed = lzma.compress(table, preset=preset)
        sizes.append(len(packed))
        if not same(lzma.decompress(packed), table):
            print(\"preset\", preset, \"does not round-trip\")
    print(sizes)
    noise = Array.zeros(200000, u8)
    state = 1
    for i in range(len(noise)):
        state = (state * 1103515245 + 12345) & 2147483647
        noise[i] = u8(state >> 23)
    zeros = Array.zeros(5000000, u8)
    for data in [noise.copy(), zeros.copy()]:
        packed = lzma.compress(data, preset=1)
        print(len(packed), same(lzma.decompress(packed), data), crc32(packed))
    print(hex_of(lzma.compress(Array.zeros(0, u8))))
    print(hex_of(lzma.compress(table[0:1].copy())))
    for preset in [10, -1]:
        try:
            lzma.compress(table, preset=preset)
        except OSError as error:
            print(preset, error.strerror)
";
    let (output, _) = run("lzma_compress", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "[3760, 3756, 3760, 3760, 3760, 3760, 3760, 3760, 3760, 3760]\n\
         200072 True 2264492558\n\
         860 True 1158598994\n\
         fd377a585a000004e6d6b446000000001cdf44211fb6f37d010000000004595a\n\
         fd377a585a000004e6d6b4460200210116000000742fe5a30100002000000000cbf301c603e39ae4\
         00011901a52c81cc1fb6f37d010000000004595a\n\
         10 Invalid or unsupported options\n\
         -1 Invalid or unsupported options\n"
    );
}
