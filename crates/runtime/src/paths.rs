//! Paths and directories (decision 0036, parts 1 and 7): the parts of a `pathlib.Path`, what it
//! asks the file system, and the directory functions of `os`, in safe code the exported
//! functions in `abi::paths` call.
//!
//! A `Path` holds its text in the form Python's `pathlib` keeps on Linux and macOS: components
//! separated by one `/`, no empty or `.` component, no `/` at the end, and `.` for the empty
//! path. [`normalize`] and [`join`] make that form; [`name`], [`stem`], [`suffix`] and
//! [`parent`] read it and never touch the file system. Each gives what `pathlib.PurePosixPath`
//! gives for the same text, which the tests pin against values CPython 3.13 printed.
//!
//! The functions that touch the file system use Rust's standard library, and every failure is
//! an [`OsFailure`], which compiled code receives as an `OSError`.

use crate::files::{ErrorKind, OsFailure};
use std::fs;

/// How many bytes at the start of `text` are its root: 2 for exactly two slashes, which POSIX
/// lets a system give a meaning of its own and `pathlib` therefore keeps, 1 for one slash or
/// for three and more, which mean one, and 0 for a relative path.
fn root_length(text: &str) -> usize {
    let slashes = text.bytes().take_while(|&byte| byte == b'/').count();
    match slashes {
        0 => 0,
        2 => 2,
        _ => 1,
    }
}

/// The components of `text` that `pathlib` keeps: every piece between slashes that is not empty
/// and not `.`.
fn components(text: &str) -> impl Iterator<Item = &str> {
    text.split('/')
        .filter(|component| !component.is_empty() && *component != ".")
}

/// The path with the root `root` and the components `kept`, in the form `pathlib` keeps.
fn build<'a>(root: &str, kept: impl Iterator<Item = &'a str>) -> String {
    let mut path = root.to_owned();
    for component in kept {
        if path.len() > root.len() {
            path.push('/');
        }
        path.push_str(component);
    }
    if path.is_empty() {
        path.push('.');
    }
    path
}

/// `Path(text)`: `text` in the form `pathlib` keeps, so `runs//out/` gives `runs/out`, `./a`
/// gives `a` and the empty text gives `.`. `..` is kept, since removing it would change what a
/// path through a symbolic link names.
pub(crate) fn normalize(text: &str) -> String {
    build(&text[..root_length(text)], components(text))
}

/// `path / other`: the two joined, in the form `pathlib` keeps. An `other` that starts with `/`
/// replaces `path`, as in Python. `path` is in that form already.
pub(crate) fn join(path: &str, other: &str) -> String {
    if other.starts_with('/') {
        return normalize(other);
    }
    let root = &path[..root_length(path)];
    build(root, components(path).chain(components(other)))
}

/// `path.name`: the last component, or the empty text for `.` and for a root.
pub(crate) fn name(path: &str) -> &str {
    let rest = &path[root_length(path)..];
    if rest == "." {
        return "";
    }
    rest.rsplit('/').next().unwrap_or("")
}

/// Where the suffix of the file name `name` starts: its last `.`, unless that is the first
/// character, as in `.bashrc`, or the last, as in `a.`; `None` when there is no suffix.
fn suffix_start(name: &str) -> Option<usize> {
    name.rfind('.')
        .filter(|&dot| dot > 0 && dot + 1 < name.len())
}

/// `path.suffix`: the name's last `.` and what follows, such as `.csv`, or the empty text.
pub(crate) fn suffix(path: &str) -> &str {
    let name = name(path);
    suffix_start(name).map_or("", |dot| &name[dot..])
}

/// `path.stem`: the name without its suffix.
pub(crate) fn stem(path: &str) -> &str {
    let name = name(path);
    suffix_start(name).map_or(name, |dot| &name[..dot])
}

/// `path.parent`: the path without its last component. A single name gives `.`, and `.` and a
/// root give themselves.
pub(crate) fn parent(path: &str) -> &str {
    let root = root_length(path);
    let rest = &path[root..];
    if rest.is_empty() || rest == "." {
        return path;
    }
    match rest.rfind('/') {
        Some(slash) => &path[..root + slash],
        None if root > 0 => &path[..root],
        None => ".",
    }
}

/// `path.exists()`: whether `path` names anything, following symbolic links. Any failure to
/// find out, such as a directory that may not be searched, is `false`, as in Python.
pub(crate) fn exists(path: &str) -> bool {
    fs::metadata(path).is_ok()
}

/// `path.is_file()`: whether `path` names a regular file, following symbolic links.
pub(crate) fn is_file(path: &str) -> bool {
    fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
}

/// `path.is_dir()`: whether `path` names a directory, following symbolic links.
pub(crate) fn is_directory(path: &str) -> bool {
    fs::metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

/// `os.listdir(path)`, or `path.iterdir()` when `joined`: the name of every entry of the
/// directory `path` but `.` and `..`, in the order the operating system gives, each joined to
/// `path` when `joined`. Cost: one allocation per entry.
///
/// # Errors
///
/// Fails when the directory cannot be read, and with the kind `INVALID_DATA`, naming the
/// directory, when an entry's name is not UTF-8: a program could not open that name again
/// (decision 0036, part 7).
pub(crate) fn list_directory(path: &str, joined: bool) -> Result<Vec<String>, OsFailure> {
    let failed = |error| OsFailure::from_io(&error, path);
    let mut names = Vec::new();
    for entry in fs::read_dir(path).map_err(failed)? {
        let Ok(name) = entry.map_err(failed)?.file_name().into_string() else {
            let message = "a name in the directory is not UTF-8";
            return Err(OsFailure::new(ErrorKind::InvalidData, message, path));
        };
        names.push(if joined { join(path, &name) } else { name });
    }
    Ok(names)
}

/// `os.remove(path)`: removes the file `path`.
///
/// # Errors
///
/// Fails when there is no such file, when `path` is a directory, or when the operating system
/// refuses.
pub(crate) fn remove(path: &str) -> Result<(), OsFailure> {
    fs::remove_file(path).map_err(|error| OsFailure::from_io(&error, path))
}

/// `os.rename(source, destination)`: moves `source` to `destination`, replacing a file there.
///
/// # Errors
///
/// Fails, naming `source`, when it does not exist, when the two are on different file systems,
/// or when the operating system refuses.
pub(crate) fn rename(source: &str, destination: &str) -> Result<(), OsFailure> {
    fs::rename(source, destination).map_err(|error| OsFailure::from_io(&error, source))
}

/// `os.mkdir(path)`: makes the directory `path`.
///
/// # Errors
///
/// Fails with the kind `FILE_EXISTS` when `path` exists, with `FILE_NOT_FOUND` when its parent
/// does not, or when the operating system refuses.
pub(crate) fn make_directory(path: &str) -> Result<(), OsFailure> {
    fs::create_dir(path).map_err(|error| OsFailure::from_io(&error, path))
}

/// `os.makedirs(path, exist_ok=exist_ok)`: makes the directory `path` and every missing
/// directory on the way to it, as Python's does: the parents first, then `path` itself.
///
/// # Errors
///
/// Fails with the kind `FILE_EXISTS` when `path` exists, unless `exist_ok` and it is a
/// directory, and when the operating system refuses to make any directory on the way.
pub(crate) fn make_directories(path: &str, exist_ok: bool) -> Result<(), OsFailure> {
    let normal = normalize(path);
    let above = parent(&normal);
    if above != normal && !exists(above) {
        match make_directories(above, exist_ok) {
            // Another process made it first, which is what was wanted.
            Err(failure) if failure.kind == ErrorKind::FileExists => {}
            other => other?,
        }
    }
    match make_directory(path) {
        Err(_) if exist_ok && is_directory(path) => Ok(()),
        other => other,
    }
}

/// `os.getcwd()`: the directory the program runs in.
///
/// # Errors
///
/// Fails when the directory is gone or may not be read, and with the kind `INVALID_DATA` when
/// its path is not UTF-8.
pub(crate) fn current_directory() -> Result<String, OsFailure> {
    let directory = std::env::current_dir().map_err(|error| OsFailure::from_io(&error, ""))?;
    directory.into_os_string().into_string().map_err(|_| {
        let message = "the current directory's path is not UTF-8";
        OsFailure::new(ErrorKind::InvalidData, message, "")
    })
}

#[cfg(test)]
mod tests;
