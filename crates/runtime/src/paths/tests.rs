//! Tests of paths and directories: the text functions against what CPython 3.13's
//! `pathlib.PurePosixPath` gives, and the file-system functions in a scratch directory.

use super::*;

/// A text, then what `Path(text)` gives in CPython 3.13: `str(path)`, `path.name`,
/// `path.stem`, `path.suffix` and `str(path.parent)`.
const PARTS: [(&str, &str, &str, &str, &str, &str); 29] = [
    ("", ".", "", "", "", "."),
    (".", ".", "", "", "", "."),
    ("..", "..", "..", "..", "", "."),
    ("/", "/", "", "", "", "/"),
    ("//", "//", "", "", "", "//"),
    ("///", "/", "", "", "", "/"),
    ("a", "a", "a", "a", "", "."),
    ("a/", "a", "a", "a", "", "."),
    ("./a", "a", "a", "a", "", "."),
    ("a/.", "a", "a", "a", "", "."),
    ("a//b/", "a/b", "b", "b", "", "a"),
    ("a/./b", "a/b", "b", "b", "", "a"),
    ("a/..", "a/..", "..", "..", "", "a"),
    ("/a", "/a", "a", "a", "", "/"),
    ("//a", "//a", "a", "a", "", "//"),
    ("///a", "/a", "a", "a", "", "/"),
    ("/a/b.c", "/a/b.c", "b.c", "b", ".c", "/a"),
    (
        "runs/out.csv",
        "runs/out.csv",
        "out.csv",
        "out",
        ".csv",
        "runs",
    ),
    ("a.tar.gz", "a.tar.gz", "a.tar.gz", "a.tar", ".gz", "."),
    (".bashrc", ".bashrc", ".bashrc", ".bashrc", "", "."),
    ("a.", "a.", "a.", "a.", "", "."),
    (".a.", ".a.", ".a.", ".a.", "", "."),
    ("...", "...", "...", "...", "", "."),
    ("a..b", "a..b", "a..b", "a.", ".b", "."),
    ("a. ", "a. ", "a. ", "a", ". ", "."),
    ("dir.d/file", "dir.d/file", "file", "file", "", "dir.d"),
    ("é/ü.txt", "é/ü.txt", "ü.txt", "ü", ".txt", "é"),
    ("/.", "/", "", "", "", "/"),
    ("../x", "../x", "x", "x", "", ".."),
];

/// A path, a text, and what `str(Path(path) / text)` gives in CPython 3.13.
const JOINS: [(&str, &str, &str); 14] = [
    ("runs", "out.csv", "runs/out.csv"),
    (".", "a", "a"),
    ("/", "a", "/a"),
    ("//", "a", "//a"),
    ("a", "/b", "/b"),
    ("a", "", "a"),
    ("a", ".", "a"),
    (".", ".", "."),
    ("a", "b//c/", "a/b/c"),
    ("a", "../b", "a/../b"),
    ("/a", "b", "/a/b"),
    ("a", "//b", "//b"),
    ("a", "///b", "/b"),
    (".", "", "."),
];

/// A new empty directory for one test, named after it, and its path as text.
fn scratch(test: &str) -> String {
    let directory =
        std::env::temp_dir().join(format!("baryon-paths-{test}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    directory.to_str().unwrap().to_owned()
}

#[test]
fn the_parts_of_a_path_match_cpython() {
    for (text, path, expected_name, expected_stem, expected_suffix, expected_parent) in PARTS {
        let normal = normalize(text);
        assert_eq!(normal, path, "Path({text:?})");
        assert_eq!(normalize(&normal), path, "Path({path:?}) is already normal");
        assert_eq!(name(&normal), expected_name, "Path({text:?}).name");
        assert_eq!(stem(&normal), expected_stem, "Path({text:?}).stem");
        assert_eq!(suffix(&normal), expected_suffix, "Path({text:?}).suffix");
        assert_eq!(parent(&normal), expected_parent, "Path({text:?}).parent");
    }
}

#[test]
fn joining_matches_cpython() {
    for (path, other, expected) in JOINS {
        assert_eq!(join(path, other), expected, "Path({path:?}) / {other:?}");
    }
}

#[test]
fn asks_the_file_system_what_a_path_names() {
    let directory = scratch("asks");
    let file = join(&directory, "data.txt");
    fs::write(&file, "1\n").unwrap();
    let missing = join(&directory, "missing");

    assert!(exists(&directory) && is_directory(&directory) && !is_file(&directory));
    assert!(exists(&file) && is_file(&file) && !is_directory(&file));
    assert!(!exists(&missing) && !is_file(&missing) && !is_directory(&missing));
    // A file used as a directory is an error to the operating system and `False` here.
    assert!(!exists(&join(&file, "below")));
    fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn lists_a_directory_by_name_or_joined() {
    let directory = scratch("lists");
    for name in ["b.txt", "a.txt"] {
        fs::write(join(&directory, name), "").unwrap();
    }
    fs::create_dir(join(&directory, "sub")).unwrap();

    let mut names = list_directory(&directory, false).unwrap();
    names.sort();
    assert_eq!(names, ["a.txt", "b.txt", "sub"]);
    let mut paths = list_directory(&directory, true).unwrap();
    paths.sort();
    let expected = ["a.txt", "b.txt", "sub"].map(|name| format!("{directory}/{name}"));
    assert_eq!(paths, expected);

    let file = join(&directory, "a.txt");
    let failure = list_directory(&file, false).unwrap_err();
    assert_eq!(
        (failure.kind, failure.errno, failure.filename.as_str()),
        (ErrorKind::NotADirectory, 20, file.as_str())
    );
    let missing = join(&directory, "missing");
    let failure = list_directory(&missing, true).unwrap_err();
    assert_eq!(
        (failure.kind, failure.strerror.as_str()),
        (ErrorKind::FileNotFound, "No such file or directory")
    );
    fs::remove_dir_all(&directory).unwrap();
}

#[cfg(unix)]
#[test]
fn a_name_that_is_not_utf8_fails_the_listing() {
    use std::os::unix::ffi::OsStrExt;

    let directory = scratch("bytes");
    let bad = std::path::Path::new(&directory).join(std::ffi::OsStr::from_bytes(b"caf\xe9"));
    // A file system that stores only UTF-8 names, as macOS's does, refuses the name; then
    // there is nothing to list wrongly.
    if fs::write(bad, "").is_ok() {
        let failure = list_directory(&directory, false).unwrap_err();
        assert_eq!(
            failure,
            OsFailure::new(
                ErrorKind::InvalidData,
                "a name in the directory is not UTF-8",
                &directory
            )
        );
    }
    fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn removes_and_renames_files() {
    let directory = scratch("removes");
    let (old, new) = (join(&directory, "old.txt"), join(&directory, "new.txt"));
    fs::write(&old, "kept").unwrap();
    fs::write(&new, "replaced").unwrap();

    rename(&old, &new).unwrap();
    assert!(!exists(&old));
    assert_eq!(fs::read_to_string(&new).unwrap(), "kept");
    let failure = rename(&old, &new).unwrap_err();
    assert_eq!(
        (failure.kind, failure.errno, failure.filename.as_str()),
        (ErrorKind::FileNotFound, 2, old.as_str())
    );

    remove(&new).unwrap();
    assert!(!exists(&new));
    let failure = remove(&new).unwrap_err();
    assert_eq!(
        (failure.kind, failure.errno, failure.filename.as_str()),
        (ErrorKind::FileNotFound, 2, new.as_str())
    );
    // Removing a directory fails: `IS_A_DIRECTORY` on Linux and `PERMISSION` on macOS, as
    // Python's `os.remove` does on each.
    assert!(remove(&directory).is_err());
    fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn makes_one_directory_or_every_missing_one() {
    let directory = scratch("makes");
    let one = join(&directory, "one");
    make_directory(&one).unwrap();
    assert!(is_directory(&one));
    let failure = make_directory(&one).unwrap_err();
    assert_eq!(
        (failure.kind, failure.errno, failure.filename.as_str()),
        (ErrorKind::FileExists, 17, one.as_str())
    );
    let deep = join(&directory, "a/b/c");
    let failure = make_directory(&deep).unwrap_err();
    assert_eq!((failure.kind, failure.errno), (ErrorKind::FileNotFound, 2));

    make_directories(&deep, false).unwrap();
    assert!(is_directory(&deep));
    let failure = make_directories(&deep, false).unwrap_err();
    assert_eq!(
        (failure.kind, failure.filename.as_str()),
        (ErrorKind::FileExists, deep.as_str())
    );
    make_directories(&deep, true).unwrap();
    // A path written with a `/` at its end names the same directory.
    make_directories(&format!("{directory}/x/y/"), false).unwrap();
    assert!(is_directory(&join(&directory, "x/y")));

    // `exist_ok` forgives a directory, never a file.
    let file = join(&directory, "file");
    fs::write(&file, "").unwrap();
    let failure = make_directories(&file, true).unwrap_err();
    assert_eq!(failure.kind, ErrorKind::FileExists);
    // A file on the way is not a directory to make one in.
    let failure = make_directories(&join(&file, "below/deeper"), true).unwrap_err();
    assert_eq!(failure.kind, ErrorKind::NotADirectory);
    fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn gives_the_current_directory() {
    let expected = std::env::current_dir().unwrap();
    assert_eq!(current_directory().unwrap(), expected.to_str().unwrap());
}
