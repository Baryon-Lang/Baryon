//! Writing `NetCDF` classic files (decision 0047), built and run: a program that makes the
//! same calls as the `netCDF4` scripts in `tests/fixtures/netcdf/README.md` writes the same
//! bytes, and reads back what it wrote.

use super::netcdf::fixtures;
use super::{baryon_build, package};
use std::fs;
use std::io;
use std::process::Command;

/// Builds `program` as the package `name`, runs it in its directory, and gives what it printed
/// and the bytes of the file `out.nc` it wrote.
///
/// # Errors
///
/// Fails when the package cannot be written, the program cannot be run, or it wrote no
/// `out.nc`.
fn written(name: &str, program: &str) -> io::Result<(String, Vec<u8>)> {
    let directory = package(name, &[("main.byn", program)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable).current_dir(&directory).output()?;
    assert!(run.status.success(), "{run:?}");
    let printed = String::from_utf8_lossy(&run.stdout).into_owned();
    Ok((printed, fs::read(directory.join("out.nc"))?))
}

/// Attributes of each kind, the unlimited dimension, a record written and variables left to
/// their fill values: the file `netCDF4` wrote, byte for byte, which reads back.
#[test]
fn a_classic_file_is_written_as_netcdf4_writes_it() {
    let program = "\
from netCDF4 import Dataset


def main() -> Raises[None, OSError | KeyError]:
    ds = Dataset(\"out.nc\", \"w\", format=\"NETCDF3_CLASSIC\")
    ds.setncattr(\"n\", 3)
    ds.setncattr(\"x\", 2.5)
    ds.setncattr(\"s\", \"hi\")
    ds.createDimension(\"t\", None)
    ds.createDimension(\"x\", 3)
    v = ds.createVariable(\"v\", \"i2\", (\"t\", \"x\"))
    row: Array[i64, 2] = Array.zeros((1, 3))
    for i in range(3):
        row[0, i] = i + 1
    v[:] = row
    ds.createVariable(\"w\", \"f4\", (\"x\",))
    ds.createVariable(\"z\", \"f8\", (\"t\",))
    ds.close()
    back = Dataset(\"out.nc\")
    values: Array[i64, 2] = back.variables[\"v\"][:]
    print(values, back.dimensions[\"t\"].size, back.variables[\"z\"].dimensions)
";
    let (printed, bytes) = written("netcdf_written", program).unwrap();
    assert_eq!(printed, "[[1 2 3]] 1 ['t']\n");
    assert_eq!(bytes, fs::read(fixtures().join("written.nc")).unwrap());
}

/// A 64-bit offset file, written in a `with` block, whose record variables are filled, padding
/// and all, when another one writes the records, and a variable's attribute set after the
/// values: the file `netCDF4` wrote, byte for byte, with the attribute read back at once.
#[test]
fn a_64_bit_offset_file_is_written_as_netcdf4_writes_it() {
    let program = "\
from netCDF4 import Dataset


def main() -> Raises[None, OSError | AttributeError]:
    with Dataset(\"out.nc\", \"w\", format=\"NETCDF3_64BIT_OFFSET\") as ds:
        ds.createDimension(\"t\", None)
        ds.createDimension(\"x\", 3)
        a = ds.createVariable(\"a\", \"i2\", (\"t\", \"x\"))
        ds.createVariable(\"b\", \"int8\", \"t\")
        ds.createVariable(\"c\", \"h\", (\"x\",))
        d = ds.createVariable(\"d\", \"f8\", (\"t\",))
        d[:] = Array([1.0, 2.0])
        a.setncattr(\"units\", \"m\")
        units: str = a.getncattr(\"units\")
        print(units, a.ncattrs())
";
    let (printed, bytes) = written("netcdf_filled", program).unwrap();
    assert_eq!(printed, "m ['units']\n");
    assert_eq!(bytes, fs::read(fixtures().join("filled.nc")).unwrap());
}

/// A file opened with the mode `"a"` takes a new variable and its values beside what it holds,
/// and a value too big for an attribute's 32 bits raises `OSError`.
#[test]
fn a_file_opened_to_add_to_takes_more() {
    let program = "\
from netCDF4 import Dataset


def main() -> Raises[None, OSError | KeyError]:
    ds = Dataset(\"out.nc\", \"w\", format=\"NETCDF3_CLASSIC\")
    ds.createDimension(\"x\", 2)
    ds.close()
    more = Dataset(\"out.nc\", \"a\")
    v = more.createVariable(\"v\", \"float64\", \"x\")
    v[:] = Array([1.5, 2.5])
    try:
        more.setncattr(\"big\", 3000000000)
    except OSError as error:
        print(error.kind, error.strerror)
    reread = Dataset(\"out.nc\")
    back: Array[f64, 1] = reread.variables[\"v\"][:]
    print(back)
";
    let (printed, _) = written("netcdf_added", program).unwrap();
    assert_eq!(
        printed,
        "ErrorKind.INVALID_DATA 3000000000 does not fit the 32-bit integer of a classic file\n\
         [1.5 2.5]\n",
    );
}
