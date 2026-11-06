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

use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
Usage: cargo xtask <command>

Commands:
  ci                   Run every check CI runs: format, architecture, lint, test, docs
  check-architecture   Verify crate dependencies follow docs/architecture.md
  help                 Show this message";

/// The result type for every xtask command.
type Outcome = Result<(), Box<dyn Error>>;

fn main() -> ExitCode {
    let outcome = match std::env::args().nth(1).as_deref() {
        Some("ci") => ci::run(),
        Some("check-architecture") => architecture::check(),
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
