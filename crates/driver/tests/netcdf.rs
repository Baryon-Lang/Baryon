//! Checking `NetCDF` files (decision 0047), end to end: programs that misuse the `netCDF4`
//! module go through the parser, the syntax check, the resolver and the checker, and the
//! rendered diagnostics are compared in full.

use driver::Session;

/// Checks a package of one file, `main.byn`, and returns every diagnostic, rendered and joined
/// by blank lines. The program's first newline, which only lets a test start it on its own
/// line, is dropped.
fn check_one(text: &str) -> String {
    let mut session = Session::new();
    if let Err(error) = session.add_file("main.byn", text.strip_prefix('\n').unwrap_or(text)) {
        return error.to_string();
    }
    session
        .check()
        .iter()
        .map(|diagnostic| diagnostics::render(diagnostic, &session).to_string())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// A program that reads a file well has no diagnostics, whether it imports `Dataset` or the
/// module.
#[test]
fn reading_checks() {
    let program = r#"
import netCDF4
from netCDF4 import Dataset, Variable

def mean(variable: Variable) -> Raises[float, OSError]:
    values: Array[f64, 1] = variable[:]
    return values.mean()

def main() -> Raises[None, OSError | KeyError | AttributeError]:
    ds = Dataset("air.nc", mode="r")
    temp: Array[f32, 3] = ds.variables["air"][:]
    steps = ds.dimensions["time"].size
    units: str = ds.variables["air"].getncattr("units")
    title: str = ds.getncattr("title")
    print(temp.shape, steps, units, title, ds.ncattrs())
    print(mean(ds.variables["lat"]))
    other = netCDF4.Dataset("other.nc")
    other.close()
"#;
    assert_eq!(check_one(program), "");
}

/// An array type no `NetCDF` variable holds, a slice of a variable, an untyped attribute, a
/// new file with no format, a listing of the variables, a class not built, and an unhandled
/// `KeyError`.
#[test]
fn what_cannot_be_read_is_explained() {
    let program = r#"
from netCDF4 import Dataset, MFDataset

def main() -> Raises[None, OSError | KeyError]:
    ds = Dataset("air.nc")
    flags: Array[u8, 1] = ds.variables["flags"][:]
    first: Array[f32, 2] = ds.variables["air"][0]
    print(ds.getncattr("title"))
    out = Dataset("out.nc", "w")
    names = ds.variables
    many = MFDataset("*.nc")

def careless(ds: Dataset) -> None:
    steps = ds.dimensions["time"].size
"#;
    assert_eq!(
        check_one(program),
        r#"error: a NetCDF variable cannot be read into an `Array[u8, 1]`
 --> main.byn:5:27
  |
5 |     flags: Array[u8, 1] = ds.variables["flags"][:]
  |                           ^^^^^^^^^^^^^^^^^^^^^^^^ not a type a NetCDF variable holds
  = note: a classic NetCDF file holds signed integers and floats, such as `Array[f32, 3]`

error: a NetCDF variable is read whole, as `var[:]`, so far
 --> main.byn:6:48
  |
6 |     first: Array[f32, 2] = ds.variables["air"][0]
  |                                                ^ not built yet
  = note: slices of a variable come later under decision 0047; read it whole, then slice the array

error: the type of a NetCDF attribute comes from where it goes
 --> main.byn:7:11
  |
7 |     print(ds.getncattr("title"))
  |           ^^^^^^^^^^^^^^^^^^^^^ type not known
  = note: give it a type, such as `units: str = var.getncattr("units")`; an attribute is read as a `str`, an `int` or a `float` (decision 0047)

error: a new NetCDF file needs its `format`
 --> main.byn:8:11
  |
8 |     out = Dataset("out.nc", "w")
  |           ^^^^^^^^^^^^^^^^^^^^^^ no format
  = note: `netCDF4` writes NetCDF-4 files when the format is left out, which are not built yet; write `format="NETCDF3_CLASSIC"` or `format="NETCDF3_64BIT_OFFSET"`

error: `ds.variables` is read by name, as `ds.variables["time"]`, so far
 --> main.byn:9:13
  |
9 |     names = ds.variables
  |             ^^^^^^^^^^^^ not built yet
  = note: listing a file's variables and dimensions comes later under decision 0047

error: the `netCDF4` module has no `MFDataset` yet
  --> main.byn:10:12
   |
10 |     many = MFDataset("*.nc")
   |            ^^^^^^^^^^^^^^^^^ not built yet
   = note: decision 0047 builds `Dataset` for NetCDF classic files

error: reading a NetCDF file can fail with `KeyError`, which is not handled here
  --> main.byn:13:13
   |
13 |     steps = ds.dimensions["time"].size
   |             ^^^^^^^^^^^^^^^^^^^^^ can fail with `KeyError`
   = note: catch it with `try:` and `except KeyError:`, or let it pass on by declaring `-> Raises[None, KeyError]`"#
    );
}

/// A program that writes a file well has no diagnostics: each of `netCDF4`'s ways to name a
/// dimension's size and a variable's dimensions and type, and attributes of each kind.
#[test]
fn writing_checks() {
    let program = r#"
from netCDF4 import Dataset

def main() -> Raises[None, OSError | KeyError]:
    ds = Dataset("out.nc", "w", format="NETCDF3_64BIT_OFFSET")
    ds.setncattr("title", "grid")
    ds.setncattr(name="version", value=3)
    ds.createDimension("time")
    ds.createDimension("lat", 2)
    ds.createDimension(dimname="lon", size=3)
    temp = ds.createVariable("temp", "float32", ("time", "lat", "lon"))
    temp.setncattr("scale_factor", 0.5)
    ds.createVariable("count", "i4", "time")
    ds.createVariable(varname="total", datatype="f8")
    values: Array[f32, 3] = Array.zeros((1, 2, 3))
    temp[:] = values
    ds.variables["count"][:] = Array([7, -8])
    ds.close()
    more = Dataset("out.nc", "a")
    more.close()
"#;
    assert_eq!(check_one(program), "");
}

/// A NetCDF-4 format, a mode not built, a variable of five dimensions or with compression, an
/// attribute of a list, a slice written, values that are not numbers, and an unhandled
/// `OSError`.
#[test]
fn what_cannot_be_written_is_explained() {
    let program = r#"
from netCDF4 import Dataset

def main() -> Raises[None, OSError]:
    four = Dataset("four.nc", "w", format="NETCDF4")
    other = Dataset("other.nc", "x")
    ds = Dataset("out.nc", "w", format="NETCDF3_CLASSIC")
    ds.createVariable("v", "f8", ("a", "b", "c", "d", "e"))
    ds.createVariable("w", "f8", ("a",), zlib=True)
    ds.setncattr("weights", [0.5, 1.5])
    v = ds.createVariable("u", "f8", ("a",))
    v[0] = Array([1.0])
    v[:] = Array([True])

def unhandled(ds: Dataset):
    ds.createDimension("x", 2)
"#;
    assert_eq!(
        check_one(program),
        r#"error: a NetCDF file is written as a classic file, so far
 --> main.byn:4:43
  |
4 |     four = Dataset("four.nc", "w", format="NETCDF4")
  |                                           ^^^^^^^^^ not built yet
  = note: NetCDF-4 files ride on HDF5, which comes later (row 3.14.6); write `"NETCDF3_CLASSIC"` or `"NETCDF3_64BIT_OFFSET"`

error: `Dataset` takes the mode `"r"`, `"a"`, `"r+"` or `"w"`
 --> main.byn:5:33
  |
5 |     other = Dataset("other.nc", "x")
  |                                 ^^^ not a mode
  = note: write the mode as text in the call, such as `"w"`

error: a NetCDF variable has at most 4 dimensions, so far
 --> main.byn:7:5
  |
7 |     ds.createVariable("v", "f8", ("a", "b", "c", "d", "e"))
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not built yet
  = note: for example `ds.createVariable("temp", "f4", ("time", "lat"))`; compression and fill values come later under decision 0047

error: `createVariable` takes `varname`, `datatype` and `dimensions`, so far
 --> main.byn:8:5
  |
8 |     ds.createVariable("w", "f8", ("a",), zlib=True)
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ not built yet
  = note: for example `ds.createVariable("temp", "f4", ("time", "lat"))`; compression and fill values come later under decision 0047

error: a NetCDF attribute is a `str`, an `int` or a `float`, found `list[f64]`
 --> main.byn:9:29
  |
9 |     ds.setncattr("weights", [0.5, 1.5])
  |                             ^^^^^^^^^^ not an attribute's type

error: a NetCDF variable is written whole, as `var[:] = values`, so far
  --> main.byn:11:7
   |
11 |     v[0] = Array([1.0])
   |       ^ not built yet
   = note: slices of a variable come later under decision 0047

error: a NetCDF variable is written from an array of numbers, found `Array[bool, 1]`
  --> main.byn:12:12
   |
12 |     v[:] = Array([True])
   |            ^^^^^^^^^^^^^ not an array a NetCDF variable takes
   = note: write an array of signed integers or floats, such as `Array[f32, 3]`

error: `ds` is a read-only parameter, so it cannot be changed
  --> main.byn:15:5
   |
15 |     ds.createDimension("x", 2)
   |     ^^ changed here
   = note: take it as `InOut[...]` to change the caller's value, or change a copy: `work = ds.copy()`

error: `ds.createDimension` can fail with `OSError`, which is not handled here
  --> main.byn:15:5
   |
15 |     ds.createDimension("x", 2)
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^ can fail with `OSError`
   = note: catch it with `try:` and `except OSError:`, or let it pass on by declaring `-> Raises[None, OSError]`"#
    );
}
