//! `cargo xtask check-architecture`: enforces the crate dependency rules in
//! `docs/architecture.md`.
//!
//! Every workspace crate declares a kind in its manifest:
//!
//! ```toml
//! [package.metadata.architecture]
//! kind = "implementation"
//! ```
//!
//! The kind decides which other workspace crates it may depend on. The rule that matters
//! most: an implementation never depends on another implementation, so any one of them can
//! be replaced without touching the rest.

use crate::{Outcome, cargo, workspace_root};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use std::process::Command;

/// The role a crate plays, which bounds what it may depend on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Shared building blocks with no compiler logic: source text, spans, diagnostics.
    Foundation,
    /// Data types and traits that stages exchange: syntax trees, types, IR.
    Interface,
    /// One replaceable piece of the compiler: a parser, a type checker, a backend.
    Implementation,
    /// An executable that wires implementations together: the compiler driver, the language server.
    Application,
    /// Repository automation that no compiler crate may depend on.
    Tooling,
    /// Code linked into every compiled program rather than into the compiler (decision 0021). It
    /// depends on no other kind, so nothing from the compiler ends up in a user's program.
    Runtime,
}

impl Kind {
    /// Every kind, in the order the documentation lists them.
    const ALL: [Self; 6] = [
        Self::Foundation,
        Self::Interface,
        Self::Implementation,
        Self::Application,
        Self::Tooling,
        Self::Runtime,
    ];

    /// The kind as written in a crate manifest.
    fn name(self) -> &'static str {
        match self {
            Self::Foundation => "foundation",
            Self::Interface => "interface",
            Self::Implementation => "implementation",
            Self::Application => "application",
            Self::Tooling => "tooling",
            Self::Runtime => "runtime",
        }
    }

    /// The kind a manifest names, or `None` if the name is not a kind.
    fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// Whether a crate of this kind may depend on a crate of the `dependency` kind.
    fn may_depend_on(self, dependency: Self) -> bool {
        match self {
            Self::Foundation => dependency == Self::Foundation,
            Self::Interface | Self::Implementation => {
                matches!(dependency, Self::Foundation | Self::Interface)
            }
            Self::Application => matches!(
                dependency,
                Self::Foundation | Self::Interface | Self::Implementation | Self::Runtime
            ),
            Self::Tooling => dependency != Self::Tooling,
            Self::Runtime => dependency == Self::Runtime,
        }
    }
}

/// A workspace crate as the checker sees it.
struct Crate {
    /// The package name.
    name: String,
    /// The declared kind, as written, if the manifest declares one.
    kind: Option<String>,
    /// Names of the workspace crates it depends on, in any dependency section.
    dependencies: Vec<String>,
}

/// One broken rule.
#[derive(Debug, PartialEq, Eq)]
enum Violation {
    /// A crate declares no kind.
    MissingKind {
        /// The crate.
        name: String,
    },
    /// A crate declares a kind that does not exist.
    UnknownKind {
        /// The crate.
        name: String,
        /// The kind it declares.
        kind: String,
    },
    /// A crate depends on a crate its kind may not depend on.
    ForbiddenDependency {
        /// The crate.
        name: String,
        /// Its kind.
        kind: Kind,
        /// The crate it depends on.
        dependency: String,
        /// The dependency's kind.
        dependency_kind: Kind,
    },
}

impl fmt::Display for Violation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingKind { name } => write!(
                formatter,
                "crate `{name}` has no `kind` under `[package.metadata.architecture]`"
            ),
            Self::UnknownKind { name, kind } => {
                let known: Vec<&str> = Kind::ALL.iter().map(|kind| kind.name()).collect();
                write!(
                    formatter,
                    "crate `{name}` has unknown kind `{kind}`; expected one of: {}",
                    known.join(", ")
                )
            }
            Self::ForbiddenDependency {
                name,
                kind,
                dependency,
                dependency_kind,
            } => write!(
                formatter,
                "{} crate `{name}` depends on {} crate `{dependency}`, which its kind forbids",
                kind.name(),
                dependency_kind.name()
            ),
        }
    }
}

/// Loads the workspace, checks every crate, and reports each violation.
pub(crate) fn check() -> Outcome {
    let crates = load_workspace()?;
    let violations = violations(&crates);
    if violations.is_empty() {
        println!(
            "architecture: {} crates follow the dependency rules",
            crates.len()
        );
        return Ok(());
    }
    for violation in &violations {
        eprintln!("architecture: {violation}");
    }
    Err(format!(
        "{} architecture violation(s); see docs/architecture.md",
        violations.len()
    )
    .into())
}

/// Every rule the given crates break, in manifest order.
fn violations(crates: &[Crate]) -> Vec<Violation> {
    let mut kinds = HashMap::new();
    let mut found = Vec::new();
    for member in crates {
        match member.kind.as_deref().map(|kind| (kind, Kind::parse(kind))) {
            None => found.push(Violation::MissingKind {
                name: member.name.clone(),
            }),
            Some((kind, None)) => found.push(Violation::UnknownKind {
                name: member.name.clone(),
                kind: kind.to_owned(),
            }),
            Some((_, Some(kind))) => {
                kinds.insert(member.name.as_str(), kind);
            }
        }
    }
    for member in crates {
        let Some(&kind) = kinds.get(member.name.as_str()) else {
            continue;
        };
        for dependency in &member.dependencies {
            let Some(&dependency_kind) = kinds.get(dependency.as_str()) else {
                continue;
            };
            if !kind.may_depend_on(dependency_kind) {
                found.push(Violation::ForbiddenDependency {
                    name: member.name.clone(),
                    kind,
                    dependency: dependency.clone(),
                    dependency_kind,
                });
            }
        }
    }
    found
}

/// Reads every workspace crate from `cargo metadata`.
fn load_workspace() -> Result<Vec<Crate>, Box<dyn std::error::Error>> {
    let output = Command::new(cargo())
        .args(["metadata", "--format-version", "1", "--no-deps", "--locked"])
        .current_dir(workspace_root())
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "`cargo metadata` failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let metadata: Value = serde_json::from_slice(&output.stdout)?;
    let packages = metadata["packages"]
        .as_array()
        .ok_or("`cargo metadata` output has no `packages` list")?;
    Ok(packages.iter().map(crate_from_package).collect())
}

/// Converts one package from `cargo metadata` output into a [`Crate`].
fn crate_from_package(package: &Value) -> Crate {
    let text = |value: &Value| value.as_str().map(str::to_owned);
    let workspace_dependencies = package["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|dependency| dependency.get("path").is_some())
        .filter_map(|dependency| text(&dependency["name"]));
    Crate {
        name: text(&package["name"]).unwrap_or_default(),
        kind: text(&package["metadata"]["architecture"]["kind"]),
        dependencies: workspace_dependencies.collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(name: &str, kind: Option<&str>, dependencies: &[&str]) -> Crate {
        Crate {
            name: name.to_owned(),
            kind: kind.map(str::to_owned),
            dependencies: dependencies.iter().map(|&name| name.to_owned()).collect(),
        }
    }

    #[test]
    fn allows_the_documented_layering() {
        let crates = [
            member("source", Some("foundation"), &[]),
            member("syntax", Some("interface"), &["source"]),
            member("parser", Some("implementation"), &["syntax", "source"]),
            member("driver", Some("application"), &["parser", "syntax"]),
            member("xtask", Some("tooling"), &["driver"]),
        ];
        assert_eq!(violations(&crates), []);
    }

    #[test]
    fn forbids_one_implementation_depending_on_another() {
        let crates = [
            member("parser", Some("implementation"), &[]),
            member("checker", Some("implementation"), &["parser"]),
        ];
        assert_eq!(
            violations(&crates),
            [Violation::ForbiddenDependency {
                name: "checker".to_owned(),
                kind: Kind::Implementation,
                dependency: "parser".to_owned(),
                dependency_kind: Kind::Implementation,
            }]
        );
    }

    #[test]
    fn forbids_foundation_depending_upward() {
        let crates = [
            member("source", Some("foundation"), &["syntax"]),
            member("syntax", Some("interface"), &[]),
        ];
        assert_eq!(violations(&crates).len(), 1);
    }

    #[test]
    fn forbids_compiler_crates_depending_on_tooling() {
        let crates = [
            member("driver", Some("application"), &["xtask"]),
            member("xtask", Some("tooling"), &[]),
        ];
        assert_eq!(violations(&crates).len(), 1);
    }

    #[test]
    fn keeps_the_runtime_apart_from_the_compiler() {
        let crates = [
            member("source", Some("foundation"), &[]),
            member("runtime", Some("runtime"), &["source"]),
            member("backend", Some("implementation"), &["runtime"]),
            member("driver", Some("application"), &["runtime"]),
        ];
        assert_eq!(
            violations(&crates),
            [
                Violation::ForbiddenDependency {
                    name: "runtime".to_owned(),
                    kind: Kind::Runtime,
                    dependency: "source".to_owned(),
                    dependency_kind: Kind::Foundation,
                },
                Violation::ForbiddenDependency {
                    name: "backend".to_owned(),
                    kind: Kind::Implementation,
                    dependency: "runtime".to_owned(),
                    dependency_kind: Kind::Runtime,
                },
            ]
        );
    }

    #[test]
    fn reports_missing_and_unknown_kinds() {
        let crates = [member("bare", None, &[]), member("odd", Some("misc"), &[])];
        assert_eq!(
            violations(&crates),
            [
                Violation::MissingKind {
                    name: "bare".to_owned()
                },
                Violation::UnknownKind {
                    name: "odd".to_owned(),
                    kind: "misc".to_owned()
                },
            ]
        );
    }
}
