//! `NetCDF` classic files (decision 0047), built and run: `Dataset` reads the files `netCDF4`
//! wrote, their variables, dimensions and attributes come out as `netCDF4` gives them, and a
//! file or a name unlike the program raises `OSError`, `KeyError` or `AttributeError`.
//!
//! The files in `tests/fixtures/netcdf` were written by `netCDF4`; its `README.md` says how.

use super::{Scratch, baryon_build, package};
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Output};

/// The directory of files `netCDF4` wrote.
fn fixtures() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/netcdf"
    ))
}

/// Builds `program` as the package `name` with `netCDF4`'s files beside it, runs it there, and
/// returns what it printed and the package's directory, which is removed when it is dropped.
///
/// # Errors
///
/// Fails when the package cannot be written or the program cannot be run.
fn run(name: &str, program: &str) -> io::Result<(Output, Scratch)> {
    let directory = package(name, &[("main.byn", program)])?;
    for file in ["classic.nc", "offset.nc"] {
        fs::copy(fixtures().join(file), directory.join(file))?;
    }
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable).current_dir(&directory).output()?;
    Ok((run, directory))
}

/// A program reads both formats' variables as the types it names, a scaled one among them,
/// and the dimensions and attributes, typed by where they go.
#[test]
fn netcdf4_files_are_read() {
    let program = "\
from netCDF4 import Dataset


def main() -> Raises[None, OSError | KeyError | AttributeError]:
    ds = Dataset(\"classic.nc\")
    temp: Array[f32, 3] = ds.variables[\"temp\"][:]
    print(temp.shape, temp[1, 1, 2])
    packed: Array[f32, 3] = ds.variables[\"packed\"][:]
    print(packed[0, 0, :])
    lat: Array[f64, 1] = ds.variables[\"lat\"][:]
    print(lat)
    flags: Array[i8, 1] = ds.variables[\"flags\"][:]
    count: Array[i64, 1] = ds.variables[\"count\"][:]
    print(flags, count)
    time = ds.dimensions[\"time\"]
    print(time.name, time.size, time.isunlimited(), ds.dimensions[\"lon\"].size)
    title: str = ds.getncattr(\"title\")
    version: int = ds.getncattr(\"version\")
    print(title, version, ds.ncattrs())
    variable = ds.variables[\"temp\"]
    units: str = variable.getncattr(\"units\")
    factor: float = ds.variables[\"packed\"].getncattr(\"scale_factor\")
    print(variable.name, variable.dimensions, units, factor, variable.ncattrs())
    ds.close()
    wide = Dataset(\"offset.nc\", \"r\")
    same: Array[f64, 3] = wide.variables[\"temp\"][:]
    print(same[0, 0, 0], same[1, 1, 2])
";
    let (output, _) = run("netcdf_read", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "(2, 2, 3) 281.5\n\
         [249.7 250.4 251.1]\n\
         [10. 20.]\n\
         [-1  0  1] [ 7 -8]\n\
         time 2 True 3\n\
         test grid 3 ['title', 'version', 'weights']\n\
         temp ['time', 'lat', 'lon'] K 0.10000000149011612 ['units']\n\
         270.5 281.5\n"
    );
}

/// A variable of another rank or a type that cannot hold it, and a file that is not `NetCDF`,
/// raise `OSError` with the kind `INVALID_DATA`; a missing variable or dimension raises
/// `KeyError`, and a missing attribute or one of another type `AttributeError`, with
/// `netCDF4`'s message.
#[test]
fn files_unlike_the_program_raise() {
    let program = "\
from netCDF4 import Dataset


def grid(ds: Dataset, name: str) -> None:
    try:
        values: Array[i16, 2] = ds.variables[name][:]
        print(values.shape)
    except OSError as error:
        print(error.kind, error.strerror)
    except KeyError as missing:
        print(\"KeyError\", missing.message)


def attribute(ds: Dataset, name: str) -> None:
    try:
        number: float = ds.getncattr(name)
        print(number)
    except AttributeError as error:
        print(\"AttributeError\", error.message)


def main() -> Raises[None, OSError]:
    with open(\"notes.txt\", \"w\") as notes:
        notes.write(\"1.5 2.5\\n\")
    ds = Dataset(\"classic.nc\")
    grid(ds, \"temp\")
    grid(ds, \"lat\")
    grid(ds, \"pressure\")
    attribute(ds, \"title\")
    attribute(ds, \"weights\")
    attribute(ds, \"history\")
    try:
        print(ds.dimensions[\"depth\"].size)
    except KeyError as missing:
        print(\"KeyError\", missing.message)
    try:
        Dataset(\"notes.txt\")
    except OSError as error:
        print(error.kind, error.strerror)
";
    let (output, _) = run("netcdf_failing", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "ErrorKind.INVALID_DATA the NetCDF variable temp has rank 3, not rank 2 as the program \
         asks\n\
         ErrorKind.INVALID_DATA the NetCDF variable lat has rank 1, not rank 2 as the program \
         asks\n\
         KeyError 'pressure'\n\
         AttributeError the attribute title holds text, not one float\n\
         AttributeError the attribute weights holds 2 floats, not one float\n\
         AttributeError NetCDF: Attribute not found\n\
         KeyError 'depth'\n\
         ErrorKind.INVALID_DATA the file is not a NetCDF file: it does not start with CDF\n"
    );
}
