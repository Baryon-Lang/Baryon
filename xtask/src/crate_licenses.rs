//! `cargo xtask crate-licenses <output>`: writes the licenses of every external crate linked
//! into `baryon` and `libruntime.a`, for the release archives.
//!
//! `baryon` is one statically linked binary, so the code of every external crate it uses ships
//! inside it. Most of those crates are MIT or Apache-2.0, and both licenses ask that their text
//! and copyright notices go with every copy of the software, binaries included. The release
//! workflow runs this command after the release build and puts the file it writes,
//! `CRATE-LICENSES.md`, in each archive beside `LICENSE`, `LICENSE-MIT` and
//! `THIRD-PARTY-NOTICES.md` (Collins, 2026-11-14, before release 0.2.0).
//!
//! Which crates: the packages `cargo metadata` resolves for the build machine's platform,
//! reached from the `driver` and `runtime` crates through normal dependencies only. Build and
//! development dependencies never reach the binary, and procedural macros run only while
//! compiling, so all three are left out. Workspace crates are ours and covered by `LICENSE`.
//!
//! Which text: every file in the crate's published source whose name starts with `LICENSE`,
//! `LICENCE`, `COPYING`, `NOTICE`, `COPYRIGHT` or `UNLICENSE`, verbatim. A crate published
//! without such a file but declared MIT gets the MIT License with its `authors` as the copyright
//! holders, which is what its declaration grants; Ruff's parser crates are published that way.
//! Any other crate without a license file stops the command, so a release never ships a crate
//! whose terms it does not carry.
//!
//! Written here, over `cargo metadata` and the `serde_json` xtask already uses, instead of
//! adding cargo-about: the job is a graph walk and a directory listing, and a second license
//! tool would need its own pin, install step and configuration beside cargo-deny's.

use crate::{Outcome, cargo, workspace_root};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The workspace crates whose binaries ship: `driver` builds `baryon`, and `runtime` builds
/// `libruntime.a`, which every Baryon program links.
const SHIPPED_CRATES: [&str; 2] = ["driver", "runtime"];

/// The start of the name of a file that holds a license or notice, compared in upper case.
const LICENSE_FILE_PREFIXES: [&str; 6] = [
    "LICENSE",
    "LICENCE",
    "COPYING",
    "NOTICE",
    "COPYRIGHT",
    "UNLICENSE",
];

/// The MIT License's text after its copyright line, from our own `LICENSE-MIT`, used for a crate
/// declared MIT that ships no license file.
const MIT_BODY_START: &str = "Permission is hereby granted";

/// One external crate linked into the shipped binaries.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Crate {
    /// The crate's name, such as `salsa`.
    name: String,
    /// Its version, such as `0.28.5`.
    version: String,
    /// Its SPDX license expression from `Cargo.toml`, such as `MIT OR Apache-2.0`.
    license: String,
    /// The people `Cargo.toml` names as its authors, for the MIT fallback's copyright line.
    authors: Vec<String>,
    /// The directory holding its published source.
    directory: PathBuf,
}

/// One license text and the crates it came from.
#[derive(Debug, PartialEq, Eq)]
struct Text {
    /// The file it was read from, such as `LICENSE-MIT`, or `MIT (declared)` for the fallback.
    file: String,
    /// The text itself.
    body: String,
}

/// Writes the licenses of every linked external crate to the file named by `arguments`.
///
/// # Errors
///
/// Fails if no output path is given, if `cargo metadata` or `rustc -vV` fails, or if a crate
/// has no license file and is not declared MIT.
pub(crate) fn write(arguments: &[String]) -> Outcome {
    let [output] = arguments else {
        return Err("usage: cargo xtask crate-licenses <output>".into());
    };
    let metadata = metadata()?;
    let crates = linked_crates(&metadata)?;
    let mit_body = mit_body(&fs::read_to_string(workspace_root().join("LICENSE-MIT"))?)?;
    let mut sections = Vec::new();
    for found in &crates {
        sections.push((found.clone(), texts(found, &mit_body)?));
    }
    fs::write(output, render(&sections))?;
    println!(
        "crate-licenses: wrote the licenses of {} crates to {output}",
        crates.len()
    );
    Ok(())
}

/// Runs `cargo metadata` for the build machine's platform and parses its output.
fn metadata() -> Result<Value, Box<dyn std::error::Error>> {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| String::from("rustc"));
    let version = Command::new(rustc).arg("-vV").output()?;
    let host = String::from_utf8_lossy(&version.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("host: ").map(str::to_owned))
        .ok_or("`rustc -vV` did not name the host platform")?;
    let output = Command::new(cargo())
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--filter-platform",
        ])
        .arg(&host)
        .current_dir(workspace_root())
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "`cargo metadata` failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

/// The external crates reached from [`SHIPPED_CRATES`] through normal dependencies, sorted by
/// name and version, without procedural macros.
///
/// # Errors
///
/// Fails if the metadata lacks a field this walk reads, or names no shipped crate.
fn linked_crates(metadata: &Value) -> Result<Vec<Crate>, String> {
    let missing = |field: &str| format!("`cargo metadata` output has no `{field}`");
    let packages: BTreeMap<&str, &Value> = metadata["packages"]
        .as_array()
        .ok_or_else(|| missing("packages"))?
        .iter()
        .filter_map(|package| Some((package["id"].as_str()?, package)))
        .collect();
    let workspace: BTreeSet<&str> = metadata["workspace_members"]
        .as_array()
        .ok_or_else(|| missing("workspace_members"))?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let nodes: BTreeMap<&str, &Value> = metadata["resolve"]["nodes"]
        .as_array()
        .ok_or_else(|| missing("resolve.nodes"))?
        .iter()
        .filter_map(|node| Some((node["id"].as_str()?, node)))
        .collect();
    let mut pending: Vec<&str> = workspace
        .iter()
        .copied()
        .filter(|id| {
            packages
                .get(id)
                .and_then(|package| package["name"].as_str())
                .is_some_and(|name| SHIPPED_CRATES.contains(&name))
        })
        .collect();
    if pending.len() != SHIPPED_CRATES.len() {
        return Err(format!(
            "the workspace does not hold all of {SHIPPED_CRATES:?}"
        ));
    }
    let mut reached = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !reached.insert(id) {
            continue;
        }
        let dependencies = nodes.get(id).and_then(|node| node["deps"].as_array());
        for dependency in dependencies.into_iter().flatten() {
            let normal = dependency["dep_kinds"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind["kind"].is_null()));
            if let (true, Some(target)) = (normal, dependency["pkg"].as_str()) {
                pending.push(target);
            }
        }
    }
    let mut crates: Vec<Crate> = reached
        .into_iter()
        .filter(|id| !workspace.contains(id))
        .filter_map(|id| packages.get(id))
        .filter(|package| !is_procedural_macro(package))
        .map(|package| crate_of(package))
        .collect::<Result<_, _>>()?;
    crates.sort_by(|left, right| (&left.name, &left.version).cmp(&(&right.name, &right.version)));
    Ok(crates)
}

/// Whether every target of `package` is a procedural macro, which runs only while compiling.
fn is_procedural_macro(package: &Value) -> bool {
    package["targets"].as_array().is_some_and(|targets| {
        targets.iter().all(|target| {
            target["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == "proc-macro"))
        })
    })
}

/// The [`Crate`] that a `cargo metadata` package describes.
///
/// # Errors
///
/// Fails if the package has no name, version or manifest path.
fn crate_of(package: &Value) -> Result<Crate, String> {
    let field = |name: &str| {
        package[name]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("a package in `cargo metadata` output has no `{name}`"))
    };
    let manifest = PathBuf::from(field("manifest_path")?);
    Ok(Crate {
        name: field("name")?,
        version: field("version")?,
        license: package["license"].as_str().unwrap_or("").to_owned(),
        authors: package["authors"]
            .as_array()
            .map(|authors| {
                authors
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        directory: manifest.parent().map(Path::to_path_buf).unwrap_or_default(),
    })
}

/// The license texts of `found`: its license files, or the MIT fallback.
///
/// # Errors
///
/// Fails if its directory cannot be read, or if it has no license file and is not declared MIT.
fn texts(found: &Crate, mit_body: &str) -> Result<Vec<Text>, String> {
    let entries = fs::read_dir(&found.directory)
        .map_err(|error| format!("cannot read {}: {error}", found.directory.display()))?;
    let mut texts = Vec::new();
    for entry in entries.flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        let upper = file.to_uppercase();
        let is_license = LICENSE_FILE_PREFIXES
            .iter()
            .any(|prefix| upper.starts_with(prefix));
        if is_license && entry.path().is_file() {
            let body = fs::read_to_string(entry.path())
                .map_err(|error| format!("cannot read {file} of {}: {error}", found.name))?;
            texts.push(Text { file, body });
        }
    }
    texts.sort_by(|left, right| left.file.cmp(&right.file));
    if texts.is_empty() {
        texts.push(declared_mit(found, mit_body)?);
    }
    Ok(texts)
}

/// The MIT License for a crate declared MIT that ships no license file, with its authors as the
/// copyright holders.
///
/// # Errors
///
/// Fails, naming the crate, if its license expression does not offer MIT.
fn declared_mit(found: &Crate, mit_body: &str) -> Result<Text, String> {
    let offers_mit = found
        .license
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '-')
        .any(|term| term == "MIT");
    if !offers_mit {
        return Err(format!(
            "{} {} ships no license file and is not declared MIT (it is `{}`); add its license \
             before releasing",
            found.name, found.version, found.license
        ));
    }
    let holders = if found.authors.is_empty() {
        format!("the {} authors", found.name)
    } else {
        found.authors.join(", ")
    };
    Ok(Text {
        file: String::from("MIT (declared in Cargo.toml; the crate ships no license file)"),
        body: format!("MIT License\n\nCopyright (c) {holders}\n\n{mit_body}"),
    })
}

/// The MIT License's permission and warranty paragraphs, cut from our own `LICENSE-MIT`.
///
/// # Errors
///
/// Fails if the file does not hold the MIT License's permission paragraph.
fn mit_body(license_mit: &str) -> Result<String, String> {
    license_mit
        .find(MIT_BODY_START)
        .map(|start| license_mit[start..].to_owned())
        .ok_or_else(|| String::from("LICENSE-MIT does not hold the MIT License's text"))
}

/// The Markdown file: a short preface, then each crate with its license and texts.
fn render(sections: &[(Crate, Vec<Text>)]) -> String {
    use std::fmt::Write as _;
    let mut out = String::from(
        "# Licenses of the crates in this release\n\n\
         `baryon` and `libruntime.a` are statically linked, so they hold the code of the \
         external Rust crates below. Each is listed with the license its `Cargo.toml` declares \
         and the license and notice files it is published with. Baryon itself is licensed under \
         `LICENSE` (Apache-2.0) or `LICENSE-MIT` (MIT), at your option, and \
         `THIRD-PARTY-NOTICES.md` lists the other third-party content it carries. This file is \
         written by `cargo xtask crate-licenses` when the release is built.\n",
    );
    for (found, texts) in sections {
        // Writing to a `String` cannot fail.
        let _ = write!(
            out,
            "\n## {} {}\n\nLicense: {}\n",
            found.name, found.version, found.license
        );
        for text in texts {
            let _ = write!(
                out,
                "\n### {}\n\n````text\n{}\n````\n",
                text.file,
                text.body.trim_end()
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Metadata with `driver` depending on `small` normally, `macros` as a procedural macro,
    /// and `tested` only for tests; `runtime` has no dependencies.
    fn metadata() -> Value {
        let package = |id: &str, kind: &str| {
            json!({"id": id, "name": id, "version": "1.0.0", "license": "MIT",
                   "authors": ["Ada"], "manifest_path": format!("/crates/{id}/Cargo.toml"),
                   "targets": [{"kind": [kind]}]})
        };
        let normal = |id: &str| json!({"pkg": id, "dep_kinds": [{"kind": null}]});
        json!({
            "packages": [package("driver", "bin"), package("runtime", "staticlib"),
                         package("small", "lib"), package("macros", "proc-macro"),
                         package("tested", "lib")],
            "workspace_members": ["driver", "runtime"],
            "resolve": {"nodes": [
                {"id": "driver", "deps": [normal("small"), normal("macros"),
                                          {"pkg": "tested", "dep_kinds": [{"kind": "dev"}]}]},
                {"id": "runtime", "deps": []},
                {"id": "small", "deps": []},
                {"id": "macros", "deps": []},
                {"id": "tested", "deps": []}
            ]}
        })
    }

    #[test]
    fn keeps_normal_dependencies_and_drops_macros_tests_and_the_workspace() {
        let crates = linked_crates(&metadata()).unwrap();
        let names: Vec<&str> = crates.iter().map(|found| found.name.as_str()).collect();
        assert_eq!(names, ["small"]);
    }

    #[test]
    fn declared_mit_names_the_authors() {
        let found = Crate {
            name: String::from("parser"),
            version: String::from("0.0.21"),
            license: String::from("MIT"),
            authors: vec![
                String::from("Charlie Marsh"),
                String::from("RustPython Team"),
            ],
            directory: PathBuf::new(),
        };
        let text = declared_mit(&found, "Permission is hereby granted.").unwrap();
        assert_eq!(
            text.body,
            "MIT License\n\nCopyright (c) Charlie Marsh, RustPython Team\n\nPermission is hereby \
             granted."
        );
    }

    #[test]
    fn refuses_a_crate_with_no_file_that_is_not_mit() {
        let found = Crate {
            name: String::from("closed"),
            version: String::from("1.0.0"),
            license: String::from("Apache-2.0"),
            authors: Vec::new(),
            directory: PathBuf::new(),
        };
        assert_eq!(
            declared_mit(&found, "").unwrap_err(),
            "closed 1.0.0 ships no license file and is not declared MIT (it is `Apache-2.0`); \
             add its license before releasing"
        );
    }

    #[test]
    fn our_license_mit_holds_the_permission_paragraph() {
        let text = fs::read_to_string(workspace_root().join("LICENSE-MIT")).unwrap();
        assert!(mit_body(&text).unwrap().starts_with(MIT_BODY_START));
    }

    #[test]
    fn renders_each_crate_with_its_texts() {
        let found = Crate {
            name: String::from("small"),
            version: String::from("1.0.0"),
            license: String::from("MIT"),
            authors: Vec::new(),
            directory: PathBuf::new(),
        };
        let text = Text {
            file: String::from("LICENSE"),
            body: String::from("Terms.\n"),
        };
        let out = render(&[(found, vec![text])]);
        assert!(out.ends_with(
            "\n## small 1.0.0\n\nLicense: MIT\n\n### LICENSE\n\n````text\nTerms.\n````\n"
        ));
    }
}
