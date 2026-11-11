//! Array files (decision 0036, part 4), built and run: `Array.fromfile` reads files `NumPy` wrote,
//! `a.tofile` writes the bytes `NumPy` writes, and each failure is an `OSError`.
//!
//! The files in `tests/fixtures/array_files` were written by `NumPy`; its `README.md` says how.

use super::{baryon_build, package};
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

/// The directory of files `NumPy` wrote.
fn fixtures() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/array_files"
    ))
}

/// Copies every `NumPy` file into `directory`, where the program under test runs.
fn copy_fixtures(directory: &Path) -> io::Result<()> {
    for entry in fs::read_dir(fixtures())? {
        let path = entry?.path();
        if let Some(name) = path
            .file_name()
            .filter(|name| name.as_encoded_bytes().ends_with(b".bin"))
        {
            fs::copy(&path, directory.join(name))?;
        }
    }
    Ok(())
}

/// A program reads `NumPy`'s files of floats, integers, `bool`s, complex numbers, records and
/// vectors, with a count and an offset, and prints what `NumPy` holds.
#[test]
fn numpy_files_are_read() {
    let program = "\
Vec3 = Fixed[f64, (3,)]


class Particle(Struct, Copy):
    mass: f64
    charge: i32
    alive: bool


def main() -> Raises[None, OSError]:
    field = Array.fromfile(\"f64.bin\", f64)
    print(field)
    print(Array.fromfile(\"f64.bin\", f64, count=2, offset=8))
    print(Array.fromfile(\"f64.bin\", count=0))
    print(len(Array.fromfile(\"f64.bin\", offset=4096)))
    small: Array[i16, 1] = Array.fromfile(\"i16.bin\")
    print(small, len(small))
    print(Array.fromfile(\"bool.bin\", dtype=bool))
    print(Array.fromfile(\"complex128.bin\", complex128))
    particles = Array.fromfile(\"particles.bin\", Particle)
    for i in range(len(particles)):
        print(particles[i].mass, particles[i].charge, particles[i].alive)
    rows = Array.fromfile(\"grid.bin\", Vec3)
    print(len(rows), rows[1][0], rows[1][2])
";
    let directory = package("array_files_read", &[("main.byn", program)]).unwrap();
    copy_fixtures(&directory).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable)
        .current_dir(&directory)
        .output()
        .unwrap();
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "[ 1.50e+000 -2.25e+000  1.00e+300  3.00e+000]\n\
         [-2.25e+000  1.00e+300]\n\
         []\n\
         0\n\
         [  1  -2 300] 3\n\
         [ True False  True]\n\
         [ 1.+2.j  -0.-0.5j]\n\
         1.5 -1 True\n\
         2.5 2 False\n\
         2 10.0 12.0\n"
    );
}

/// A program writes arrays, views and the value of an expression, and each file holds the
/// bytes `NumPy` wrote for the same array. A file read and written again is unchanged.
#[test]
fn written_files_match_numpy() {
    let program = "\
class Particle(Struct, Copy):
    mass: f64
    charge: i32
    alive: bool


def main() -> Raises[None, OSError]:
    grid = Array.zeros((2, 3))
    for i in range(2):
        for j in range(3):
            grid[i, j] = 10.0 * i + j
    grid.tofile(\"out_grid.bin\")
    grid[:, 1].tofile(\"out_column.bin\")
    field = Array.fromfile(\"f64.bin\")
    field[::2].tofile(\"out_every_other.bin\")
    (field * 1.0).tofile(\"out_f64.bin\")
    Array.fromfile(\"particles.bin\", Particle).tofile(\"out_particles.bin\")
    Array.fromfile(\"bool.bin\", bool).tofile(\"out_bool.bin\")
    Array.fromfile(\"complex128.bin\", complex128).tofile(\"out_complex128.bin\")
    Array.fromfile(\"i16.bin\", i16).tofile(\"out_i16.bin\")
    empty: Array[f64, 1] = Array.zeros(0)
    empty.tofile(\"out_empty.bin\")
    field.tofile(\"out_grid.bin\")
    grid.tofile(\"out_grid.bin\")
";
    let directory = package("array_files_written", &[("main.byn", program)]).unwrap();
    copy_fixtures(&directory).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable)
        .current_dir(&directory)
        .output()
        .unwrap();
    assert!(run.status.success(), "{run:?}");
    for name in [
        "grid",
        "column",
        "every_other",
        "f64",
        "particles",
        "bool",
        "complex128",
        "i16",
    ] {
        assert_eq!(
            fs::read(directory.join(format!("out_{name}.bin"))).unwrap(),
            fs::read(fixtures().join(format!("{name}.bin"))).unwrap(),
            "{name}"
        );
    }
    assert_eq!(fs::read(directory.join("out_empty.bin")).unwrap(), []);
}

/// Each failure is an `OSError` a program can catch and test: too few elements for the count,
/// bytes that are not whole elements, a missing file, a negative offset, and a directory that
/// is not there to write into. One that is not caught ends the program as any error does.
#[test]
fn failures_are_os_errors() {
    let program = "\
from os import ErrorKind


def main() -> Raises[None, OSError]:
    try:
        Array.fromfile(\"f64.bin\", f64, count=100)
    except OSError as error:
        print(error.kind == ErrorKind.INVALID_DATA, error.filename, error.errno)
        print(error.strerror)
    try:
        Array.fromfile(\"i16.bin\", f64)
    except OSError as error:
        print(error.kind == ErrorKind.INVALID_DATA, error.strerror)
    try:
        Array.fromfile(\"missing.bin\", f64)
    except OSError as error:
        print(error.kind == ErrorKind.FILE_NOT_FOUND, error.filename, error.strerror)
    try:
        Array.fromfile(\"f64.bin\", f64, offset=-8)
    except OSError as error:
        print(error.kind == ErrorKind.OTHER, error.errno, error.strerror)
    grid = Array.zeros(4)
    try:
        grid.tofile(\"no/such/directory/grid.bin\")
    except OSError as error:
        print(error.kind == ErrorKind.FILE_NOT_FOUND, error.filename)
    grid.tofile(\".\")
";
    let directory = package("array_files_failing", &[("main.byn", program)]).unwrap();
    copy_fixtures(&directory).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable)
        .current_dir(&directory)
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(1), "{run:?}");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "True f64.bin 0\n\
         the file holds 4 elements of 8 bytes after the offset, fewer than the 100 asked for\n\
         True the file holds 6 bytes after the offset, not a whole number of elements of 8 bytes\n\
         True missing.bin No such file or directory\n\
         True 22 Invalid argument\n\
         True no/such/directory/grid.bin\n"
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stderr),
        "error: OSError(kind=ErrorKind.IS_A_DIRECTORY, errno=21, strerror='Is a directory', \
         filename='.')\n"
    );
}

/// The reference's example (Files and processes, "Array files"): a grid is saved, restored into
/// another grid, and a grid of another size refuses the file.
#[test]
fn the_reference_example_saves_and_restores() {
    let program = "\
from os import ErrorKind


def save(path: str, temperature: Array[f64, 2]) -> Raises[None, OSError]:
    temperature.tofile(path)


def restore(path: str, temperature: InOut[Array[f64, 2]]) -> Raises[bool, OSError]:
    rows, columns = temperature.shape
    try:
        saved = Array.fromfile(path, f64, count=rows * columns)
    except OSError as error:
        if error.kind == ErrorKind.FILE_NOT_FOUND:
            return False
        raise
    for i in range(rows):
        for j in range(columns):
            temperature[i, j] = saved[i * columns + j]
    return True


def main() -> Raises[None, OSError]:
    grid = Array.zeros((64, 64))
    print(restore(\"heat.bin\", grid))
    grid[3, 5] = 2.5
    save(\"heat.bin\", grid)
    other = Array.zeros((64, 64))
    print(restore(\"heat.bin\", other), other[3, 5])
    larger = Array.zeros((65, 64))
    restore(\"heat.bin\", larger)
";
    let directory = package("array_files_reference", &[("main.byn", program)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable)
        .current_dir(&directory)
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(1), "{run:?}");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "False\nTrue 2.5\n");
    assert_eq!(
        String::from_utf8_lossy(&run.stderr),
        "error: OSError(kind=ErrorKind.INVALID_DATA, errno=0, strerror='the file holds 4096 \
         elements of 8 bytes after the offset, fewer than the 4160 asked for', \
         filename='heat.bin')\n"
    );
    assert_eq!(fs::read(directory.join("heat.bin")).unwrap().len(), 32768);
}
