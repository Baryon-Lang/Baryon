//! `cargo xtask ci`: every check, cheapest first, stopping at the first failure.

use crate::{Outcome, architecture, cargo, links, workspace_root};
use std::process::Command;

/// One check in the CI sequence.
enum Check {
    /// A `cargo` invocation that must succeed.
    Cargo {
        /// Short name printed before the check runs.
        name: &'static str,
        /// Arguments passed to `cargo`.
        arguments: &'static [&'static str],
        /// Extra environment variables for the invocation.
        environment: &'static [(&'static str, &'static str)],
    },
    /// The crate dependency rules in `docs/architecture.md`.
    Architecture,
    /// Every relative link in the Markdown documentation resolves.
    Links,
}

/// The checks `cargo xtask ci` runs, in order. Fast checks come first so failures surface early.
const CHECKS: &[Check] = &[
    Check::Cargo {
        name: "format",
        arguments: &["fmt", "--all", "--check"],
        environment: &[],
    },
    Check::Architecture,
    Check::Links,
    Check::Cargo {
        name: "lint",
        arguments: &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--",
            "--deny",
            "warnings",
        ],
        environment: &[],
    },
    Check::Cargo {
        name: "test",
        arguments: &["test", "--workspace", "--locked"],
        environment: &[],
    },
    Check::Cargo {
        name: "docs",
        arguments: &[
            "doc",
            "--workspace",
            "--no-deps",
            "--document-private-items",
            "--locked",
        ],
        environment: &[("RUSTDOCFLAGS", "--deny warnings")],
    },
];

/// Runs every check in [`CHECKS`] and stops at the first failure.
pub(crate) fn run() -> Outcome {
    for check in CHECKS {
        println!("==> {}", check.name());
        check.run()?;
    }
    println!("==> all checks passed");
    Ok(())
}

impl Check {
    /// The name printed before the check runs.
    fn name(&self) -> &'static str {
        match self {
            Self::Cargo { name, .. } => name,
            Self::Architecture => "architecture",
            Self::Links => "links",
        }
    }

    /// Runs the check from the workspace root.
    fn run(&self) -> Outcome {
        match self {
            Self::Cargo {
                name,
                arguments,
                environment,
            } => {
                let status = Command::new(cargo())
                    .args(*arguments)
                    .envs(environment.iter().copied())
                    .current_dir(workspace_root())
                    .status()?;
                if status.success() {
                    Ok(())
                } else {
                    Err(format!("check `{name}` failed ({status})").into())
                }
            }
            Self::Architecture => architecture::check(),
            Self::Links => links::check(),
        }
    }
}
