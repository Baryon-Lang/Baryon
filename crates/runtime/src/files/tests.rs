//! Tests of text files: each opens real files in a directory of its own under the system's
//! temporary directory.

use super::*;
use std::path::PathBuf;

/// A new empty directory for one test, named after it and this process.
fn directory(test: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("baryon-files-{}-{test}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

/// The path of `name` in `directory`, as a program would pass it.
fn path(directory: &std::path::Path, name: &str) -> String {
    directory.join(name).to_str().unwrap().to_owned()
}

#[test]
fn writes_then_reads_back_lines_with_any_line_end() {
    let directory = directory("lines");
    let file = path(&directory, "data.txt");
    let mut out = OpenFile::open(&file, Mode::Write).unwrap();
    assert_eq!(out.write("α=1\r\nβ=2\rγ=3\n").unwrap(), 13);
    assert_eq!(out.write("end").unwrap(), 3);
    out.close().unwrap();

    let mut input = OpenFile::open(&file, Mode::Read).unwrap();
    assert_eq!(input.read_line().unwrap(), "α=1\n");
    assert_eq!(input.read_lines().unwrap(), ["β=2\n", "γ=3\n", "end"]);
    assert_eq!(input.read_line().unwrap(), "");
    input.close().unwrap();

    let mut whole = OpenFile::open(&file, Mode::Read).unwrap();
    assert_eq!(whole.read().unwrap(), "α=1\nβ=2\nγ=3\nend");
    assert_eq!(whole.read().unwrap(), "");
}

#[test]
fn modes_append_and_refuse_to_overwrite() {
    let directory = directory("modes");
    let file = path(&directory, "log.txt");
    let mut created = OpenFile::open(&file, Mode::Create).unwrap();
    created.write("a").unwrap();
    created.close().unwrap();
    let exists = OpenFile::open(&file, Mode::Create).unwrap_err();
    assert_eq!(exists.kind, ErrorKind::FileExists);
    assert_eq!(exists.errno, 17);
    assert_eq!(exists.strerror, "File exists");
    assert_eq!(exists.filename, file);

    let mut appended = OpenFile::open(&file, Mode::Append).unwrap();
    appended.write("b").unwrap();
    appended.close().unwrap();
    assert_eq!(
        OpenFile::open(&file, Mode::Read).unwrap().read().unwrap(),
        "ab"
    );

    OpenFile::open(&file, Mode::Write).unwrap().close().unwrap();
    assert_eq!(
        OpenFile::open(&file, Mode::Read).unwrap().read().unwrap(),
        ""
    );
}

#[test]
fn failures_carry_python_s_kind_number_and_message() {
    let directory = directory("failures");
    let missing = path(&directory, "missing.txt");
    let failure = OpenFile::open(&missing, Mode::Read).unwrap_err();
    assert_eq!(
        failure,
        OsFailure {
            kind: ErrorKind::FileNotFound,
            errno: 2,
            strerror: "No such file or directory".to_owned(),
            filename: missing,
        }
    );
    let folder = directory.to_str().unwrap();
    assert_eq!(
        OpenFile::open(folder, Mode::Read).unwrap_err().kind,
        ErrorKind::IsADirectory
    );

    let text = path(&directory, "text.txt");
    let mut out = OpenFile::open(&text, Mode::Write).unwrap();
    assert_eq!(out.read().unwrap_err().strerror, "not readable");
    out.close().unwrap();
    let mut input = OpenFile::open(&text, Mode::Read).unwrap();
    assert_eq!(input.write("x").unwrap_err().strerror, "not writable");
}

#[test]
fn text_that_is_not_utf8_names_its_byte_offset() {
    let directory = directory("utf8");
    let file = path(&directory, "bad.txt");
    fs::write(&file, b"ok\nbad \xff\n").unwrap();
    let mut input = OpenFile::open(&file, Mode::Read).unwrap();
    assert_eq!(input.read_line().unwrap(), "ok\n");
    let failure = input.read_line().unwrap_err();
    assert_eq!(failure.kind, ErrorKind::InvalidData);
    assert_eq!(failure.errno, 0);
    assert_eq!(failure.strerror, "invalid UTF-8 at byte 7");
}

/// The registry lists an open writer, and a flush through it writes the writer out. The test
/// flushes only its own file: `flush_all` would also flush the files other tests are writing on
/// their own threads at the same moment, which is a data race on their buffers.
#[test]
fn flushing_the_registry_writes_out_an_open_writer() {
    let directory = directory("flush");
    let file = path(&directory, "held.txt");
    let mut out = OpenFile::open(&file, Mode::Write).unwrap();
    out.write("held").unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), "");
    let own = out.address();
    flush_where(|address| address == own);
    assert_eq!(fs::read_to_string(&file).unwrap(), "held");
    out.close().unwrap();
}
