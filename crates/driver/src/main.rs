//! `baryon`, the compiler's command line.
//!
//! ```text
//! baryon check <directory>
//! baryon build [--release] [--timings] <directory> <output>
//! baryon run [--release] [--timings] <directory>
//! baryon test [--release] [--timings] [-k <text>] <directory>
//! baryon watch [--release] [--run] <directory> <output>
//! baryon doc <directory> <output>
//! baryon lsp
//! baryon --version
//! baryon --help
//! ```
//!
//! `baryon --version` (or `-V`) prints `baryon` and the release, such as `baryon 0.2.0`, and
//! `baryon --help` (or `-h`, or `help`) prints the usage line; both to standard output, with
//! exit status 0. Any other arguments print the usage line to standard error, with status 2.
//!
//! `baryon check` loads every `.byn` file under the directory as one package, checks it, and prints
//! each problem to standard error. Today checking means module naming, syntax, the constructs
//! the language supports, names that refer to nothing, and type errors; each new stage adds its
//! diagnostics here. The exit status is 0 when there are no errors, 1 when there are, and 2 when
//! the command itself is wrong or a file cannot be read.
//!
//! `baryon build` checks the package the same way, compiles it, and links the executable `output`
//! with `clang` and the runtime ([`link`]). Running it runs the package's one `main` function;
//! a failed check in it prints a panic with its location and exits with status 101. A package
//! that does not check, or uses what the compiler cannot build yet, gets its errors printed and
//! the exit status 1. The executable carries line tables, so `gdb` and `lldb` stop at and show
//! `.byn` lines. This is a prototype (roadmap 1.11.3); a generic function is compiled once per
//! type it is called at (roadmap 1.9.5).
//!
//! `build`, `run` and `test` make a debug build unless given `--release`, anywhere after the
//! command. A debug build compiles fast and runs unoptimized (`clang -O0`); a release build is
//! optimized (`clang -O2`) (decision 0048). Both carry line tables; in a debug build, integer
//! arithmetic that overflows panics, and in a release build it wraps around ([`Profile`],
//! decision 0030).
//!
//! `--timings`, also anywhere after `build`, `run` or `test`, prints to standard error how long
//! each stage of the build took, once the executable is linked ([`Timings`]). It is how a
//! compile-speed change measures its gain (roadmap 2.0).
//!
//! `baryon run` builds the package the same way into a scratch directory, runs the program with
//! the terminal's standard input and output, removes it, and exits with the program's own exit
//! status: 0 when it ran to its end, 101 when a check failed. A package that does not build
//! exits as `baryon build` does, with nothing run.
//!
//! `baryon test` builds the package the same way, but as a test program ([`runner`]): it runs
//! every top-level function decorated `@test`, each in its own process so a failed check ends
//! only its own test, and reports each as `ok` or `FAILED`, with a failed test's output after
//! the list. The package needs no `main`. The exit status is 0 when every test passed, 1 when
//! one failed or the package does not build. This is a prototype (roadmap 1.11.6).
//!
//! `baryon watch` builds the package as `build` does, then keeps it loaded and rebuilds it each
//! time a file is saved, running it after each build with `--run` ([`watch`], decision 0050).
//! `salsa` reuses what the edit did not reach, so a rebuild skips parsing and checking the files
//! that did not change. It runs until stopped.
//!
//! `baryon doc` loads the package the same way and writes its documentation as Markdown into the
//! output directory, creating it if needed: one page per module, named after the module, such
//! as `physics.nbody.md`, and an `index.md` listing them. A package with syntax or module naming
//! errors gets no pages; its errors are printed and the exit status is 1.
//!
//! `baryon lsp` runs the language server, which an editor starts and talks to over standard input
//! and output ([`lsp`], decision 0019). It reports the same diagnostics as `baryon check`, as the
//! user types.

mod link;
mod lsp;
mod package;
mod runner;
mod timings;
mod watch;

use diagnostics::{Diagnostic, Severity};
use driver::{Profile, Session};
use std::io::{self, Write as _};
use std::path::Path;
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};
use std::{env, fs};
use timings::Timings;

/// How to call the command, printed when the arguments are wrong.
<<<<<<< HEAD
const USAGE: &str = "usage: baryon check <directory> | baryon build [--release] [--timings] <directory> <output> | baryon run [--release] [--timings] <directory> | baryon test [--release] [--timings] [-k <text>] <directory> | baryon watch [--release] [--run] <directory> <output> | baryon doc <directory> <output> | baryon lsp";
=======
const USAGE: &str = "usage: baryon check <directory> | baryon build [--release] [--timings] <directory> <output> | baryon run [--release] [--timings] <directory> | baryon test [--release] [--timings] [-k <text>] <directory> | baryon doc <directory> <output> | baryon lsp";
>>>>>>> origin/main

/// The flag that makes `build`, `run` and `test` compile a release build.
const RELEASE: &str = "--release";

/// The flag that makes `build`, `run` and `test` print how long each stage of the build took.
const TIMINGS: &str = "--timings";

<<<<<<< HEAD
/// The flag that makes `watch` run the program after each build.
const RUN: &str = "--run";

=======
>>>>>>> origin/main
/// The option that makes `test` run only the tests whose name contains the text after it, as
/// pytest's `-k` does at its simplest (roadmap 2.4.3).
const FILTER: &str = "-k";

fn main() -> ExitCode {
    let mut arguments: Vec<String> = env::args().skip(1).collect();
    let builds = arguments
        .first()
        .is_some_and(|command| ["build", "run", "test", "watch"].contains(&command.as_str()));
    let profile = if take_flag(&mut arguments, builds, RELEASE) {
        Profile::Release
    } else {
        Profile::Debug
    };
    let timings = take_flag(&mut arguments, builds, TIMINGS);
    let filter = take_filter(&mut arguments);
<<<<<<< HEAD
    let watching = arguments.first().is_some_and(|command| command == "watch");
    let run_each = take_flag(&mut arguments, watching, RUN);
=======
>>>>>>> origin/main
    let outcome = match arguments.as_slice() {
        [command, directory] if command == "check" => check(Path::new(directory)),
        [command, directory, output] if command == "build" => {
            build(Path::new(directory), Path::new(output), profile, timings)
        }
        [command, directory] if command == "run" => run(Path::new(directory), profile, timings),
        [command, directory, output] if command == "watch" && !timings => {
            watch::watch(Path::new(directory), Path::new(output), profile, run_each)
        }
        [command, directory, output] if command == "doc" => {
            document(Path::new(directory), Path::new(output))
        }
        [command, directory] if command == "test" => {
            test(Path::new(directory), profile, timings, filter.as_deref())
        }
        [flag] if flag == "--version" || flag == "-V" => {
            // A tester's bug report starts with this line.
            println_to_stdout(&format!("baryon {}", env!("CARGO_PKG_VERSION")))
        }
        [flag] if flag == "--help" || flag == "-h" || flag == "help" => println_to_stdout(USAGE),
        [command] if command == "lsp" => lsp::serve(io::stdin().lock(), io::stdout().lock())
            .map(ExitCode::from)
            .map_err(|error| format!("lsp: {error}")),
        _ => Err(USAGE.to_owned()),
    };
    match outcome {
        Ok(code) => code,
        Err(message) => {
            // Nothing more can be reported if standard error itself fails.
            let _ = writeln!(io::stderr(), "baryon: {message}");
            ExitCode::from(2)
        }
    }
}

/// Removes every `flag` from `arguments` and says whether there was one, when `builds`, the
/// command being `build`, `run` or `test`; otherwise leaves `arguments` alone and says no, so a
/// flag given to another command is reported as wrong usage.
fn take_flag(arguments: &mut Vec<String>, builds: bool, flag: &str) -> bool {
    if builds && arguments.iter().any(|argument| argument == flag) {
        arguments.retain(|argument| argument != flag);
        true
    } else {
        false
    }
}

/// Removes `-k <text>` from `arguments` when the command is `test`, and returns the text: the
/// part of a test's name that picks it. Any other command keeps the option, so it is reported
/// as wrong usage; so is a `-k` given twice or with nothing after it.
fn take_filter(arguments: &mut Vec<String>) -> Option<String> {
    if arguments.first().is_none_or(|command| command != "test") {
        return None;
    }
    let at = arguments.iter().position(|argument| argument == FILTER)?;
    if at + 1 >= arguments.len() || arguments[at + 2..].contains(&FILTER.to_owned()) {
        return None;
    }
    let text = arguments.remove(at + 1);
    arguments.remove(at);
    Some(text)
}

/// Prints `line` to standard output, for `--version` and `--help`, and succeeds.
///
/// # Errors
///
/// Fails with a message when standard output cannot be written, such as a closed pipe.
fn println_to_stdout(line: &str) -> Result<ExitCode, String> {
    writeln!(io::stdout(), "{line}").map_err(|error| error.to_string())?;
    Ok(ExitCode::SUCCESS)
}

/// Loads the package rooted at `root` into a new session, and returns it with the number of
/// files loaded and the number of lines in them.
///
/// # Errors
///
/// Fails with a message when a file cannot be read or is too large.
fn load(root: &Path) -> Result<(Session, usize, usize), String> {
    let paths = package::files(root).map_err(|error| format!("{}: {error}", root.display()))?;
    let mut session = Session::new();
    let mut lines = 0;
    for path in &paths {
        let text =
            fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
        lines += text.lines().count();
        session
            .add_file(package::module_path(root, path), text)
            .map_err(|error| error.to_string())?;
    }
    Ok((session, paths.len(), lines))
}

/// Runs `baryon check` on the package rooted at `root`.
///
/// # Errors
///
/// Fails with a message when a file cannot be read or is too large.
fn check(root: &Path) -> Result<ExitCode, String> {
    let (session, files, _) = load(root)?;
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

/// Runs `baryon build` on the package rooted at `root`, writing the executable `output` built
/// for `profile`, and printing each stage's time when `timings` is set.
///
/// # Errors
///
/// Fails with a message when a file cannot be read, or when `clang` or the runtime archive is
/// missing or the link fails.
fn build(root: &Path, output: &Path, profile: Profile, timings: bool) -> Result<ExitCode, String> {
    let files = match compile(root, output, profile, timings, |session, directory| {
        session
            .compile_parts(directory, profile, parts(profile))
            .map(|modules| (modules, Vec::new()))
    })? {
        Ok((files, _)) => files,
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

/// Runs `baryon run` on the package rooted at `root`: builds it for `profile` into a scratch
/// directory, printing each stage's time when `timings` is set, runs it, and returns its exit
/// status. A program ended by a signal, which has no
/// status, gives 1.
///
/// # Errors
///
/// Fails with a message when the package cannot be built for the reasons [`build`] gives, or
/// the program cannot be started.
fn run(root: &Path, profile: Profile, timings: bool) -> Result<ExitCode, String> {
    let scratch = env::temp_dir().join(format!("baryon-run-{}", std::process::id()));
    fs::create_dir_all(&scratch).map_err(|error| format!("{}: {error}", scratch.display()))?;
    let program = scratch.join("program");
    let built = compile(root, &program, profile, timings, |session, directory| {
        session
            .compile_parts(directory, profile, parts(profile))
            .map(|modules| (modules, Vec::new()))
    });
    let ran = built.and_then(|built| match built {
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

/// Runs `baryon test` on the package rooted at `root`: builds its test program for `profile` into
/// a scratch directory, printing each stage's time when `timings` is set, and runs each test
/// ([`runner::run`]).
///
/// # Errors
///
/// Fails with a message when the package cannot be built for the reasons [`build`] gives, the
/// program cannot be started, or the report cannot be written.
fn test(
    root: &Path,
    profile: Profile,
    timings: bool,
    filter: Option<&str>,
) -> Result<ExitCode, String> {
    let scratch = env::temp_dir().join(format!("baryon-test-{}", std::process::id()));
    fs::create_dir_all(&scratch).map_err(|error| format!("{}: {error}", scratch.display()))?;
    let program = scratch.join("tests");
    let built = compile(root, &program, profile, timings, |session, directory| {
        session.compile_tests_parts(directory, profile, parts(profile))
    });
    let ran = built.and_then(|built| match built {
        Ok((_, tests)) => runner::run(&program, &tests, filter, &mut io::stdout().lock()),
        Err(code) => Ok(code),
    });
    let _ = fs::remove_dir_all(&scratch);
    ran
}

/// Compiles the package rooted at `root` with `program`, one of [`Session::compile_parts`] and
/// [`Session::compile_tests_parts`] giving the modules and the test names, and links the modules
/// into the executable `output`, optimized when `profile` is [`Profile::Release`]. Returns the
/// number of files compiled and the test names, or, when the package has errors, prints them and
/// returns the exit status 1. When `timings` is set and the
/// executable is linked, prints each stage's time to standard error ([`Timings`]).
///
/// # Errors
///
/// Fails with a message when a file cannot be read, or when `clang` or the runtime archive is
/// missing or the link fails.
fn compile(
    root: &Path,
    output: &Path,
    profile: Profile,
    timings: bool,
    program: impl FnOnce(&Session, Option<&str>) -> Result<(Vec<String>, Vec<String>), Vec<Diagnostic>>,
) -> Result<Result<(usize, Vec<String>), ExitCode>, String> {
    let started = Instant::now();
    let (session, files, lines) = load(root)?;
    let loaded = Loaded {
        session: &session,
        files,
        lines,
        load: started.elapsed(),
    };
    compile_loaded(&loaded, root, output, profile, timings, program)
}

/// A package already read into a [`Session`]: what [`compile_loaded`] builds. `baryon watch`
/// keeps one between rebuilds, so `salsa` reuses whatever an edit left unchanged.
pub(crate) struct Loaded<'s> {
    /// The package's files.
    pub(crate) session: &'s Session,
    /// How many files it has.
    pub(crate) files: usize,
    /// How many lines they hold together.
    pub(crate) lines: usize,
    /// How long reading them took, for `--timings`.
    pub(crate) load: Duration,
}

/// [`compile`] for a package already loaded, rooted at `root`.
///
/// # Errors
///
/// Fails as [`compile`] does, except that every file is already read.
pub(crate) fn compile_loaded(
    loaded: &Loaded<'_>,
    root: &Path,
    output: &Path,
    profile: Profile,
    timings: bool,
    program: impl FnOnce(&Session, Option<&str>) -> Result<(Vec<String>, Vec<String>), Vec<Diagnostic>>,
) -> Result<Result<(usize, Vec<String>), ExitCode>, String> {
    let (session, files, lines) = (loaded.session, loaded.files, loaded.lines);
    let loaded_at = Instant::now();
    let directory =
        fs::canonicalize(root).map_err(|error| format!("{}: {error}", root.display()))?;
    let (modules, tests) = match program(session, Some(&directory.to_string_lossy())) {
        Ok(compiled) => compiled,
        Err(problems) => {
            let mut stderr = io::stderr().lock();
            problems
                .iter()
                .try_for_each(|problem| {
                    writeln!(stderr, "{}\n", diagnostics::render(problem, session))
                })
                .map_err(|error| error.to_string())?;
            return Ok(Err(ExitCode::FAILURE));
        }
    };
    let compiled = Instant::now();
    let linked = link::link(&modules, output, profile == Profile::Release)?;
    if timings {
        let report = Timings {
            files,
            lines,
            load: loaded.load,
            compile: compiled - loaded_at,
            machine_code: linked.machine_code,
            link: linked.link,
        };
        writeln!(io::stderr(), "{report}").map_err(|error| error.to_string())?;
    }
    Ok(Ok((files, tests)))
}

/// How many LLVM modules to split a program built for `profile` into (roadmap 2.6).
///
/// A debug build gets one per core the operating system lets this process use, so each gets a
/// `clang` of its own, or one when that is unknown. A release build stays one module: `clang`
/// never inlines a call into another module, and at `-O2` a small function called from a hot
/// loop in another part would run slower, a cost no benchmark measures yet. At `-O0` nothing
/// is inlined, so splitting a debug build costs nothing.
fn parts(profile: Profile) -> usize {
    match profile {
        Profile::Debug => std::thread::available_parallelism().map_or(1, usize::from),
        Profile::Release => 1,
    }
}

/// Runs `baryon doc` on the package rooted at `root`, writing its pages into `output`.
///
/// # Errors
///
/// Fails with a message when a file cannot be read, or the output directory or a page cannot
/// be written.
fn document(root: &Path, output: &Path) -> Result<ExitCode, String> {
    let (session, _, _) = load(root)?;
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
