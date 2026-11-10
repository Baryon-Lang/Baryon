//! How `baryon build` finds `clang` (decision 0020): it needs version 15 or newer, and when the
//! only `clang` on `PATH` is older, or there is none, it says so and how to install one, before
//! compiling anything. A stand-in `clang`, a shell script that prints a version, plays each case.

use super::{BARYON, build_runtime, package, stderr};
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Command, Output};

/// The end of every message about a missing or old `clang`.
const INSTALL: &str = "Install it with `xcode-select --install` on macOS, or the `clang` package \
     on Linux; where that is older than 15, such as on Ubuntu 22.04, install `clang-15` or newer\n";

/// Runs `baryon build` on a one-line program with only `bin` on `PATH`, after `bin` fills it.
fn build_with_path(name: &str, bin: impl FnOnce(&Path) -> io::Result<()>) -> io::Result<Output> {
    build_runtime();
    let directory = package(name, &[("main.byn", "def main():\n    print(1)\n")])?;
    let path = directory.join("bin");
    fs::create_dir_all(&path)?;
    bin(&path)?;
    Command::new(BARYON)
        .arg("build")
        .arg(directory.join("package"))
        .arg(directory.join("program"))
        .env("PATH", &path)
        .output()
}

/// Writes an executable stand-in `clang` called `name` into `bin` that prints `version` for
/// `--version`.
fn fake_clang(bin: &Path, name: &str, version: &str) -> io::Result<()> {
    let script = bin.join(name);
    fs::write(&script, format!("#!/bin/sh\necho '{version}'\n"))?;
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755))
}

/// A `clang` 14, Ubuntu 22.04's default, is refused by name and version.
#[test]
fn an_old_clang_is_refused_with_its_version() {
    let output = build_with_path("old-clang", |bin| {
        fake_clang(bin, "clang", "Ubuntu clang version 14.0.0-1ubuntu1.1")
    })
    .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        stderr(&output),
        format!(
            "baryon: `clang` is version 14, and `baryon build` needs clang 15 or newer. {INSTALL}"
        )
    );
}

/// No `clang` at all gets the message that says how to install one.
#[test]
fn a_missing_clang_says_how_to_install_one() {
    let output = build_with_path("no-clang", |_| Ok(())).unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        stderr(&output),
        format!(
            "baryon: `clang` was not found; `baryon build` uses it to compile and link. {INSTALL}"
        )
    );
}
