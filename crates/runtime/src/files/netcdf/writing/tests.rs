//! Tests of writing `NetCDF` classic files: each makes, change by change, a file `netCDF4` 1.7.2
//! wrote with the same calls (the driver's `tests/fixtures/netcdf`, whose `README.md` holds the
//! script), and compares the bytes.

use super::*;
use crate::files::netcdf::{read_header, read_variable};
use std::fs;

/// A path for one test's file, named after it and this process.
fn file(test: &str) -> String {
    let path = std::env::temp_dir().join(format!(
        "baryon-netcdf-write-{}-{test}.nc",
        std::process::id()
    ));
    path.to_str().unwrap().to_owned()
}

/// The bytes of the fixture `name` that `netCDF4` wrote.
fn fixture(name: &str) -> Vec<u8> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../driver/tests/fixtures/netcdf/"
    );
    fs::read(format!("{path}{name}")).unwrap()
}

/// Applies each of `lines` to the file at `path`.
fn apply(path: &str, lines: &[&str]) {
    for line in lines {
        define(path, line).unwrap();
    }
}

/// The machine-order bytes of `values`.
fn longs(values: &[i64]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect()
}

#[test]
fn attributes_and_a_record_match_netcdf4() {
    let path = file("written");
    apply(
        &path,
        &[
            "C\t1",
            "A\t\tint\tn\t3",
            "A\t\tfloat\tx\t2.5",
            "A\t\tstr\ts\thi",
            "D\tt\t0",
            "D\tx\t3",
            "V\tv\ti2\tt,x",
        ],
    );
    write(&path, ("v", &[]), Pixel::I64, &[1, 3], &longs(&[1, 2, 3])).unwrap();
    apply(&path, &["V\tw\tf4\tx", "V\tz\tf8\tt"]);
    assert_eq!(fs::read(&path).unwrap(), fixture("written.nc"));
}

#[test]
fn values_not_written_are_filled_as_netcdf4_fills() {
    let path = file("filled");
    apply(
        &path,
        &[
            "C\t2",
            "D\tt\t0",
            "D\tx\t3",
            "V\ta\ti2\tt,x",
            "V\tb\ti1\tt",
            "V\tc\ti2\tx",
            "V\td\tf8\tt",
        ],
    );
    let doubles: Vec<u8> = [1.0_f64, 2.0]
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect();
    write(&path, ("d", &[]), Pixel::F64, &[2], &doubles).unwrap();
    apply(&path, &["A\ta\tstr\tunits\tm"]);
    assert_eq!(fs::read(&path).unwrap(), fixture("filled.nc"));
}

#[test]
fn a_new_record_is_filled_padding_and_all_before_it_is_written() {
    let path = file("padded");
    apply(
        &path,
        &["C\t1", "D\tt\t0", "D\tx\t3", "V\ta\ti2\tt,x", "V\tb\ti1\tt"],
    );
    write(&path, ("a", &[]), Pixel::I64, &[1, 3], &longs(&[4, 5, 6])).unwrap();
    assert_eq!(fs::read(&path).unwrap(), fixture("padded.nc"));
}

#[test]
fn the_only_record_variable_is_unpadded_until_another_joins() {
    let path = file("single");
    apply(&path, &["C\t1", "D\tt\t0", "V\te\ti2\tt"]);
    write(&path, ("e", &[]), Pixel::I64, &[3], &longs(&[5, 6, 7])).unwrap();
    assert_eq!(fs::read(&path).unwrap(), fixture("single.nc"));
    apply(&path, &["V\tf\ti2\tt"]);
    assert_eq!(fs::read(&path).unwrap(), fixture("moved.nc"));
    let header = read_header(&path).unwrap();
    let mut memory = vec![0_u8; 3 * 8];
    let variable = &header.variables[0];
    let selection = header.everything(variable);
    read_variable(
        &path,
        &header,
        (variable, &selection),
        Pixel::I64,
        &mut memory,
    )
    .unwrap();
    assert_eq!(memory, longs(&[5, 6, 7]));
}

#[test]
fn what_the_format_refuses_is_refused() {
    let path = file("refused");
    apply(&path, &["C\t1", "D\tt\t0", "D\tx\t2", "V\tv\tf8\tx"]);
    let cases = [
        ("D\tt\t4", "NetCDF: String match to name in use"),
        ("D\tu\t0", "NetCDF: NC_UNLIMITED size already in use"),
        ("V\tw\tf8\tx,t", "NetCDF: NC_UNLIMITED in the wrong index"),
        ("V\tw\tf8\ty", "the NetCDF file has no dimension y"),
        (
            "V\tw\tu2\tx",
            "u2 is not a type a classic NetCDF file holds",
        ),
        ("A\tw\tstr\tunits\tK", "the NetCDF file has no variable w"),
        (
            "A\t\tint\tbig\t3000000000",
            "3000000000 does not fit the 32-bit integer of a classic file",
        ),
    ];
    for (line, message) in cases {
        let failure = define(&path, line).unwrap_err();
        assert_eq!(failure.kind, ErrorKind::InvalidData, "{line}");
        assert_eq!(failure.strerror, message, "{line}");
    }
    let failure = write(&path, ("v", &[]), Pixel::F64, &[3], &[0; 24]).unwrap_err();
    assert_eq!(
        failure.strerror,
        "the NetCDF variable v has the shape (2), which an array of shape (3) does not fill"
    );
}

#[test]
fn an_attributes_text_may_hold_tabs() {
    let path = file("tabs");
    apply(&path, &["C\t1", "A\t\tstr\tnote\ta\tb"]);
    let header = read_header(&path).unwrap();
    assert_eq!(
        crate::files::netcdf::describe(&header),
        ["A\t\tstr\t1\tnote\ta\tb"]
    );
}

#[test]
fn a_header_that_claims_more_than_the_file_holds_is_refused() {
    // `written.nc` with its record count, bytes 4 to 8, raised to 2^31 - 1: a write must report
    // the short file, not try to hold two billion records in memory.
    let mut bytes = fixture("written.nc");
    bytes[4..8].copy_from_slice(&0x7FFF_FFFF_u32.to_be_bytes());
    let path = file("claims");
    fs::write(&path, bytes).unwrap();
    for failure in [
        define(&path, "D\ty\t2").unwrap_err(),
        write(&path, ("w", &[]), Pixel::F64, &[3], &[0; 24]).unwrap_err(),
    ] {
        assert_eq!(failure.kind, ErrorKind::InvalidData);
        assert_eq!(
            failure.strerror,
            "the file ends inside the NetCDF variable v"
        );
    }
}

#[test]
fn sizes_past_the_formats_32_bits_are_refused() {
    let path = file("sizes");
    apply(&path, &["C\t2", "D\tx\t4000000000", "D\ty\t4"]);
    let cases = [
        ("D\tz\t5000000000", "NetCDF: Invalid dimension size"),
        (
            "V\tv\tf8\tx,y",
            "NetCDF: One or more variable sizes violate format constraints",
        ),
    ];
    for (line, message) in cases {
        let failure = define(&path, line).unwrap_err();
        assert_eq!(failure.kind, ErrorKind::InvalidData, "{line}");
        assert_eq!(failure.strerror, message, "{line}");
    }
    assert!(read_header(&path).unwrap().variables.is_empty());
}

#[test]
fn slices_write_what_netcdf4_writes() {
    let every = Item::Slice {
        start: None,
        stop: None,
        stride: None,
    };
    // `a[2] = [4, 5, 6]`, which adds three records, then `a[0, ::2] = [7, 8]`.
    let path = file("rows");
    apply(
        &path,
        &["C\t1", "D\tt\t0", "D\tx\t3", "V\ta\ti2\tt,x", "V\tb\ti1\tt"],
    );
    write(
        &path,
        ("a", &[Item::Index(2)]),
        Pixel::I64,
        &[3],
        &longs(&[4, 5, 6]),
    )
    .unwrap();
    let alternate = Item::Slice {
        start: None,
        stop: None,
        stride: Some(2),
    };
    let items = [Item::Index(0), alternate];
    write(&path, ("a", &items), Pixel::I64, &[2], &longs(&[7, 8])).unwrap();
    assert_eq!(fs::read(&path).unwrap(), fixture("rows.nc"));
    // `a[1, 1] = 9` of the only record variable.
    let path = file("point");
    apply(&path, &["C\t1", "D\tt\t0", "D\tx\t3", "V\ta\ti2\tt,x"]);
    let items = [Item::Index(1), Item::Index(1)];
    write(&path, ("a", &items), Pixel::I64, &[], &longs(&[9])).unwrap();
    assert_eq!(fs::read(&path).unwrap(), fixture("point.nc"));
    // `c[1:3, -1] = [1.5, 2.5]` of a fixed variable.
    let path = file("grid");
    apply(&path, &["C\t1", "D\tx\t4", "D\ty\t3", "V\tc\tf4\tx,y"]);
    let middle = Item::Slice {
        start: Some(1),
        stop: Some(3),
        stride: None,
    };
    let values: Vec<u8> = [1.5_f64, 2.5]
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect();
    let items = [middle, Item::Index(-1)];
    write(&path, ("c", &items), Pixel::F64, &[2], &values).unwrap();
    assert_eq!(fs::read(&path).unwrap(), fixture("grid.nc"));
    // An array of another shape than the values picked is refused.
    let failure = write(
        &path,
        ("c", &[every, Item::Index(0)]),
        Pixel::F64,
        &[2],
        &values,
    )
    .unwrap_err();
    assert_eq!(
        failure.strerror,
        "the values picked of the NetCDF variable c have the shape (4), which an array of shape \
         (2) does not fill"
    );
}
