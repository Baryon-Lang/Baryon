//! `cargo xtask check-dependencies`: every external crate must pass the rules in `deny.toml`.
//!
//! The rules cover four things, each a section of cargo-deny's report:
//!
//! - **advisories**: no crate has a RustSec security advisory, is unmaintained, or is yanked;
//! - **licenses**: every license is permissive and compatible with Apache-2.0 and MIT;
//! - **bans**: no crate appears in two versions, and no version requirement is a wildcard;
//! - **sources**: every crate comes from crates.io.
//!
//! The check runs [cargo-deny](https://github.com/EmbarkStudios/cargo-deny), pinned to
//! [`CARGO_DENY_VERSION`] so a laptop and CI apply the same rules. The advisories section
//! downloads the RustSec database, so the check needs a network connection.

use crate::{Outcome, cargo, workspace_root};
use std::process::Command;

/// The cargo-deny release the rules are written for. CI installs this exact version; keep the
/// install step in `.github/workflows/ci.yml` in step with it.
const CARGO_DENY_VERSION: &str = "0.20.2";

/// Checks every dependency in `Cargo.lock` against `deny.toml`, after confirming the pinned
/// cargo-deny is installed.
pub(crate) fn check() -> Outcome {
    let installed = installed_version()?;
    if installed != CARGO_DENY_VERSION {
        return Err(format!(
            "found cargo-deny {installed}, but the rules need {CARGO_DENY_VERSION}\n{}",
            install_hint()
        )
        .into());
    }
    let status = Command::new(cargo())
        .args(["deny", "--locked", "check"])
        .current_dir(workspace_root())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err("a dependency breaks a rule in deny.toml; see the report above".into())
    }
}

/// The version of the installed cargo-deny, for example `0.20.2`.
///
/// # Errors
///
/// Fails, with instructions to install it, if cargo-deny is not installed.
fn installed_version() -> Result<String, String> {
    let output = Command::new(cargo())
        .args(["deny", "--version"])
        .output()
        .map_err(|error| format!("could not run `cargo deny --version`: {error}"))?;
    if !output.status.success() {
        return Err(format!("cargo-deny is not installed\n{}", install_hint()));
    }
    parse_version(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| format!("could not read the cargo-deny version\n{}", install_hint()))
}

/// Extracts the version from `cargo deny --version` output, which reads `cargo-deny 0.20.2`.
fn parse_version(output: &str) -> Option<String> {
    output
        .trim()
        .strip_prefix("cargo-deny ")
        .map(|version| version.trim().to_owned())
}

/// The command that installs the pinned cargo-deny.
fn install_hint() -> String {
    format!("install it with: cargo install --locked cargo-deny@{CARGO_DENY_VERSION}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_version_line() {
        assert_eq!(
            parse_version("cargo-deny 0.20.2\n").as_deref(),
            Some("0.20.2")
        );
    }

    #[test]
    fn rejects_unexpected_output() {
        assert_eq!(parse_version("error: no such command: `deny`"), None);
    }

    #[test]
    fn workflow_installs_the_pinned_version() {
        let workflow =
            std::fs::read_to_string(workspace_root().join(".github/workflows/ci.yml")).unwrap();
        assert!(workflow.contains(&format!("cargo-deny@{CARGO_DENY_VERSION}")));
    }
}
