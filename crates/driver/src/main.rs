//! `npl`, the compiler's command line.
//!
//! ```text
//! npl check <directory>
//! npl lsp
//! ```
//!
//! `npl check` loads every `.npl` file under the directory as one package, checks it, and prints
//! each problem to standard error. Today checking means module naming, syntax, the constructs
//! the language supports, names that refer to nothing, and type errors; each new stage adds its
//! diagnostics here. The exit status is 0 when there are no errors, 1 when there are, and 2 when
//! the command itself is wrong or a file cannot be read.
//!
//! `npl lsp` runs the language server, which an editor starts and talks to over standard input
//! and output ([`lsp`], decision 0019). It reports the same diagnostics as `npl check`, as the
//! user types.

mod lsp;
mod package;

use diagnostics::Severity;
use driver::Session;
use std::io::{self, Write as _};
use std::path::Path;
use std::process::ExitCode;
use std::{env, fs};

/// How to call the command, printed when the arguments are wrong.
const USAGE: &str = "usage: npl check <directory> | npl lsp";

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let outcome = match arguments.as_slice() {
        [command, directory] if command == "check" => check(Path::new(directory)),
        [command] if command == "lsp" => lsp::serve(io::stdin().lock(), io::stdout().lock())
            .map(ExitCode::from)
            .map_err(|error| format!("lsp: {error}")),
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
    let paths = package::files(root).map_err(|error| format!("{}: {error}", root.display()))?;

    let mut session = Session::new();
    for path in &paths {
        let text =
            fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
        session
            .add_file(package::module_path(root, path), text)
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
