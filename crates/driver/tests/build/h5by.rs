//! HDF5 files (decision 0052), built and run: `h5by.File` reads the files h5py wrote, their
//! datasets, whole or picked by a subscript, deflated or not, and their attributes come out as
//! h5py gives them, and a name or a subscript unlike the file raises `KeyError`, `IndexError`
//! or `ValueError` with h5py's messages.
//!
//! The files in `tests/fixtures/hdf5` were written by h5py; its `README.md` says how.

use super::{Scratch, baryon_build, package};
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Output};

/// Builds `program` as the package `name` with h5py's files beside it, runs it there, and
/// returns what it printed and the package's directory, which is removed when it is dropped.
///
/// # Errors
///
/// Fails when the package cannot be written or the program cannot be run.
fn run(name: &str, program: &str) -> io::Result<(Output, Scratch)> {
    let fixtures = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/hdf5"));
    let directory = package(name, &[("main.byn", program)])?;
    for file in ["earliest.h5", "latest.h5"] {
        fs::copy(fixtures.join(file), directory.join(file))?;
    }
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable).current_dir(&directory).output()?;
    Ok((run, directory))
}

/// A program reads datasets whole and in part, deflated ones among them, as the types it
/// names, and the attributes, groups and names, typed by where they go.
#[test]
fn h5by_files_are_read() {
    let program = "\
import h5by


def main() -> Raises[None, OSError | KeyError | IndexError | ValueError]:
    f = h5by.File(\"latest.h5\")
    temp: Array[f32, 3] = f[\"run1/temp\"][...]
    print(temp.shape, temp[1, 2, 3])
    part: Array[f64, 2] = f[\"run1\"][\"temp\"][1, ::2, 1:3]
    print(part)
    grid: Array[f64, 2] = f[\"deflated\"][...]
    print(grid[5, :])
    picked: Array[f64, 1] = f[\"deflated\"][5, ::3]
    print(picked)
    count: Array[i32, 1] = f[\"count\"][...]
    print(count)
    dset = f[\"run1/temp\"]
    print(dset.name, dset.shape, dset.ndim, dset.size, dset.dtype)
    units: str = dset.attrs[\"units\"]
    title: str = f.attrs[\"title\"]
    version: int = f.attrs[\"version\"]
    weights: Array[f64, 1] = f.attrs[\"weights\"]
    print(units, title, version, weights)
    print(f.keys())
    print(f[\"run1\"].keys(), \"run1/temp\" in f, \"nope\" in f, \"units\" in dset.attrs)
    run: h5by.Group = f[\"run1\"]
    print(run.name, run.keys(), \"temp\" in run, dset.attrs.keys())
    f.close()
    with h5by.File(\"earliest.h5\", \"r\") as old:
        last: Array[i64, 1] = old[\"long\"][-3:]
        big: Array[i16, 1] = old[\"big\"][...]
        print(last, big)
";
    let (output, _) = run("h5by_read", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "(2, 3, 4) 293.5\n\
         [[283.5 284.5]\n [291.5 292.5]]\n\
         [12.5 13.  13.5 14.  14.5]\n\
         [12.5 14. ]\n\
         [ 7 -8  9]\n\
         /run1/temp [2, 3, 4] 3 24 <f4\n\
         K test run 3 [0.5  1.25]\n\
         ['big', 'compact', 'count', 'deflated', 'grow', 'grow2', 'long', 'many', 'plain', \
         'run1', 'scalar', 'shuffled', 'single']\n\
         ['temp'] True False True\n\
         /run1 ['temp'] True ['units']\n\
         [397 398 399] [  1  -2 300]\n"
    );
}

/// A missing object or attribute raises h5py's `KeyError`, a subscript that picks nothing its
/// `IndexError` or `ValueError`, and a file that is not HDF5 `OSError`, each caught by the
/// program.
#[test]
fn h5by_errors_are_raised_as_h5py_raises_them() {
    let program = "\
import h5by


def main() -> Raises[None, OSError]:
    f = h5by.File(\"latest.h5\")
    for name in [\"nope\", \"run1/nope\", \"nope/temp\"]:
        try:
            values: Array[f64, 1] = f[name][...]
            print(values)
        except KeyError as missing:
            print(missing.message)
    try:
        units: str = f[\"count\"].attrs[\"units\"]
        print(units)
    except KeyError as absent:
        print(absent.message)
    try:
        row: Array[f64, 2] = f[\"run1/temp\"][5]
        print(row)
    except IndexError as outside:
        print(outside.message)
    except ValueError as wrong:
        print(wrong.message)
    except KeyError as gone:
        print(gone.message)
    try:
        backwards: Array[f64, 3] = f[\"run1/temp\"][::-1]
        print(backwards)
    except IndexError as past:
        print(past.message)
    except ValueError as negative:
        print(negative.message)
    except KeyError as lost:
        print(lost.message)
    try:
        h5by.File(\"package/main.byn\")
    except OSError as error:
        print(error.strerror)
";
    let (output, _) = run("h5by_errors", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "\"Unable to synchronously open object (object 'nope' doesn't exist)\"\n\
         \"Unable to synchronously open object (object 'nope' doesn't exist)\"\n\
         'Unable to synchronously open object (component not found)'\n\
         \"Unable to synchronously open attribute (can't locate attribute: 'units')\"\n\
         Index (5) out of range for (0-1)\n\
         Step must be >= 1 (got -1)\n\
         the file is not an HDF5 file\n"
    );
}
