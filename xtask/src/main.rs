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
mod links;
mod roadmap;

use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;

/// Help text printed by `cargo xtask help` and on a missing or unknown command.
const USAGE: &str = "\
Usage: cargo xtask <command>

Commands:
  ci                   Run every check CI runs: format, roadmap, architecture, links, lint, test, docs
  check-architecture   Verify crate dependencies follow docs/architecture.md
  check-links          Verify every relative link in the Markdown documentation resolves
  check-roadmap        Verify ROADMAP.md is well formed and updated by the current change
  help                 Show this message";

/// The result type for every xtask command.
type Outcome = Result<(), Box<dyn Error>>;

/// Runs the command named by the first argument and exits non-zero if it fails.
fn main() -> ExitCode {
    let outcome = match std::env::args().nth(1).as_deref() {
        Some("ci") => ci::run(),
        Some("check-architecture") => architecture::check(),
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
