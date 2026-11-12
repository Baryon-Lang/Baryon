//! Tests of the exported HDF5 functions, called as compiled code calls them, on the files
//! h5py 3.16 wrote for the driver's build tests.

use super::super::print_array::ELEMENT_F64;
use super::*;
use crate::abi::text::OwnedText;
use crate::baryon_rt_deallocate;
use std::mem::MaybeUninit;

/// The file h5py wrote with `libver="latest"`.
const LATEST: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../driver/tests/fixtures/hdf5/latest.h5"
);

/// The texts of the owned `str`s of `list`, which are then freed.
fn texts(list: &OwnedList) -> Vec<String> {
    (0..usize::try_from(list.length).unwrap())
        .map(|index| {
            // SAFETY: the list holds `length` owned `str`s.
            let owned: &OwnedText = unsafe { &*list.elements.wrapping_add(index) };
            let length = usize::try_from(owned.length).unwrap();
            // SAFETY: an owned `str` is `length` bytes of UTF-8 at `bytes`.
            let text = unsafe { std::slice::from_raw_parts(owned.bytes, length) }.to_vec();
            // SAFETY: each is an owned `str` the runtime allocated, freed once.
            unsafe { baryon_rt_deallocate(owned.bytes, length, 1) };
            String::from_utf8(text).unwrap()
        })
        .collect()
}

#[test]
fn the_deflated_chunks_are_listed_then_read_back_inflated() {
    let name = "deflated";
    let mut list = MaybeUninit::<OwnedList>::uninit();
    let mut error = MaybeUninit::<RaisedError>::uninit();
    // SAFETY: the path and the name are the strings' bytes; `list` and `error` are locals.
    let failed = unsafe {
        baryon_rt_hdf5_chunks(
            LATEST.as_ptr(),
            LATEST.len(),
            name.as_ptr(),
            name.len(),
            list.as_mut_ptr(),
            error.as_mut_ptr(),
        )
    };
    assert!(!failed);
    // SAFETY: a call that did not fail wrote the list.
    let lines = texts(&unsafe { list.assume_init() });
    // Six chunks of 4 by 2 cover the 6 by 5 dataset.
    assert_eq!(lines.len(), 6);
    let address: u64 = lines[0].split('\t').next().unwrap().parse().unwrap();
    let bytes = std::fs::read(LATEST).unwrap();
    assert_eq!(bytes[usize::try_from(address).unwrap()], 0x78);
    // Inflated chunks that are too few: the read stops at the first one missing.
    let empty = [8_i64, 0, 1];
    let mut loaded = [0_i64; 5];
    // SAFETY: as above; the empty arrays are never read and `loaded` holds `1 + 2 * 2` words.
    let failed = unsafe {
        baryon_rt_hdf5_read(
            LATEST.as_ptr(),
            LATEST.len(),
            name.as_ptr(),
            name.len(),
            empty.as_ptr(),
            empty.as_ptr(),
            ELEMENT_F64,
            2,
            loaded.as_mut_ptr(),
            error.as_mut_ptr(),
        )
    };
    assert!(failed);
    // SAFETY: a failed call wrote the error.
    let error = unsafe { error.assume_init() };
    let message = &error.error.strerror;
    // SAFETY: an owned `str` is `length` bytes of UTF-8 at `bytes`.
    let text = unsafe {
        std::slice::from_raw_parts(message.bytes, usize::try_from(message.length).unwrap())
    };
    assert_eq!(
        text,
        b"the inflated chunks ran out, in the HDF5 dataset deflated"
    );
    for owned in [&error.error.strerror, &error.error.filename] {
        // SAFETY: each is an owned `str` the runtime allocated, freed once.
        unsafe { baryon_rt_deallocate(owned.bytes, usize::try_from(owned.length).unwrap(), 1) };
    }
}

#[test]
fn the_description_starts_with_the_path() {
    let mut list = MaybeUninit::<OwnedList>::uninit();
    let mut error = MaybeUninit::<RaisedError>::uninit();
    // SAFETY: the path is the string's bytes; `list` and `error` are locals.
    let failed = unsafe {
        baryon_rt_hdf5_open(
            LATEST.as_ptr(),
            LATEST.len(),
            list.as_mut_ptr(),
            error.as_mut_ptr(),
        )
    };
    assert!(!failed);
    // SAFETY: a call that did not fail wrote the list.
    let lines = texts(&unsafe { list.assume_init() });
    assert_eq!(lines[0], format!("P\t{LATEST}"));
    assert_eq!(lines[1], "G\t/");
}
