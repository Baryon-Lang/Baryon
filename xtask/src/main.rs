//! Repository automation. Run `cargo xtask help` for the commands.
//!
//! Every check CI runs lives here, so `cargo xtask ci` on a laptop and the CI workflow run
//! exactly the same steps.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "xtask is a command-line tool that reports to the terminal"
)]

mod architecture;
mod ci;
mod dependencies;
mod examples;
mod links;
mod roadmap;

use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Help text printed by `cargo xtask help` and on a missing or unknown command.
const USAGE: &str = "\
Usage: cargo xtask <command>

Commands:
  ci                   Run every check CI runs: format, roadmap, architecture, links, examples,
                       dependencies, lint, test, docs
  check-architecture   Verify crate dependencies follow docs/architecture.md
  check-dependencies   Verify external crates pass the license, advisory, ban and source rules
                       in deny.toml (needs cargo-deny)
  check-examples       Verify every example program is valid Python 3.12 syntax
  check-links          Verify every relative link in the Markdown documentation resolves
  check-roadmap        Verify ROADMAP.md is well formed and updated by the current change
  help                 Show this message";

/// Directories never searched for files: version-control data and build output.
const SKIPPED_DIRECTORIES: [&str; 2] = [".git", "target"];

/// The result type for every xtask command.
type Outcome = Result<(), Box<dyn Error>>;

/// Runs the command named by the first argument and exits non-zero if it fails.
fn main() -> ExitCode {
    let outcome = match std::env::args().nth(1).as_deref() {
        Some("ci") => ci::run(),
        Some("check-architecture") => architecture::check(),
        Some("check-dependencies") => dependencies::check(),
        Some("check-examples") => examples::check(),
        Some("check-links") => links::check(),
        Some("check-roadmap") => roadmap::check(),
        Some("help" | "--help" | "-h") => {
            println!("{USAGE}");
            Ok(())
        }
        Some(unknown) => Err(format!("unknown command `{unknown}`\n\n{USAGE}").into()),
        None => Err(USAGE.into()),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The repository root: the parent of this crate's directory.
fn workspace_root() -> PathBuf {
    let manifest_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_directory
        .parent()
        .map_or(manifest_directory.clone(), PathBuf::from)
}

/// The `cargo` that invoked xtask, so every step uses the pinned toolchain.
fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| String::from("cargo"))
}

/// Adds every file under `directory` whose extension is `extension` to `files`, at any depth,
/// skipping [`SKIPPED_DIRECTORIES`]. Files are added in directory order; callers sort.
fn collect_files(directory: &Path, extension: &str, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            let skipped = SKIPPED_DIRECTORIES
                .iter()
                .any(|name| entry.file_name() == *name);
            if !skipped {
                collect_files(&path, extension, files)?;
            }
        } else if path.extension().is_some_and(|found| found == extension) {
            files.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_files_in_subdirectories_by_extension() {
        let root = workspace_root();
        let mut files = Vec::new();
        collect_files(&root.join("docs"), "byn", &mut files).unwrap();
        assert!(files.contains(&root.join("docs/examples/heat_equation.byn")));
        assert!(files.iter().all(|file| file.extension().unwrap() == "byn"));
    }
}
