//! Tests of the exported file functions, called as compiled code calls them.

use super::*;
use crate::baryon_rt_deallocate;
use std::mem::{MaybeUninit, offset_of};

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

/// Opens `path` in `mode` as compiled code does: the file, or the error written.
fn open(path: &str, mode: u8) -> Result<*mut OpenFile, RaisedError> {
    let mut file = std::ptr::null_mut();
    let mut error = MaybeUninit::<RaisedError>::uninit();
    // SAFETY: the pointer and length are the string's; `file` and `error` are locals.
    let failed = unsafe {
        baryon_rt_file_open(
            path.as_ptr(),
            path.len(),
            mode,
            &raw mut file,
            error.as_mut_ptr(),
        )
    };
    if failed {
        // SAFETY: a failure wrote the error.
        Err(unsafe { error.assume_init() })
    } else {
        Ok(file)
    }
}

#[test]
fn an_os_error_is_laid_out_as_the_library_class_declares() {
    // `Raises[None, OSError]`: a one-byte tag, then the error at its alignment, 8.
    assert_eq!(offset_of!(RaisedError, error), 8);
    // `OSError(kind: ErrorKind, errno: i32, strerror: str, filename: str)`.
    assert_eq!(offset_of!(OsError, kind), 0);
    assert_eq!(offset_of!(OsError, errno), 4);
    assert_eq!(offset_of!(OsError, strerror), 8);
    assert_eq!(offset_of!(OsError, filename), 24);
    assert_eq!(size_of::<OsError>(), 40);
}

#[test]
fn opens_writes_reads_and_closes() {
    let directory = std::env::temp_dir().join(format!("baryon-abi-files-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("out.txt").to_str().unwrap().to_owned();

    let file = open(&path, 1).unwrap();
    let mut count = 0;
    let mut error = MaybeUninit::<RaisedError>::uninit();
    let text = "π\n";
    // SAFETY: `file` is open; the text, `count` and `error` are locals.
    let failed = unsafe {
        baryon_rt_file_write(
            file,
            text.as_ptr(),
            text.len(),
            &raw mut count,
            error.as_mut_ptr(),
        )
    };
    assert!(!failed);
    assert_eq!(count, 2);
    // SAFETY: `file` is open and used no more.
    assert!(!unsafe { baryon_rt_file_close(file, error.as_mut_ptr()) });

    let file = open(&path, 0).unwrap();
    let mut line = MaybeUninit::<OwnedText>::uninit();
    // SAFETY: `file` is open; `line` and `error` are locals.
    assert!(!unsafe { baryon_rt_file_read_line(file, line.as_mut_ptr(), error.as_mut_ptr()) });
    // SAFETY: success wrote the line.
    assert_eq!(take(&unsafe { line.assume_init() }), "π\n");
    // SAFETY: `file` is open and used no more.
    unsafe { baryon_rt_file_drop(file) };

    let missing = directory.join("missing.txt").to_str().unwrap().to_owned();
    let raised = open(&missing, 0).unwrap_err();
    assert_eq!(raised.tag, 0);
    assert_eq!(raised.error.kind, 0);
    assert_eq!(raised.error.errno, 2);
    assert_eq!(take(&raised.error.strerror), "No such file or directory");
    assert_eq!(take(&raised.error.filename), missing);
}
