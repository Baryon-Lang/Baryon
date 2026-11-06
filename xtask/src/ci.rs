//! `cargo xtask ci`: every check, cheapest first, stopping at the first failure.

use crate::{Outcome, architecture, cargo, workspace_root};
use std::process::Command;

/// One check in the CI sequence.
enum Check {
    /// A `cargo` invocation that must succeed.
    Cargo {
        name: &'static str,
        arguments: &'static [&'static str],
        environment: &'static [(&'static str, &'static str)],
    },
    /// The crate dependency rules in `docs/architecture.md`.
    Architecture,
}

const CHECKS: &[Check] = &[
    Check::Cargo {
        name: "format",
        arguments: &["fmt", "--all", "--check"],
        environment: &[],
    },
    Check::Architecture,
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
        arguments: &["doc", "--workspace", "--no-deps", "--locked"],
        environment: &[("RUSTDOCFLAGS", "--deny warnings")],
    },
];

pub(crate) fn run() -> Outcome {
    for check in CHECKS {
        println!("==> {}", check.name());
        check.run()?;
    }
    println!("==> all checks passed");
    Ok(())
}

impl Check {
    fn name(&self) -> &'static str {
        match self {
            Self::Cargo { name, .. } => name,
            Self::Architecture => "architecture",
        }
    }

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
        }
    }
}
