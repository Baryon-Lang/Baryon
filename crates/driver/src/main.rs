//! `npl`, the compiler's command line.
//!
//! ```text
//! npl check <directory>
//! npl build <directory> <output>
//! npl run <directory>
//! npl doc <directory> <output>
//! npl lsp
//! ```
//!
//! `npl check` loads every `.npl` file under the directory as one package, checks it, and prints
//! each problem to standard error. Today checking means module naming, syntax, the constructs
//! the language supports, names that refer to nothing, and type errors; each new stage adds its
//! diagnostics here. The exit status is 0 when there are no errors, 1 when there are, and 2 when
//! the command itself is wrong or a file cannot be read.
//!
//! `npl build` checks the package the same way, compiles it, and links the executable `output`
//! with `clang` and the runtime ([`link`]). Running it runs the package's one `main` function;
//! a failed check in it prints a panic with its location and exits with status 101. A package
//! that does not check, or uses what the compiler cannot build yet, gets its errors printed and
//! the exit status 1. The executable carries line tables, so `gdb` and `lldb` stop at and show
//! `.npl` lines. This is a prototype (roadmap 1.11.3): no generic functions yet.
//!
//! `npl run` builds the package the same way into a scratch directory, runs the program with
//! the terminal's standard input and output, removes it, and exits with the program's own exit
//! status: 0 when it ran to its end, 101 when a check failed. A package that does not build
//! exits as `npl build` does, with nothing run.
//!
//! `npl doc` loads the package the same way and writes its documentation as Markdown into the
//! output directory, creating it if needed: one page per module, named after the module, such
//! as `physics.nbody.md`, and an `index.md` listing them. A package with syntax or module naming
//! errors gets no pages; its errors are printed and the exit status is 1.
//!
//! `npl lsp` runs the language server, which an editor starts and talks to over standard input
//! and output ([`lsp`], decision 0019). It reports the same diagnostics as `npl check`, as the
//! user types.

mod link;
mod lsp;
mod package;

use diagnostics::Severity;
use driver::Session;
use std::io::{self, Write as _};
use std::path::Path;
use std::process::{Command, ExitCode};
use std::{env, fs};

/// How to call the command, printed when the arguments are wrong.
const USAGE: &str = "usage: npl check <directory> | npl build <directory> <output> | npl run <directory> | npl doc <directory> <output> | npl lsp";

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let outcome = match arguments.as_slice() {
        [command, directory] if command == "check" => check(Path::new(directory)),
        [command, directory, output] if command == "build" => {
            build(Path::new(directory), Path::new(output))
        }
        [command, directory] if command == "run" => run(Path::new(directory)),
        [command, directory, output] if command == "doc" => {
            document(Path::new(directory), Path::new(output))
        }
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

/// Loads the package rooted at `root` into a new session, and returns it with the number of
/// files loaded.
///
/// # Errors
///
/// Fails with a message when a file cannot be read or is too large.
fn load(root: &Path) -> Result<(Session, usize), String> {
    let paths = package::files(root).map_err(|error| format!("{}: {error}", root.display()))?;
    let mut session = Session::new();
    for path in &paths {
        let text =
            fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
        session
            .add_file(package::module_path(root, path), text)
            .map_err(|error| error.to_string())?;
    }
    Ok((session, paths.len()))
}

/// Runs `npl check` on the package rooted at `root`.
///
/// # Errors
///
/// Fails with a message when a file cannot be read or is too large.
fn check(root: &Path) -> Result<ExitCode, String> {
    let (session, files) = load(root)?;
    let problems = session.check();
    let errors = problems
        .iter()
        .filter(|problem| problem.severity() == Severity::Error)
        .count();
    let mut stderr = io::stderr().lock();
    let report = problems
        .iter()
        .try_for_each(|problem| writeln!(stderr, "{}\n", diagnostics::render(problem, &session)))
        .and_then(|()| writeln!(stderr, "checked {files} file(s): {errors} error(s)"));
    report.map_err(|error| error.to_string())?;
    Ok(if errors == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// Runs `npl build` on the package rooted at `root`, writing the executable `output`.
///
/// # Errors
///
/// Fails with a message when a file cannot be read, or when `clang` or the runtime archive is
/// missing or the link fails.
fn build(root: &Path, output: &Path) -> Result<ExitCode, String> {
    let files = match compile(root, output)? {
        Ok(files) => files,
        Err(code) => return Ok(code),
    };
    writeln!(
        io::stderr(),
        "built {} from {files} file(s)",
        output.display()
    )
    .map_err(|error| error.to_string())?;
    Ok(ExitCode::SUCCESS)
}

/// Runs `npl run` on the package rooted at `root`: builds it into a scratch directory, runs it,
/// and returns its exit status. A program ended by a signal, which has no status, gives 1.
///
/// # Errors
///
/// Fails with a message when the package cannot be built for the reasons [`build`] gives, or
/// the program cannot be started.
fn run(root: &Path) -> Result<ExitCode, String> {
    let scratch = env::temp_dir().join(format!("npl-run-{}", std::process::id()));
    fs::create_dir_all(&scratch).map_err(|error| format!("{}: {error}", scratch.display()))?;
    let program = scratch.join("program");
    let ran = compile(root, &program).and_then(|built| match built {
        Ok(_) => Command::new(&program)
            .status()
            .map(|status| {
                let code = status.code().unwrap_or(1);
                ExitCode::from(u8::try_from(code).unwrap_or(1))
            })
            .map_err(|error| format!("{}: {error}", program.display())),
        Err(code) => Ok(code),
    });
    let _ = fs::remove_dir_all(&scratch);
    ran
}

/// Compiles the package rooted at `root` and links it into the executable `output`. Returns
/// the number of files compiled, or, when the package has errors, prints them and returns the
/// exit status 1.
///
/// # Errors
///
/// Fails with a message when a file cannot be read, or when `clang` or the runtime archive is
/// missing or the link fails.
fn compile(root: &Path, output: &Path) -> Result<Result<usize, ExitCode>, String> {
    let (session, files) = load(root)?;
    let directory =
        fs::canonicalize(root).map_err(|error| format!("{}: {error}", root.display()))?;
    let module = match session.compile(Some(&directory.to_string_lossy())) {
        Ok(module) => module,
        Err(problems) => {
            let mut stderr = io::stderr().lock();
            problems
                .iter()
                .try_for_each(|problem| {
                    writeln!(stderr, "{}\n", diagnostics::render(problem, &session))
                })
                .map_err(|error| error.to_string())?;
            return Ok(Err(ExitCode::FAILURE));
        }
    };
    link::link(&module, output)?;
    Ok(Ok(files))
}

/// Runs `npl doc` on the package rooted at `root`, writing its pages into `output`.
///
/// # Errors
///
/// Fails with a message when a file cannot be read, or the output directory or a page cannot
/// be written.
fn document(root: &Path, output: &Path) -> Result<ExitCode, String> {
    let (session, _) = load(root)?;
    let mut stderr = io::stderr().lock();
    let modules = match session.document() {
        Ok(modules) => modules,
        Err(problems) => {
            problems
                .iter()
                .try_for_each(|problem| {
                    writeln!(stderr, "{}\n", diagnostics::render(problem, &session))
                })
                .map_err(|error| error.to_string())?;
            return Ok(ExitCode::FAILURE);
        }
    };
    let write = |name: &str, page: String| {
        let path = output.join(name);
        fs::write(&path, page).map_err(|error| format!("{}: {error}", path.display()))
    };
    fs::create_dir_all(output).map_err(|error| format!("{}: {error}", output.display()))?;
    for module in &modules {
        write(&module.file_name(), module.markdown())?;
    }
    write("index.md", documentation::index(&modules))?;
    writeln!(
        stderr,
        "documented {} module(s) in {}",
        modules.len(),
        output.display()
    )
    .map_err(|error| error.to_string())?;
    Ok(ExitCode::SUCCESS)
}
