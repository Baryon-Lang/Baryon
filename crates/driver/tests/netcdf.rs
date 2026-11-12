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
/// mode that writes, a listing of the variables, a class not built, and an unhandled
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

error: `Dataset` opens a file for reading only, so far
 --> main.byn:8:29
  |
8 |     out = Dataset("out.nc", "w")
  |                             ^^^ not built yet
  = note: write `"r"` or leave the mode out; writing NetCDF files comes next under decision 0047

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
