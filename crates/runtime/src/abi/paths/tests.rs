//! Tests of the exported path functions, called as compiled code calls them.

use super::*;
use crate::abi::files::OsError;
use crate::baryon_rt_deallocate;
use crate::files::ErrorKind;
use std::mem::MaybeUninit;

/// The text of an owned `str`, which it frees.
fn take(text: &OwnedText) -> String {
    let length = usize::try_from(text.length).unwrap();
    // SAFETY: the runtime made `text` with `length` initialized bytes.
    let bytes = unsafe { std::slice::from_raw_parts(text.bytes, length) };
    let copy = String::from_utf8(bytes.to_vec()).unwrap();
    // SAFETY: freed once, with its length.
    unsafe { baryon_rt_deallocate(text.bytes, length, 1) };
    copy
}

/// The texts of a `list[str]` the runtime made, which it frees.
fn take_list(list: &OwnedList) -> Vec<String> {
    let length = usize::try_from(list.length).unwrap();
    // SAFETY: the list holds `length` initialized elements.
    let elements = unsafe { std::slice::from_raw_parts(list.elements, length) };
    let texts = elements.iter().map(take).collect();
    // SAFETY: the buffer has room for `capacity` elements aligned to 8, and is freed once.
    unsafe {
        let size = usize::try_from(list.capacity).unwrap() * size_of::<OwnedText>();
        baryon_rt_deallocate(list.elements.cast(), size, 8);
    }
    texts
}

/// The kind and file name of the error a failed call wrote, which it frees.
fn raised(error: &MaybeUninit<RaisedError>) -> (u8, String) {
    // SAFETY: a failed call wrote the error.
    let OsError {
        kind,
        strerror,
        filename,
        ..
    } = &unsafe { error.assume_init_ref() }.error;
    take(strerror);
    (*kind, take(filename))
}

/// A new empty directory for one test, named after it, and its path as text.
fn scratch(test: &str) -> String {
    let directory =
        std::env::temp_dir().join(format!("baryon-abi-paths-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory.to_str().unwrap().to_owned()
}

#[test]
fn takes_each_part_of_a_path() {
    let part = |text: &str, part| {
        // SAFETY: the pointer and length are the string's.
        take(&unsafe { baryon_rt_path_part(text.as_ptr(), text.len(), part) })
    };
    assert_eq!(part("runs//2026/out.csv/", 0), "runs/2026/out.csv");
    assert_eq!(part("", 0), ".");
    assert_eq!(part("runs/2026/out.csv", 1), "out.csv");
    assert_eq!(part("runs/2026/out.csv", 2), "out");
    assert_eq!(part("runs/2026/out.csv", 3), ".csv");
    assert_eq!(part("runs/2026/out.csv", 4), "runs/2026");
}

#[test]
fn joins_and_asks() {
    let directory = scratch("asks");
    let name = "data.txt";
    // SAFETY: each pointer and length is a string's.
    let file = take(&unsafe {
        baryon_rt_path_join(
            directory.as_ptr(),
            directory.len(),
            name.as_ptr(),
            name.len(),
        )
    });
    assert_eq!(file, format!("{directory}/data.txt"));
    std::fs::write(&file, "").unwrap();
    // SAFETY: the pointer and length are the string's.
    let is = |path: &str, what| unsafe { baryon_rt_path_is(path.as_ptr(), path.len(), what) };
    assert_eq!(
        [is(&file, 0), is(&file, 1), is(&file, 2)],
        [true, true, false]
    );
    assert_eq!(
        [is(&directory, 0), is(&directory, 1), is(&directory, 2)],
        [true, false, true]
    );
    assert!(!is("", 0));
    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn makes_lists_renames_and_removes() {
    let directory = scratch("makes");
    let deep = format!("{directory}/a/b");
    let mut error = MaybeUninit::<RaisedError>::uninit();
    let make = |path: &str, parents, exist_ok, error: &mut MaybeUninit<RaisedError>| {
        // SAFETY: the pointer and length are the string's, and `error` is a local.
        unsafe {
            baryon_rt_path_make_directory(
                path.as_ptr(),
                path.len(),
                parents,
                exist_ok,
                error.as_mut_ptr(),
            )
        }
    };
    assert!(make(&deep, false, false, &mut error));
    assert_eq!(
        raised(&error),
        (ErrorKind::FileNotFound as u8, deep.clone())
    );
    assert!(!make(&deep, true, false, &mut error));
    assert!(make(&deep, true, false, &mut error));
    assert_eq!(raised(&error), (ErrorKind::FileExists as u8, deep.clone()));
    assert!(!make(&deep, true, true, &mut error));

    let (old, new) = (format!("{deep}/old.txt"), format!("{deep}/new.txt"));
    std::fs::write(&old, "").unwrap();
    // SAFETY: each pointer and length is a string's, and `error` is a local.
    let failed = unsafe {
        baryon_rt_path_rename(
            old.as_ptr(),
            old.len(),
            new.as_ptr(),
            new.len(),
            error.as_mut_ptr(),
        )
    };
    assert!(!failed);

    let mut list = MaybeUninit::<OwnedList>::uninit();
    for (joined, expected) in [(false, "new.txt".to_owned()), (true, new.clone())] {
        // SAFETY: the pointer and length are the string's; `list` and `error` are locals.
        let failed = unsafe {
            baryon_rt_path_list(
                deep.as_ptr(),
                deep.len(),
                joined,
                list.as_mut_ptr(),
                error.as_mut_ptr(),
            )
        };
        assert!(!failed);
        // SAFETY: a call that succeeded wrote the list.
        assert_eq!(take_list(unsafe { list.assume_init_ref() }), [expected]);
    }
    // SAFETY: as above.
    let failed = unsafe {
        baryon_rt_path_list(
            old.as_ptr(),
            old.len(),
            false,
            list.as_mut_ptr(),
            error.as_mut_ptr(),
        )
    };
    assert!(failed);
    assert_eq!(raised(&error), (ErrorKind::FileNotFound as u8, old.clone()));

    // SAFETY: the pointer and length are the string's, and `error` is a local.
    let remove = |error: &mut MaybeUninit<RaisedError>| unsafe {
        baryon_rt_path_remove(new.as_ptr(), new.len(), error.as_mut_ptr())
    };
    assert!(!remove(&mut error));
    assert!(remove(&mut error));
    assert_eq!(raised(&error), (ErrorKind::FileNotFound as u8, new.clone()));
    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn writes_the_current_directory() {
    let mut directory = MaybeUninit::<OwnedText>::uninit();
    let mut error = MaybeUninit::<RaisedError>::uninit();
    // SAFETY: `directory` and `error` are locals.
    let failed = unsafe { baryon_rt_current_directory(directory.as_mut_ptr(), error.as_mut_ptr()) };
    assert!(!failed);
    // SAFETY: a call that succeeded wrote the text.
    let directory = take(unsafe { directory.assume_init_ref() });
    assert_eq!(
        directory,
        std::env::current_dir().unwrap().to_str().unwrap()
    );
}
