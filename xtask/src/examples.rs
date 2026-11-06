//! `cargo xtask check-examples`: every example program must be valid Python 3.12 syntax, as
//! decision 0004 proposes.
//!
//! The check runs Python's own parser through `python3`, so it agrees exactly with what editors,
//! formatters and CPython accept. It needs Python 3.12 or newer on the `PATH`.

use crate::{Outcome, collect_files, workspace_root};
use std::path::Path;
use std::process::Command;

/// Where the example programs live, relative to the repository root.
const EXAMPLES: &str = "docs/examples";

/// The file extension of a program in the language. A working choice until roadmap decision D.6.
const EXTENSION: &str = "npl";

/// The Python script that parses each file named on its command line.
const CHECK_SYNTAX: &str = include_str!("../python/check_syntax.py");

/// Parses every example program, in any subdirectory, and reports each one that is not valid
/// Python syntax.
pub(crate) fn check() -> Outcome {
    let root = workspace_root();
    let mut files = Vec::new();
    collect_files(&root.join(EXAMPLES), EXTENSION, &mut files)?;
    files.sort();
    if files.is_empty() {
        return Err(format!("no `.{EXTENSION}` programs found in {EXAMPLES}").into());
    }

    let relative: Vec<&Path> = files
        .iter()
        .map(|path| path.strip_prefix(&root).unwrap_or(path))
        .collect();
    let status = Command::new("python3")
        .arg("-I")
        .arg("-c")
        .arg(CHECK_SYNTAX)
        .args(&relative)
        .current_dir(&root)
        .status()
        .map_err(|error| {
            format!("could not run `python3`, which checks example syntax: {error}")
        })?;
    if status.success() {
        println!(
            "examples: {} programs are valid Python 3.12 syntax",
            files.len()
        );
        Ok(())
    } else {
        Err("an example program is not valid Python syntax; see the lines above".into())
    }
}
