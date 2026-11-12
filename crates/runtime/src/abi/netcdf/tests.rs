//! Tests of the exported `NetCDF` functions, called as compiled code calls them, on the files
//! `netCDF4` 1.7.2 wrote for the driver's build tests.

use super::super::print_array::{ELEMENT_F32, ELEMENT_I16, ELEMENT_I32};
use super::*;
use crate::baryon_rt_deallocate;
use std::mem::MaybeUninit;

/// The classic file `netCDF4` wrote.
const CLASSIC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../driver/tests/fixtures/netcdf/classic.nc"
);

/// Reads the variable `name` of `path` as `rank` dimensions of the type numbered `element`,
/// `size` bytes each: the shape and the elements' bytes, which are then freed, or the error
/// raised.
fn read(
    path: &str,
    name: &str,
    element: u32,
    size: usize,
    rank: usize,
) -> Result<(Vec<i64>, Vec<u8>), RaisedError> {
    let mut words = vec![0_i64; 1 + 2 * rank];
    let mut error = MaybeUninit::<RaisedError>::uninit();
    // SAFETY: the path and the name are the strings' bytes, `words` holds `1 + 2 * rank` words
    // and `error` is a local.
    let failed = unsafe {
        baryon_rt_netcdf_read(
            path.as_ptr(),
            path.len(),
            name.as_ptr(),
            name.len(),
            element,
            rank,
            words.as_mut_ptr(),
            error.as_mut_ptr(),
        )
    };
    if failed {
        // SAFETY: a failed call wrote the error.
        return Err(unsafe { error.assume_init() });
    }
    let count: i64 = words[1..=rank].iter().product();
    let bytes = usize::try_from(count).unwrap() * size;
    let data = std::ptr::with_exposed_provenance_mut::<u8>(usize::try_from(words[0]).unwrap());
    // SAFETY: the runtime allocated `bytes` bytes; they are copied, then freed once.
    let elements = unsafe { std::slice::from_raw_parts(data, bytes) }.to_vec();
    // SAFETY: as above.
    unsafe { baryon_rt_deallocate(data, bytes, size) };
    Ok((words[1..].to_vec(), elements))
}

/// Frees the `str`s an `OSError` the runtime raised owns, and gives its kind and message.
fn raised(error: &RaisedError) -> (u8, String) {
    let message = &error.error.strerror;
    // SAFETY: an owned `str` is `length` bytes of UTF-8 at `bytes`.
    let text = unsafe {
        std::slice::from_raw_parts(message.bytes, usize::try_from(message.length).unwrap())
    };
    let text = String::from_utf8(text.to_vec()).unwrap();
    for owned in [&error.error.strerror, &error.error.filename] {
        // SAFETY: each is an owned `str` the runtime allocated, freed once.
        unsafe { baryon_rt_deallocate(owned.bytes, usize::try_from(owned.length).unwrap(), 1) };
    }
    (error.error.kind, text)
}

#[test]
fn a_record_variable_reads_with_the_records_as_its_first_axis() {
    let (words, bytes) = read(CLASSIC, "temp", ELEMENT_F32, 4, 3).unwrap();
    // The lengths, then the row-major strides.
    assert_eq!(words, [2, 2, 3, 6, 3, 1]);
    let last = f32::from_ne_bytes(bytes[44..].try_into().unwrap());
    assert!((last - 281.5).abs() < f32::EPSILON);
}

#[test]
fn another_rank_type_or_name_raises_invalid_data() {
    let invalid = ErrorKind::InvalidData as u8;
    let cases = [
        (
            "temp",
            ELEMENT_F32,
            2,
            "the NetCDF variable temp has rank 3, not rank 2 as the program asks",
        ),
        (
            "count",
            ELEMENT_I16,
            1,
            "the NetCDF variable count holds i4, which does not read as `i16`; read it as a \
             float array",
        ),
        (
            "pressure",
            ELEMENT_I32,
            1,
            "the NetCDF file has no variable pressure",
        ),
    ];
    for (name, element, rank, message) in cases {
        let error = read(CLASSIC, name, element, 4, rank).unwrap_err();
        assert_eq!(raised(&error), (invalid, message.to_owned()));
    }
}

#[test]
fn the_header_lines_describe_the_file() {
    let mut list = MaybeUninit::<OwnedList>::uninit();
    let mut error = MaybeUninit::<RaisedError>::uninit();
    // SAFETY: the path is the string's bytes; `list` and `error` are locals.
    let failed = unsafe {
        baryon_rt_netcdf_open(
            CLASSIC.as_ptr(),
            CLASSIC.len(),
            list.as_mut_ptr(),
            error.as_mut_ptr(),
        )
    };
    assert!(!failed);
    // SAFETY: a call that did not fail wrote the list.
    let list = unsafe { list.assume_init() };
    assert_eq!(list.length, 15);
    // SAFETY: the list holds 15 owned `str`s; the first is read.
    let first = unsafe { &*list.elements };
    // SAFETY: an owned `str` is `length` bytes at `bytes`; `D\ttime\t2\t1` is 10.
    let text = unsafe { std::slice::from_raw_parts(first.bytes, 10) };
    assert_eq!(text, b"D\ttime\t2\t1");
}

#[test]
fn a_missing_file_raises_not_found() {
    let path = "/nonexistent/baryon-netcdf.nc";
    let mut list = MaybeUninit::<OwnedList>::uninit();
    let mut error = MaybeUninit::<RaisedError>::uninit();
    // SAFETY: the path is the string's bytes; `list` and `error` are locals.
    let failed = unsafe {
        baryon_rt_netcdf_open(
            path.as_ptr(),
            path.len(),
            list.as_mut_ptr(),
            error.as_mut_ptr(),
        )
    };
    assert!(failed);
    // SAFETY: a failed call wrote the error.
    let error = unsafe { error.assume_init() };
    assert_eq!(raised(&error).0, ErrorKind::FileNotFound as u8);
}
