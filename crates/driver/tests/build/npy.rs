//! `NumPy`'s `.npy` files (decision 0043), built and run: `Array.load` reads files `NumPy`
//! wrote, `Array.save` writes the bytes `NumPy` writes, and a file of another type, rank or
//! order raises `OSError` with the kind `INVALID_DATA`.
//!
//! The files in `tests/fixtures/npy` were written by `NumPy`; its `README.md` says how.

use super::{Scratch, baryon_build, package};
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Output};

/// The directory of files `NumPy` wrote.
fn fixtures() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/npy"))
}

/// Builds `program` as the package `name` with `NumPy`'s files beside it, runs it there, and
/// returns what it printed and the package's directory, which is removed when it is dropped.
///
/// # Errors
///
/// Fails when the package cannot be written or the program cannot be run.
fn run(name: &str, program: &str) -> io::Result<(Output, Scratch)> {
    let directory = package(name, &[("main.byn", program)])?;
    for entry in fs::read_dir(fixtures())? {
        let path = entry?.path();
        if let Some(file) = path
            .file_name()
            .filter(|file| file.as_encoded_bytes().ends_with(b".npy"))
        {
            fs::copy(&path, directory.join(file))?;
        }
    }
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable).current_dir(&directory).output()?;
    Ok((run, directory))
}

/// A program reads `NumPy`'s files of floats, integers, `bool`s, complex numbers and bytes, of
/// ranks 1 to 3, an empty one and one of format version 2.0, and prints what `NumPy` holds.
#[test]
fn numpy_files_are_read() {
    let program = "\
def main() -> Raises[None, OSError]:
    grid: Array[f64, 2] = Array.load(\"grid.npy\")
    print(grid, grid.shape)
    print(Array.load(\"column.npy\"))
    small: Array[i16, 1] = Array.load(\"i16.npy\")
    print(small)
    flags: Array[bool, 1] = Array.load(\"bool.npy\")
    print(flags)
    waves: Array[complex128, 1] = Array.load(file=\"complex128.npy\")
    print(waves)
    cube: Array[u8, 3] = Array.load(\"cube.npy\")
    print(cube.shape, cube[1, 2, 3], cube[0, 1, 0])
    empty: Array[f64, 2] = Array.load(\"empty.npy\")
    print(empty.shape)
    again: Array[f64, 2] = Array.load(\"version_2.npy\")
    print(again[1, 2])
";
    let (output, _) = run("npy_read", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "[[ 0.  1.  2.]\n \
         [10. 11. 12.]] (2, 3)\n\
         [ 1. 11.]\n\
         [  1  -2 300]\n\
         [ True False  True]\n\
         [ 1.+2.j  -0.-0.5j]\n\
         (2, 3, 4) 23 4\n\
         (0, 3)\n\
         12.0\n"
    );
}

/// A program saves arrays, a view and arrays it read, and each file holds the bytes `NumPy`
/// wrote for the same array.
#[test]
fn saved_files_match_numpy() {
    let program = "\
def main() -> Raises[None, OSError]:
    grid = Array.zeros((2, 3))
    for i in range(2):
        for j in range(3):
            grid[i, j] = 10.0 * i + j
    Array.save(\"out_grid.npy\", grid)
    Array.save(\"out_column.npy\", grid[:, 1])
    small: Array[i16, 1] = Array.load(\"i16.npy\")
    Array.save(\"out_i16.npy\", small)
    flags: Array[bool, 1] = Array.load(\"bool.npy\")
    Array.save(\"out_bool.npy\", flags)
    waves: Array[complex128, 1] = Array.load(\"complex128.npy\")
    Array.save(file=\"out_complex128.npy\", arr=waves)
    cube: Array[u8, 3] = Array.load(\"cube.npy\")
    Array.save(\"out_cube.npy\", cube)
    empty: Array[f64, 2] = Array.zeros((0, 3))
    Array.save(\"out_empty.npy\", empty)
";
    let (output, directory) = run("npy_saved", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    for name in [
        "grid",
        "column",
        "i16",
        "bool",
        "complex128",
        "cube",
        "empty",
    ] {
        let saved = fs::read(directory.join(format!("out_{name}.npy"))).unwrap();
        let numpy = fs::read(fixtures().join(format!("{name}.npy"))).unwrap();
        assert_eq!(saved, numpy, "{name}");
    }
}

/// A file of another element type or rank, in Fortran order, big-endian, or not a `.npy` file
/// at all raises `OSError` with the kind `INVALID_DATA` and a message that says what it found.
#[test]
fn files_unlike_the_program_raise_invalid_data() {
    let program = "\
def read(path: str) -> None:
    try:
        grid: Array[f64, 2] = Array.load(path)
        print(grid.shape)
    except OSError as error:
        print(error.kind, error.strerror)


def main() -> Raises[None, OSError]:
    with open(\"notes.txt\", \"w\") as notes:
        notes.write(\"1.5 2.5\\n\")
    for path in [\"i16.npy\", \"column.npy\", \"fortran.npy\", \"big_endian.npy\", \"notes.txt\"]:
        read(path)
";
    let (output, _) = run("npy_failing", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "ErrorKind.INVALID_DATA the .npy file holds elements of type '<i2', not '<f8' as the \
         program asks\n\
         ErrorKind.INVALID_DATA the .npy file holds an array of rank 1, not rank 2 as the \
         program asks\n\
         ErrorKind.INVALID_DATA the .npy file holds its elements in Fortran order; save it from \
         a C-ordered array\n\
         ErrorKind.INVALID_DATA the .npy file holds elements of type '>f8', not '<f8' as the \
         program asks\n\
         ErrorKind.INVALID_DATA the file is not a .npy file\n"
    );
}
