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
            std::ptr::null(),
            0,
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
    assert_eq!(list.length, 16);
    // SAFETY: the list holds 16 owned `str`s; the first is read.
    let first = unsafe { &*list.elements };
    // SAFETY: and the second.
    let second = unsafe { &*list.elements.wrapping_add(1) };
    // SAFETY: an owned `str` is `length` bytes at `bytes`.
    let path = unsafe { std::slice::from_raw_parts(first.bytes, 2 + CLASSIC.len()) };
    assert_eq!(path, format!("P\t{CLASSIC}").as_bytes());
    // SAFETY: as above; `D\ttime\t2\t1` is 10 bytes.
    let text = unsafe { std::slice::from_raw_parts(second.bytes, 10) };
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

#[test]
fn writing_a_variable_the_file_does_not_have_raises_invalid_data() {
    let path =
        std::env::temp_dir().join(format!("baryon-netcdf-abi-write-{}.nc", std::process::id()));
    let path = path.to_str().unwrap();
    let mut error = MaybeUninit::<RaisedError>::uninit();
    let line = "C\t1";
    let mut lines = MaybeUninit::<OwnedList>::uninit();
    // SAFETY: the path and the line are the strings' bytes; `lines` and `error` are locals.
    let failed = unsafe {
        baryon_rt_netcdf_define(
            path.as_ptr(),
            path.len(),
            line.as_ptr(),
            line.len(),
            lines.as_mut_ptr(),
            error.as_mut_ptr(),
        )
    };
    assert!(!failed);
    let mut values = [1_i32, 2];
    let array = [values.as_mut_ptr() as i64, 2, 1];
    // SAFETY: as above, and `array` describes `values`.
    let failed = unsafe {
        baryon_rt_netcdf_write(
            path.as_ptr(),
            path.len(),
            "v".as_ptr(),
            1,
            std::ptr::null(),
            0,
            array.as_ptr(),
            1,
            ELEMENT_I32,
            error.as_mut_ptr(),
        )
    };
    assert!(failed);
    // SAFETY: a failed call wrote the error.
    let error = unsafe { error.assume_init() };
    assert_eq!(
        raised(&error),
        (
            ErrorKind::InvalidData as u8,
            "the NetCDF file has no variable v".to_owned()
        )
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn a_subscript_that_picks_nothing_raises_index_error_or_value_error() {
    // `temp[5]` of two records, then `temp[::0]`, each read as one `f32` axis fewer.
    let cases: [([i64; 4], u8, &str); 2] = [
        (
            [0, 5, i64::MIN, i64::MIN],
            1,
            "index 5 is out of bounds for the dimension time of length 2",
        ),
        ([1, i64::MIN, i64::MIN, 0], 2, "slice step cannot be zero"),
    ];
    for (item, tag, message) in cases {
        let name = "temp";
        let mut words = [0_i64; 5];
        let mut error = MaybeUninit::<RaisedError>::uninit();
        // SAFETY: the path and the name are the strings' bytes, `item` is one item's four
        // words, `words` holds `1 + 2 * 2` words, and `error` is a local the size of an
        // `OSError`, more than an `IndexError` or `ValueError` needs.
        let failed = unsafe {
            baryon_rt_netcdf_read(
                CLASSIC.as_ptr(),
                CLASSIC.len(),
                name.as_ptr(),
                name.len(),
                item.as_ptr(),
                1,
                ELEMENT_F32,
                2,
                words.as_mut_ptr(),
                error.as_mut_ptr(),
            )
        };
        assert!(failed);
        // SAFETY: the failed call wrote the tag and, where an `OSError` starts, the message:
        // a `RaisedMessage`.
        let RaisedMessage {
            tag: written,
            message: text,
        } = unsafe { error.as_ptr().cast::<RaisedMessage>().read() };
        let length = usize::try_from(text.length).unwrap();
        // SAFETY: an owned `str` is `length` bytes of UTF-8 at `bytes`, freed once here.
        let bytes = unsafe { std::slice::from_raw_parts(text.bytes, length) }.to_vec();
        // SAFETY: as above.
        unsafe { baryon_rt_deallocate(text.bytes, length, 1) };
        assert_eq!(
            (written, String::from_utf8(bytes).unwrap()),
            (tag, message.to_owned())
        );
    }
}
