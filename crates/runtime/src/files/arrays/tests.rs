//! Tests of array files: each reads and writes real files in a directory of its own under the
//! system's temporary directory.

use super::*;
use std::path::PathBuf;

/// A new empty directory for one test, named after it and this process.
fn directory(test: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("baryon-arrays-{}-{test}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

/// The path of `name` in `directory`, as a program would pass it.
fn path(directory: &std::path::Path, name: &str) -> String {
    directory.join(name).to_str().unwrap().to_owned()
}

/// An element of `size` bytes with no `bool`s.
fn plain(size: usize) -> Element<'static> {
    Element { size, bools: &[] }
}

/// Reads as [`read`] does, into a new vector: the bytes read.
fn read_bytes(
    path: &str,
    element: Element<'_>,
    count: i64,
    offset: i64,
) -> Result<Vec<u8>, OsFailure> {
    let mut memory = Vec::new();
    let elements = read(path, element, count, offset, |elements| {
        memory = vec![0; elements * element.size];
        &mut memory
    })?;
    assert_eq!(memory.len(), elements * element.size);
    Ok(memory)
}

#[test]
fn writes_then_reads_back_the_same_bytes() {
    let directory = directory("round-trip");
    let file = path(&directory, "field.bin");
    let values = [1.5_f64, -2.25, 1e300];
    let bytes: Vec<u8> = values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    write(&file, |writer| writer.write_all(&bytes)).unwrap();
    assert_eq!(fs::read(&file).unwrap(), bytes);
    assert_eq!(read_bytes(&file, plain(8), -1, 0).unwrap(), bytes);
    // A second write empties the file first.
    write(&file, |writer| writer.write_all(&bytes[..8])).unwrap();
    assert_eq!(read_bytes(&file, plain(8), -1, 0).unwrap(), bytes[..8]);
}

#[test]
fn count_and_offset_choose_a_block() {
    let directory = directory("block");
    let file = path(&directory, "data.bin");
    let bytes: Vec<u8> = (0..40).collect();
    fs::write(&file, &bytes).unwrap();
    assert_eq!(read_bytes(&file, plain(4), 3, 8).unwrap(), bytes[8..20]);
    assert_eq!(read_bytes(&file, plain(4), -1, 32).unwrap(), bytes[32..]);
    assert_eq!(read_bytes(&file, plain(4), 0, 0).unwrap(), []);
    // Past the end there is nothing to read, which is an empty array.
    assert_eq!(read_bytes(&file, plain(4), -1, 400).unwrap(), []);
    // With a count, bytes after the block need not be whole elements.
    assert_eq!(read_bytes(&file, plain(16), 2, 1).unwrap(), bytes[1..33]);
}

#[test]
fn a_file_that_is_too_short_or_ragged_is_invalid_data() {
    let directory = directory("short");
    let file = path(&directory, "data.bin");
    fs::write(&file, [0_u8; 20]).unwrap();
    let ragged = read_bytes(&file, plain(8), -1, 0).unwrap_err();
    assert_eq!(ragged.kind, ErrorKind::InvalidData);
    assert_eq!(ragged.errno, 0);
    assert_eq!(
        ragged.strerror,
        "the file holds 20 bytes after the offset, not a whole number of elements of 8 bytes"
    );
    assert_eq!(ragged.filename, file);
    let short = read_bytes(&file, plain(8), 3, 4).unwrap_err();
    assert_eq!(short.kind, ErrorKind::InvalidData);
    assert_eq!(
        short.strerror,
        "the file holds 2 elements of 8 bytes after the offset, fewer than the 3 asked for"
    );
}

#[test]
fn failures_are_the_operating_systems() {
    let directory = directory("failures");
    let missing = path(&directory, "missing.bin");
    let error = read_bytes(&missing, plain(8), -1, 0).unwrap_err();
    assert_eq!(error.kind, ErrorKind::FileNotFound);
    assert_eq!(error.errno, 2);
    assert_eq!(error.filename, missing);
    let folder = directory.to_str().unwrap();
    let error = read_bytes(folder, plain(8), -1, 0).unwrap_err();
    assert_eq!(error.kind, ErrorKind::IsADirectory);
    let error = write(folder, |_| Ok(())).unwrap_err();
    assert_eq!(error.kind, ErrorKind::IsADirectory);
    let file = path(&directory, "data.bin");
    fs::write(&file, [0_u8; 8]).unwrap();
    let error = read_bytes(&file, plain(8), -1, -1).unwrap_err();
    assert_eq!((error.kind, error.errno), (ErrorKind::Other, EINVAL));
    assert_eq!(error.strerror, "Invalid argument");
}

#[test]
fn bools_read_as_zero_or_one() {
    let directory = directory("bools");
    let file = path(&directory, "flags.bin");
    // Two elements of `{ bool, u8, bool }`.
    fs::write(&file, [0, 7, 2, 255, 9, 1]).unwrap();
    let element = Element {
        size: 3,
        bools: &[0, 2],
    };
    assert_eq!(
        read_bytes(&file, element, -1, 0).unwrap(),
        [0, 7, 1, 1, 9, 1]
    );
}

#[test]
fn an_element_with_no_bytes_reads_as_many_as_asked() {
    let directory = directory("empty-element");
    let file = path(&directory, "data.bin");
    fs::write(&file, [1_u8, 2, 3]).unwrap();
    let count = |count| read(&file, plain(0), count, 0, |_| &mut []).unwrap();
    assert_eq!(count(5), 5);
    assert_eq!(count(-1), 0);
}

/// A pipe has no length to ask for, so it is read to its end: a FIFO another thread writes.
#[cfg(unix)]
#[test]
fn a_pipe_is_read_to_its_end() {
    let directory = directory("pipe");
    let fifo = path(&directory, "pipe");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(made.success());
    let bytes: Vec<u8> = (0..24).collect();
    for (count, offset, expected) in [(-1, 0, &bytes[..]), (2, 4, &bytes[4..20])] {
        let (writer, sent) = (fifo.clone(), bytes.clone());
        let thread = std::thread::spawn(move || fs::write(writer, sent).unwrap());
        assert_eq!(
            read_bytes(&fifo, plain(8), count, offset).unwrap(),
            expected
        );
        thread.join().unwrap();
    }
}
