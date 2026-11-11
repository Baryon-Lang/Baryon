//! Tests of the exported array file functions, called as compiled code calls them.

use super::*;
use crate::baryon_rt_deallocate;
use std::mem::MaybeUninit;
use std::path::PathBuf;

/// A new empty directory for one test, named after it and this process.
fn directory(test: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("baryon-abi-arrays-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

/// The header of an array of `rank` dimensions as compiled code lays it out: the pointer, the
/// lengths, then the strides.
fn header(data: *const u8, lengths: &[i64], strides: &[i64]) -> Vec<i64> {
    let mut words = vec![i64::try_from(data.expose_provenance()).unwrap()];
    words.extend_from_slice(lengths);
    words.extend_from_slice(strides);
    words
}

/// The bytes of `values` as an array file holds them.
fn bytes(values: &[f64]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

/// The `count` bytes of a new array's elements, which it frees with their alignment.
fn take(array: &ArrayHeader, count: usize, align: usize) -> Vec<u8> {
    // SAFETY: the runtime allocated and filled `count` bytes.
    let copy = unsafe { std::slice::from_raw_parts(array.data, count) }.to_vec();
    // SAFETY: freed once, with the size and alignment it was allocated with.
    unsafe { baryon_rt_deallocate(array.data, count, align) };
    copy
}

/// Writes the array `words` describes, of `f64`s, to `path` as compiled code does: the bytes
/// the file then holds, as `f64`s.
fn to_file(path: &str, words: &[i64], rank: usize) -> Vec<f64> {
    let mut error = MaybeUninit::<RaisedError>::uninit();
    // SAFETY: the path is the string's bytes, `words` is a header of `rank` dimensions whose
    // pointer has the exposed address of the test's values, and `error` is a local.
    let failed = unsafe {
        baryon_rt_array_to_file(
            path.as_ptr(),
            path.len(),
            words.as_ptr(),
            rank,
            8,
            error.as_mut_ptr(),
        )
    };
    assert!(!failed);
    std::fs::read(path)
        .unwrap()
        .chunks_exact(8)
        .map(|bytes| f64::from_le_bytes(bytes.try_into().unwrap()))
        .collect()
}

/// Reads `path` as compiled code does, with elements of `layout`: the new array's header, or
/// the error written.
fn from_file(
    path: &str,
    layout: &ElementLayout,
    count: i64,
    offset: i64,
) -> Result<ArrayHeader, RaisedError> {
    let mut array = MaybeUninit::<ArrayHeader>::uninit();
    let mut error = MaybeUninit::<RaisedError>::uninit();
    // SAFETY: the path is the string's bytes, and the rest are locals.
    let failed = unsafe {
        baryon_rt_array_from_file(
            path.as_ptr(),
            path.len(),
            layout,
            count,
            offset,
            array.as_mut_ptr(),
            error.as_mut_ptr(),
        )
    };
    if failed {
        // SAFETY: a failure wrote the error.
        Err(unsafe { error.assume_init() })
    } else {
        // SAFETY: a success wrote the header.
        Ok(unsafe { array.assume_init() })
    }
}

/// The layout of an `f64`.
const F64: ElementLayout = ElementLayout {
    size: 8,
    align: 8,
    bool_count: 0,
    bools: std::ptr::null(),
};

#[test]
fn a_header_is_laid_out_as_compiled_code_reads_it() {
    assert_eq!(std::mem::offset_of!(ArrayHeader, data), 0);
    assert_eq!(std::mem::offset_of!(ArrayHeader, length), 8);
    assert_eq!(std::mem::offset_of!(ArrayHeader, stride), 16);
    assert_eq!(std::mem::offset_of!(ElementLayout, bools), 24);
}

#[test]
fn writes_an_array_and_reads_it_back() {
    let directory = directory("round-trip");
    let path = directory.join("field.bin").to_str().unwrap().to_owned();
    let values = [1.5_f64, -2.5, 3.25, 1e-300];
    let data = values.as_ptr().cast::<u8>();
    assert_eq!(to_file(&path, &header(data, &[4], &[1]), 1), values);

    let array = from_file(&path, &F64, -1, 0).unwrap();
    assert_eq!((array.length, array.stride), (4, 1));
    assert_eq!(take(&array, 32, 8), bytes(&values));

    let block = from_file(&path, &F64, 2, 8).unwrap();
    assert_eq!(block.length, 2);
    assert_eq!(take(&block, 16, 8), bytes(&values[1..3]));
}

#[test]
fn views_are_written_in_row_major_order() {
    let directory = directory("views");
    let path = directory.join("view.bin").to_str().unwrap().to_owned();
    // A 3 by 4 matrix, row-major: element `[i, j]` is `10 i + j`.
    let values: Vec<f64> = (0..3)
        .flat_map(|row| (0..4).map(move |column| f64::from(10 * row + column)))
        .collect();
    let data = values.as_ptr().cast::<u8>();
    // The whole matrix: one run.
    assert_eq!(to_file(&path, &header(data, &[3, 4], &[4, 1]), 2), values);
    // Rows 0 and 2, every column: two runs of a row each.
    assert_eq!(
        to_file(&path, &header(data, &[2, 4], &[8, 1]), 2),
        [0.0, 1.0, 2.0, 3.0, 20.0, 21.0, 22.0, 23.0]
    );
    // Every row, columns 1 and 3: a run for each element.
    // SAFETY: element 1 is within the matrix.
    let second = unsafe { data.add(8) };
    assert_eq!(
        to_file(&path, &header(second, &[3, 2], &[4, 2]), 2),
        [1.0, 3.0, 11.0, 13.0, 21.0, 23.0]
    );
    // The transpose: strides 1 and 4.
    assert_eq!(
        to_file(&path, &header(data, &[4, 3], &[1, 4]), 2),
        [
            0.0, 10.0, 20.0, 1.0, 11.0, 21.0, 2.0, 12.0, 22.0, 3.0, 13.0, 23.0
        ]
    );
    // An empty array writes an empty file.
    assert_eq!(to_file(&path, &header(data, &[0, 4], &[4, 1]), 2), []);
}

#[test]
fn failures_are_raised_as_os_errors() {
    let directory = directory("failures");
    let path = directory.join("short.bin").to_str().unwrap().to_owned();
    std::fs::write(&path, [0_u8; 12]).unwrap();
    let error = from_file(&path, &F64, -1, 0).unwrap_err();
    assert_eq!(error.tag, 0);
    assert_eq!(error.error.kind, crate::files::ErrorKind::InvalidData as u8);
    assert_eq!(error.error.errno, 0);
    for text in [&error.error.strerror, &error.error.filename] {
        // SAFETY: each is an owned `str` the runtime allocated, freed once.
        unsafe { baryon_rt_deallocate(text.bytes, usize::try_from(text.length).unwrap(), 1) };
    }
    let missing = directory.join("none.bin").to_str().unwrap().to_owned();
    let error = from_file(&missing, &F64, -1, 0).unwrap_err();
    assert_eq!(
        error.error.kind,
        crate::files::ErrorKind::FileNotFound as u8
    );
    assert_eq!(error.error.errno, 2);
}

#[test]
fn bools_in_an_element_read_as_zero_or_one() {
    let directory = directory("bools");
    let path = directory.join("flags.bin").to_str().unwrap().to_owned();
    std::fs::write(&path, [3_u8, 0, 1, 200]).unwrap();
    let offsets = [0_usize];
    let layout = ElementLayout {
        size: 1,
        align: 1,
        bool_count: 1,
        bools: offsets.as_ptr(),
    };
    let array = from_file(&path, &layout, -1, 0).unwrap();
    assert_eq!(take(&array, 4, 1), [1, 0, 1, 1]);
}
