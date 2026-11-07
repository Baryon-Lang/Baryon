//! Finding a package's files on disk, for `npl check` and `npl lsp` alike.
//!
//! A package is every `.npl` file under one root directory, at any depth. Each file's name in
//! the [`Session`](driver::Session) is its path relative to that root, which decides its module
//! name, so both commands must name files the same way.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The extension of a source file.
pub(crate) const EXTENSION: &str = "npl";

/// Every `.npl` file under `root`, at any depth, sorted so the order is the same on every
/// machine.
///
/// # Errors
///
/// Fails when a directory cannot be listed.
pub(crate) fn files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    collect(root, &mut paths)?;
    paths.sort();
    Ok(paths)
}

/// Appends every `.npl` file under `directory`, at any depth, to `paths`.
///
/// # Errors
///
/// Fails when a directory cannot be listed.
fn collect(directory: &Path, paths: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, paths)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == EXTENSION)
        {
            paths.push(path);
        }
    }
    Ok(())
}

/// The name a file gets in the package: its path relative to `root`, with `/` between
/// directories on every operating system, which is what decides its module name.
pub(crate) fn module_path(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_paths_are_relative_with_forward_slashes() {
        let root = Path::new("/work/sim");
        let path = root.join("physics").join("nbody.npl");
        assert_eq!(module_path(root, &path), "physics/nbody.npl");
    }

    #[test]
    fn files_are_found_at_any_depth_in_sorted_order() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/package");
        let names: Vec<String> = files(&root)
            .unwrap()
            .iter()
            .map(|path| module_path(&root, path))
            .collect();
        assert_eq!(names, ["main.npl", "physics/nbody.npl"]);
    }
}
