//! `npl`, the compiler's command line.
//!
//! ```text
//! npl check <directory>
//! ```
//!
//! loads every `.npl` file under the directory as one package, checks it, and prints each
//! problem to standard error. Today checking means module naming and syntax; each new stage adds
//! its diagnostics here. The exit status is 0 when there are no errors, 1 when there are, and 2
//! when the command itself is wrong or a file cannot be read.

use diagnostics::Severity;
use driver::Session;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{env, fs};

/// How to call the command, printed when the arguments are wrong.
const USAGE: &str = "usage: npl check <directory>";

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let outcome = match arguments.as_slice() {
        [command, directory] if command == "check" => check(Path::new(directory)),
        _ => Err(USAGE.to_owned()),
    };
    match outcome {
        Ok(code) => code,
        Err(message) => {
            // Nothing more can be reported if standard error itself fails.
            let _ = writeln!(io::stderr(), "npl: {message}");
            ExitCode::from(2)
        }
    }
}

/// Runs `npl check` on the package rooted at `root`.
///
/// # Errors
///
/// Fails with a message when a file cannot be read or is too large.
fn check(root: &Path) -> Result<ExitCode, String> {
    let mut paths = Vec::new();
    collect(root, &mut paths).map_err(|error| format!("{}: {error}", root.display()))?;
    paths.sort();

    let mut session = Session::new();
    for path in &paths {
        let text =
            fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
        session
            .add_file(module_path(root, path), text)
            .map_err(|error| error.to_string())?;
    }

    let problems = session.check();
    let errors = problems
        .iter()
        .filter(|problem| problem.severity() == Severity::Error)
        .count();
    let mut stderr = io::stderr().lock();
    let report = problems
        .iter()
        .try_for_each(|problem| writeln!(stderr, "{}\n", diagnostics::render(problem, &session)))
        .and_then(|()| writeln!(stderr, "checked {} file(s): {errors} error(s)", paths.len()));
    report.map_err(|error| error.to_string())?;
    Ok(if errors == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// Appends every `.npl` file under `directory`, at any depth, to `paths`.
///
/// # Errors
///
/// Fails when a directory cannot be listed.
fn collect(directory: &Path, paths: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, paths)?;
        } else if path.extension().is_some_and(|extension| extension == "npl") {
            paths.push(path);
        }
    }
    Ok(())
}

/// The name a file gets in the package: its path relative to `root`, with `/` between
/// directories on every operating system, which is what decides its module name.
fn module_path(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
