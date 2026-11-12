//! Tests of the exported `.npy` functions, called as compiled code calls them.

use super::*;
use crate::baryon_rt_deallocate;
use crate::files::npy::header;
use std::mem::MaybeUninit;

/// A path for one test's file, named after it and this process.
fn file(test: &str) -> String {
    let path =
        std::env::temp_dir().join(format!("baryon-abi-npy-{}-{test}.npy", std::process::id()));
    path.to_str().unwrap().to_owned()
}

/// The layout of an `i32`.
const I32: ElementLayout = ElementLayout {
    size: 4,
    align: 4,
    bool_count: 0,
    bools: std::ptr::null(),
};

/// Loads the `.npy` file at `path` as an array of `rank` dimensions of the type `descr` with
/// the layout `element`: the header words, or the error raised.
fn load(
    path: &str,
    element: &ElementLayout,
    descr: &str,
    rank: usize,
) -> Result<Vec<i64>, RaisedError> {
    let mut words = vec![0_i64; 1 + 2 * rank];
    let mut error = MaybeUninit::<RaisedError>::uninit();
    // SAFETY: the path and the type are the strings' bytes, the layout is a local, `words`
    // holds `1 + 2 * rank` words and `error` is a local.
    let failed = unsafe {
        baryon_rt_array_load(
            path.as_ptr(),
            path.len(),
            element,
            descr.as_ptr(),
            descr.len(),
            rank,
            words.as_mut_ptr(),
            error.as_mut_ptr(),
        )
    };
    if failed {
        // SAFETY: a failed call wrote the error.
        Err(unsafe { error.assume_init() })
    } else {
        Ok(words)
    }
}

/// Frees the `str`s an `OSError` the runtime raised owns.
fn free(error: &RaisedError) {
    for text in [&error.error.strerror, &error.error.filename] {
        // SAFETY: each is an owned `str` the runtime allocated, freed once.
        unsafe { baryon_rt_deallocate(text.bytes, usize::try_from(text.length).unwrap(), 1) };
    }
}

#[test]
fn a_numpy_file_loads_with_its_shape_and_row_major_strides() {
    let path = file("load");
    let mut bytes = header("<i4", &[2, 3]);
    bytes.extend((0_i32..6).flat_map(i32::to_le_bytes));
    std::fs::write(&path, bytes).unwrap();
    let words = load(&path, &I32, "<i4", 2).unwrap();
    assert_eq!(words[1..], [2, 3, 3, 1]);
    let data = std::ptr::with_exposed_provenance_mut::<u8>(usize::try_from(words[0]).unwrap());
    // SAFETY: the runtime allocated six `i32`s, 24 bytes; they are read, then freed once.
    let bytes = unsafe { std::slice::from_raw_parts(data, 24) }.to_vec();
    // SAFETY: as above.
    unsafe { baryon_rt_deallocate(data, 24, 4) };
    let expected: Vec<u8> = (0_i32..6).flat_map(i32::to_le_bytes).collect();
    assert_eq!(bytes, expected);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn a_view_saves_its_own_elements_after_numpys_header() {
    let path = file("save");
    let mut values = [0_i32, 1, 2, 3, 4, 5];
    // The second column of the 2 by 3 matrix: a view of two elements, stride 3.
    // SAFETY: element 1 is within `values`.
    let data = unsafe { values.as_mut_ptr().add(1) };
    let words = [i64::try_from(data.expose_provenance()).unwrap(), 2, 3];
    let mut error = MaybeUninit::<RaisedError>::uninit();
    // SAFETY: the path and the type are the strings' bytes, `words` is a header of one
    // dimension whose pointer has the exposed address of `values`, and `error` is a local.
    let failed = unsafe {
        baryon_rt_array_save(
            path.as_ptr(),
            path.len(),
            words.as_ptr(),
            1,
            4,
            "<i4".as_ptr(),
            3,
            error.as_mut_ptr(),
        )
    };
    assert!(!failed);
    let mut expected = header("<i4", &[2]);
    expected.extend([1_i32, 4].iter().flat_map(|value| value.to_le_bytes()));
    assert_eq!(std::fs::read(&path).unwrap(), expected);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn another_type_rank_or_a_short_file_is_invalid_data() {
    let path = file("mismatch");
    let mut bytes = header("<i4", &[2, 3]);
    bytes.extend((0_i32..5).flat_map(i32::to_le_bytes));
    std::fs::write(&path, bytes).unwrap();
    for (descr, rank) in [("<f4", 2), ("<i4", 1), ("<i4", 2)] {
        let error = load(&path, &I32, descr, rank).unwrap_err();
        assert_eq!(error.tag, 0);
        assert_eq!(
            error.error.kind,
            ErrorKind::InvalidData as u8,
            "{descr} {rank}"
        );
        free(&error);
    }
    let missing = file("none");
    let error = load(&missing, &I32, "<i4", 1).unwrap_err();
    assert_eq!(error.error.kind, ErrorKind::FileNotFound as u8);
    free(&error);
    std::fs::remove_file(path).unwrap();
}
