//! Tests of NetCDF-4 reads that need the file's description changed in memory: a dimension far
//! longer than any file on disk, as a chunked dataset with no values written may declare.

use super::*;
use crate::files::netcdf::{read_header, read_variable, select};

/// The NetCDF-4 file `netCDF4` wrote for the driver's build tests.
const NETCDF4: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../driver/tests/fixtures/netcdf/netcdf4.nc"
);

#[test]
fn positions_far_apart_cost_only_themselves() {
    // `temp` grows along `time` to 2^40 records, none written past the second: picking the
    // last and the first, `temp[::-(2**40 - 1)]`, reads two records, not the span between them.
    let long = 1_u64 << 40;
    let mut header = read_header(NETCDF4).unwrap();
    let netcdf4 = header.netcdf4.as_mut().unwrap();
    netcdf4.lengths[0] = long;
    let object = netcdf4
        .file
        .objects
        .iter_mut()
        .find(|object| object.path == "/temp")
        .unwrap();
    object.dataset.as_mut().unwrap().shape[0] = long;
    let variable = header
        .variables
        .iter()
        .find(|variable| variable.name == "temp")
        .unwrap();
    let items = [Item::Slice {
        start: None,
        stop: None,
        stride: Some(-i64::try_from(long - 1).unwrap()),
    }];
    let names = ["time", "lat", "lon"];
    let lengths = [long, 2, 3];
    let selection = select("temp", &items, (&names, &lengths), true, None).unwrap();
    assert_eq!(selection.shape(), [2, 2, 3]);
    let mut memory = vec![0_u8; 12 * 8];
    read_variable(
        NETCDF4,
        &header,
        (variable, &selection),
        Pixel::F64,
        &mut memory,
        &mut |_: &[u8]| Err(String::new()),
    )
    .unwrap();
    // The first record comes second.
    let first: Vec<f64> = memory[6 * 8..]
        .chunks_exact(8)
        .map(|bytes| f64::from_ne_bytes(bytes.try_into().unwrap()))
        .collect();
    assert_eq!(first, [270.5, 271.5, 272.5, 273.5, 274.5, 275.5]);
}

#[test]
fn positions_in_equal_steps_either_way_are_one_slice() {
    let (item, within) = block(&[7, 4, 1]).unwrap();
    assert_eq!(
        item,
        Item::Slice {
            start: Some(1),
            stop: Some(8),
            stride: Some(3),
        }
    );
    assert_eq!(within, [2, 1, 0]);
    assert_eq!(block(&[0, 1, 3]), None);
    assert_eq!(block(&[2, 2]), None);
}
