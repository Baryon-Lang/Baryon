//! Checking HDF5 files (decision 0052), end to end: programs that use and misuse the `h5by`
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

/// A program that reads a file well has no diagnostics, whether it imports `File` or the
/// module, and a group or a dataset is passed to a function by its class.
#[test]
fn reading_checks() {
    let program = r#"
import h5by
from h5by import Dataset, File, Group

def mean(dataset: Dataset) -> Raises[float, OSError]:
    values: Array[f64, 1] = dataset[...]
    return values.mean()

def names(group: Group) -> list[str]:
    return group.keys()

def main() -> Raises[None, OSError | KeyError | IndexError | ValueError]:
    f = File("run.h5", "r")
    temp: Array[f32, 3] = f["run1"]["temp"][0:2, :, ::2]
    print(temp, mean(f["lat"]), f["run1"].keys(), "lat" in f, "x" not in f["run1"])
    run: h5by.Group = f["run1"]
    print(names(run), run["temp"].shape, "units" in run["temp"].attrs)
    units: str = f["run1/temp"].attrs["units"]
    scale: float = run.attrs["scale"]
    print(units, scale, f.attrs.keys())
    f.close()
    with h5by.File("other.h5") as other:
        print(other.keys())
"#;
    assert_eq!(check_one(program), "");
}

/// A lookup used as a group after it took the type `Dataset`, a group indexed by numbers, an
/// untyped attribute, an array type no dataset holds, a float index, a mode not built, and a
/// member not built.
#[test]
fn what_cannot_be_read_is_explained() {
    let program = r#"
import h5by

def main() -> Raises[None, OSError | KeyError | IndexError | ValueError]:
    f = h5by.File("run.h5")
    run = f["run1"]
    temp: Array[f32, 3] = run["temp"][...]
    grid: Array[f64, 2] = f[0]
    units = f["temp"].attrs["units"]
    flags: Array[bool, 1] = f["flags"][...]
    half: Array[f64, 1] = f["lat"][0.5]
    out = h5by.File("out.h5", "w")
    h5by.get_config()
"#;
    assert_eq!(
        check_one(program),
        r#"error: `run` is an HDF5 dataset, which is not read by name
 --> main.byn:6:27
  |
6 |     temp: Array[f32, 3] = run["temp"][...]
  |                           ^^^^^^^^^^^ a dataset
  = note: if it is a group, give it the type, as `run: h5by.Group = ...` (decision 0052)

error: an HDF5 file is read by name
 --> main.byn:7:27
  |
7 |     grid: Array[f64, 2] = f[0]
  |                           ^^^^ not a name
  = note: for example `f["temp"]`, then a subscript of the dataset

error: the type of an HDF5 attribute comes from where it goes
 --> main.byn:8:13
  |
8 |     units = f["temp"].attrs["units"]
  |             ^^^^^^^^^^^^^^^^^^^^^^^^ type not known
  = note: give it a type, such as `units: str = dset.attrs["units"]`; an attribute is read as a `str`, an `int`, a `float` or an `Array[f64, 1]` (decision 0052)

error: an HDF5 dataset cannot be read into an `Array[bool, 1]`
 --> main.byn:9:29
  |
9 |     flags: Array[bool, 1] = f["flags"][...]
  |                             ^^^^^^^^^^^^^^^ not a type an HDF5 dataset holds
  = note: datasets of integers and floats are read so far, into an array such as `Array[f32, 3]` (decision 0052)

error: an HDF5 dataset is indexed by integers and slices, found `f64`
  --> main.byn:10:36
   |
10 |     half: Array[f64, 1] = f["lat"][0.5]
   |                                    ^^^ not an integer
   = note: for example `dset[...]` or `dset[0, :, ::2]`; other indexes come later under decision 0052

error: an HDF5 file is opened to read, so far
  --> main.byn:11:31
   |
11 |     out = h5by.File("out.h5", "w")
   |                               ^^^ not built yet
   = note: write the mode `"r"`; writing HDF5 files is roadmap row 3.14.6.2

error: the `h5by` module has no `get_config` yet
  --> main.byn:12:5
   |
12 |     h5by.get_config()
   |     ^^^^^^^^^^^^^^^^^ not built yet
   = note: decision 0052 builds `File`, for reading HDF5 files"#
    );
}
