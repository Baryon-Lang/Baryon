//! `baryon watch`: rebuild on each save, in one process that keeps the package loaded
//! (roadmap 2.2.1, decision 0050).
//!
//! `baryon build` starts from nothing every time. `watch` reads the package once into a
//! [`Session`], builds it, and then looks at the files a few times a second. When one changes,
//! only its text is replaced in the session; `salsa` ([decision 0016]) then reuses every query
//! the edit did not reach, such as parsing and checking the other files, and the package is built
//! again. A file added or removed changes the package's modules, so the session is read again
//! from nothing.
//!
//! Each build splits the program into modules an edit changes only where it reaches
//! ([`Session::compile_chunks`]), and keeps every module's machine code in memory: a module
//! whose text did not change skips `clang`, which is about half of a build (roadmap 2.2.2).
//!
//! It runs until it is stopped, with `Ctrl-C`. Each rebuild prints `built <output> in <time>`
//! and how many modules `clang` compiled,
//! or the errors, to standard error; with `--run` the program then runs, with the terminal's
//! output, and its exit status is printed. A failed build never stops the watch: fixing the error
//! and saving builds again. Nor does a file that cannot be read after a change, as when an editor
//! saves by writing a new file and renaming it over the old one: the message is printed, and the
//! package is read again from nothing at the next change. Only a package that cannot be read at
//! the start, or a missing `clang` or runtime, ends the watch.
//!
//! With `--run`, the next look waits for the program to end: a program that never ends holds up
//! every later rebuild until it is stopped. Running it in the background, so a save could stop
//! it and start the new build, is left for when someone needs it.
//!
//! [decision 0016]: ../../docs/decisions/0016-salsa-query-engine.md
//!
//! Looking costs one `stat` of each file per look, and reads only a file whose size or
//! modification time changed. Polling needs no dependency and behaves the same on Linux and
//! macOS; a look every 200 ms keeps the wait for a save well under the rebuild itself.

use crate::link::Objects;
use crate::{Loaded, compile_loaded, package};
use database::File;
use driver::{Profile, Session};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

/// How long `watch` waits between looks at the files.
const PAUSE: Duration = Duration::from_millis(200);

/// What a file looked like at the last look: its size and modification time.
type Stamp = (u64, Option<SystemTime>);

/// The package as `watch` holds it: the session, and each file's handle in it and line count, by
/// path, so a changed file's text can be replaced.
struct Package {
    /// The package's files, checked and built.
    session: Session,
    /// Each file's handle in the session and how many lines it has, by path.
    files: BTreeMap<PathBuf, (File, usize)>,
}

impl Package {
    /// Reads every file of the package rooted at `root` into a new session.
    ///
    /// # Errors
    ///
    /// Fails with a message when the directory cannot be listed, or a file read or too large.
    fn load(root: &Path) -> Result<Self, String> {
        let paths = package::files(root).map_err(|error| format!("{}: {error}", root.display()))?;
        let mut session = Session::new();
        let mut files = BTreeMap::new();
        for path in paths {
            let text = read(&path)?;
            let lines = text.lines().count();
            let file = session
                .add_file(package::module_path(root, &path), text)
                .map_err(|error| error.to_string())?;
            files.insert(path, (file, lines));
        }
        Ok(Self { session, files })
    }

    /// Reads `path` again and replaces its text in the session; `salsa` then redoes only what
    /// depends on it.
    ///
    /// # Errors
    ///
    /// Fails with a message when the file cannot be read or is too large.
    fn update(&mut self, path: &Path) -> Result<(), String> {
        let text = read(path)?;
        if let Some((file, lines)) = self.files.get_mut(path) {
            *lines = text.lines().count();
            self.session
                .set_text(*file, text)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    /// The package as [`compile_loaded`] takes it, read in `load`.
    fn loaded(&self, load: Duration) -> Loaded<'_> {
        Loaded {
            session: &self.session,
            files: self.files.len(),
            lines: self.files.values().map(|&(_, lines)| lines).sum(),
            load,
        }
    }
}

/// Runs `baryon watch` on the package rooted at `root`: builds `output` for `profile`, then
/// rebuilds it after each change to the package's files, running it after each build when `run`
/// is set. Returns only on an error that a save cannot fix.
///
/// # Errors
///
/// Fails with a message when the package cannot be read at the start, or `clang` or the runtime
/// archive is missing.
pub(crate) fn watch(
    root: &Path,
    output: &Path,
    profile: Profile,
    run: bool,
) -> Result<ExitCode, String> {
    let mut stamps = look(root)?;
    let started = Instant::now();
    let mut package = Package::load(root)?;
    let mut load = started.elapsed();
    let mut objects = Objects::default();
    loop {
        rebuild(
            &package.loaded(load),
            root,
            output,
            profile,
            run,
            &mut objects,
        )?;
        loop {
            thread::sleep(PAUSE);
            // A listing that fails mid-save is looked at again at the next pause.
            let Ok(now) = look(root) else {
                continue;
            };
            if now == stamps {
                continue;
            }
            let started = Instant::now();
            let refreshed = if now.keys().eq(stamps.keys()) {
                now.iter()
                    .filter(|&(path, stamp)| stamps.get(path) != Some(stamp))
                    .try_for_each(|(path, _)| package.update(path))
            } else {
                Package::load(root).map(|loaded| package = loaded)
            };
            load = started.elapsed();
            match refreshed {
                Ok(()) => {
                    stamps = now;
                    break;
                }
                Err(message) => {
                    writeln!(io::stderr(), "baryon: {message}\nwatching for changes")
                        .map_err(|error| error.to_string())?;
                    // No stamps match an empty map, so the next change reads the package again.
                    stamps = BTreeMap::new();
                }
            }
        }
    }
}

/// The text of the file `path`.
///
/// # Errors
///
/// Fails with a message naming the file when it cannot be read.
fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}

/// Builds the loaded package into `output`, prints how long it took and how many modules
/// `clang` compiled, and runs it when `run` is set. A package with errors gets them printed,
/// and nothing is run. `objects` holds the machine code of the last build's modules, and a
/// module whose text is unchanged reuses it instead of running `clang` (roadmap 2.2.2).
///
/// # Errors
///
/// Fails with a message when `clang` or the runtime archive is missing, or the report cannot be
/// written.
fn rebuild(
    loaded: &Loaded<'_>,
    root: &Path,
    output: &Path,
    profile: Profile,
    run: bool,
    objects: &mut Objects,
) -> Result<(), String> {
    let started = Instant::now();
    let built = compile_loaded(
        loaded,
        root,
        output,
        profile,
        false,
        Some(objects),
        |session, directory| {
            session
                .compile_chunks(directory, profile)
                .map(|modules| (modules, Vec::new()))
        },
    )?;
    let took = loaded.load + started.elapsed();
    let mut stderr = io::stderr().lock();
    let report = |error: io::Error| error.to_string();
    if built.is_err() {
        writeln!(stderr, "watching for changes").map_err(report)?;
        return Ok(());
    }
    let (compiled, modules) = objects.compiled();
    writeln!(
        stderr,
        "built {} in {} ms, {compiled} of {modules} modules compiled",
        output.display(),
        took.as_millis()
    )
    .map_err(report)?;
    if run {
        drop(stderr);
        let status = Command::new(output)
            .status()
            .map_err(|error| format!("{}: {error}", output.display()))?;
        let code = status
            .code()
            .map_or_else(|| "a signal".to_owned(), |code| code.to_string());
        writeln!(io::stderr(), "exited with {code}").map_err(report)?;
    }
    Ok(())
}

/// The stamp of every `.byn` file under `root`, looked at now.
///
/// # Errors
///
/// Fails with a message when a directory cannot be listed or a file's metadata read.
fn look(root: &Path) -> Result<BTreeMap<PathBuf, Stamp>, String> {
    let paths = package::files(root).map_err(|error| format!("{}: {error}", root.display()))?;
    paths
        .into_iter()
        .map(|path| {
            let metadata =
                fs::metadata(&path).map_err(|error| format!("{}: {error}", path.display()))?;
            Ok((path, (metadata.len(), metadata.modified().ok())))
        })
        .collect()
}
