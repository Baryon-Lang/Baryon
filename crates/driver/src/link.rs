//! `baryon build`: hands the compiled module to `clang`, which optimizes it, compiles it and links
//! it with the runtime into an executable (decisions 0020 and 0021).
//!
//! The runtime is the static library `libruntime.a`, found next to the `baryon` executable, where
//! `cargo build` puts both. The module is written to a scratch directory and removed after.
//!
//! The backend emits opaque pointers, which `clang` accepts from version 15 (decision 0020), so
//! [`find_clang`] looks for a `clang` that new: `clang` itself, then the versioned names Linux
//! distributions install side by side, such as `clang-15` on Ubuntu 22.04, whose plain `clang`
//! is 14.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};
use std::{env, fs};

/// How long the two `clang` stages of [`link`] took, for `--timings`.
pub(crate) struct Linked {
    /// Optimizing the module and writing machine code into an object file.
    pub(crate) machine_code: Duration,
    /// Linking the object file with the runtime, and on macOS running `dsymutil`.
    pub(crate) link: Duration,
}

/// The machine code of each module compiled so far, by the module's text, which `baryon watch`
/// keeps between rebuilds so a module an edit did not change skips `clang` (roadmap 2.2.2).
///
/// Only the modules of the latest build are kept: about 2 MB of object code and 24 MB of text
/// for the 4,706-line program of roadmap 2.0, held in memory so nothing is left on disk when
/// the watch is stopped. A cache serves builds of one profile only, since the text does not
/// say how `clang` optimized it.
#[derive(Default)]
pub(crate) struct Objects {
    /// Each module's object file, by the module's text.
    built: HashMap<String, Vec<u8>>,
    /// How many modules the latest build had `clang` compile.
    compiled: usize,
    /// The `clang` the first build found, and the flag that links with `lld` where there is
    /// one: asked once per watch rather than at every rebuild, since each question starts a
    /// `clang`, about 25 ms.
    tools: Option<(String, Option<&'static str>)>,
}

impl Objects {
    /// How many modules the latest build had `clang` compile, and how many it had.
    pub(crate) fn compiled(&self) -> (usize, usize) {
        (self.compiled, self.built.len())
    }
}

/// The system libraries the runtime, a Rust static library, needs, as
/// `rustc --print native-static-libs` lists them for each system.
const SYSTEM_LIBRARIES: &[&str] = if cfg!(target_os = "macos") {
    &["-lSystem", "-lc", "-lm"]
} else {
    &[
        "-lgcc_s",
        "-lutil",
        "-lrt",
        "-lpthread",
        "-lm",
        "-ldl",
        "-lc",
    ]
};

/// The linker flag that drops every function and constant nothing reaches. The runtime is a
/// Rust static library with the whole standard library inside it, and most of it is never
/// called: dropping it takes a program that prints a number from 5.2 MB to 4.3 MB on Linux
/// (roadmap 1.12.4), and what is kept keeps its line tables.
const DROP_UNUSED: &str = if cfg!(target_os = "macos") {
    "-Wl,-dead_strip"
} else {
    "-Wl,--gc-sections"
};

/// What `clang` is asked for when it optimizes the module, beyond `-O2`.
///
/// `-force-ordered-reductions` lets the loop vectorizer vectorize a loop that sums floats, such
/// as `total += x[j] * w[j]`, while still adding the terms one at a time in index order, as
/// gfortran does. Without it LLVM leaves such a loop scalar, since reordering the additions would
/// change the result. Results are the same to the bit; the n-body benchmark kernel ran in 0.61 s
/// with it and 1.18 s without (roadmap 3.3). The option is LLVM's own, through `-mllvm`, and
/// every `clang` from 15, the oldest this driver accepts, knows it.
const OPTIMIZE: &[&str] = &["-O2", "-mllvm", "-force-ordered-reductions=true"];

/// What `clang` is asked for in a debug build: no optimization, so the module becomes machine
/// code about five times sooner, and the program runs up to an order of magnitude slower
/// (decision 0048). A 4,706-line program's object file took 9.0 s at `-O2` and 1.8 s at `-O0`.
/// Results are the same to the bit: `-O0` never reorders arithmetic, and [`OPTIMIZE`] keeps
/// float sums in order.
const DEBUG: &[&str] = &["-O0"];

/// The C math library, linked before the runtime. LLVM calls `floor` and `trunc` for those
/// operations on CPUs without an instruction for them, and the runtime, a Rust static library,
/// carries portable versions under the same names. Naming the system library first makes the
/// linker take its versions, which pick the fastest instruction the CPU has: the Lennard-Jones
/// benchmark kernel ran in 6.1 s with them and 8.0 s with the runtime's (roadmap 3.3). Both are
/// exact, so results do not change.
const MATH_LIBRARY: &str = "-lm";

/// Compiles the LLVM modules `modules`, at `-O2` for a release build and `-O0` for a debug build
/// (`optimize`, decision 0048), and links them with the runtime into the executable `output`,
/// keeping the modules' line tables (`-g`) so debuggers and panic backtraces show `.byn` lines.
///
/// Each module is compiled to an object file in a scratch directory by a `clang` of its own,
/// all at once, so a program split into one module per core ([`Session::compile_parts`]) uses
/// every core (roadmap 2.6); then the objects are linked. On Linux the line tables are copied
/// into the executable. On macOS the linker leaves them in the object files and records where
/// they were, so `dsymutil` collects them into `output.dSYM` beside the executable before the
/// scratch directory, and the object files with it, is removed. `clang` runs `dsymutil` itself
/// only for source files it compiles, not for an LLVM module.
///
/// With `kept`, a module whose text is there skips `clang`: its kept machine code is written
/// to the scratch directory instead. Afterwards `kept` holds the machine code of exactly this
/// build's modules.
///
/// Returns how long the object files and the link took ([`Linked`]); writing the modules to the
/// scratch directory counts toward the object files.
///
/// [`Session::compile_parts`]: driver::Session::compile_parts
pub(crate) fn link(
    modules: &[String],
    output: &Path,
    optimize: bool,
    kept: Option<&mut Objects>,
) -> Result<Linked, String> {
    let runtime = runtime()?;
    let found = kept.as_ref().and_then(|kept| kept.tools.clone());
    let clang = match &found {
        Some((clang, _)) => clang.clone(),
        None => find_clang()?,
    };
    let scratch = env::temp_dir().join(format!("baryon-build-{}", std::process::id()));
    fs::create_dir_all(&scratch).map_err(|error| format!("{}: {error}", scratch.display()))?;
    let objects: Vec<PathBuf> = (0..modules.len())
        .map(|index| scratch.join(format!("module{index}.o")))
        .collect();
    let started = Instant::now();
    let mut compiled = started;
    let lld = match &found {
        Some(_) => Lld(None),
        None => Lld::look(&clang),
    };
    let mut lld_flag = found.and_then(|(_, linker)| linker);
    let result = compile_objects(
        &clang,
        &scratch,
        modules,
        &objects,
        optimize,
        kept.as_deref(),
    )
    .and_then(|count| {
        compiled = Instant::now();
        lld_flag = lld_flag.or_else(|| lld.flag());
        let mut command = Command::new(&clang);
        command
            .args(lld_flag)
            .arg("-g")
            .args(&objects)
            .arg(MATH_LIBRARY)
            .arg(&runtime)
            .args(SYSTEM_LIBRARIES)
            .arg(DROP_UNUSED)
            .arg("-o")
            .arg(output)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let linking = command
            .spawn()
            .map_err(|error| format!("{clang}: {error}"))?;
        // The link only reads the object files, so the cache takes this build's machine code
        // while it runs: hashing every module's text again costs about 20 ms on the 4,706-line
        // program, which the link's 55 ms hides.
        let kept = kept.map_or(Ok(()), |kept| {
            kept.tools = Some((clang.clone(), lld_flag));
            keep(kept, modules, &objects, count)
        });
        let linked = linking
            .wait_with_output()
            .map_err(|error| format!("{clang}: {error}"))
            .and_then(|finished| clang_failure(&clang, &finished));
        linked.and(kept)
    })
    .and_then(|()| {
        if cfg!(target_os = "macos") {
            dsymutil(output)
        } else {
            Ok(())
        }
    });
    let linked = Instant::now();
    let _ = fs::remove_dir_all(&scratch);
    result.map(|()| Linked {
        machine_code: compiled - started,
        link: linked - compiled,
    })
}

/// The search for LLVM's linker, `lld`, which links on Linux in about half the time of the
/// system's default `ld`: the 4,706-line program of roadmap 2.0 linked in 57 ms instead of
/// 122 ms (roadmap 2.3). It is used when it is installed and never required, so a system with
/// only `clang` links as before. On macOS the system linker is already as fast, and is kept.
///
/// `clang --print-prog-name=ld.lld` prints the path of the `lld` that `clang -fuse-ld=lld` would
/// run, found beside `clang` or on the search path, or the bare name when there is none. The
/// question costs a `clang` start, about 20 ms, so it runs while the object files are compiled
/// and its answer is read only when the link starts.
struct Lld(Option<std::process::Child>);

impl Lld {
    /// Starts asking `clang` where `lld` is, on Linux; does nothing on macOS, or when the
    /// question cannot be started.
    fn look(clang: &str) -> Self {
        if cfg!(target_os = "macos") {
            return Self(None);
        }
        let child = Command::new(clang)
            .arg("--print-prog-name=ld.lld")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok();
        Self(child)
    }

    /// The flag that makes `clang` link with `lld` when the answer names one that exists, and
    /// none otherwise.
    fn flag(self) -> Option<&'static str> {
        let finished = self.0?.wait_with_output().ok()?;
        let answer = String::from_utf8_lossy(&finished.stdout);
        let path = Path::new(answer.trim());
        (finished.status.success() && path.is_absolute() && path.is_file())
            .then_some("-fuse-ld=lld")
    }
}

/// Writes each of `modules` into `scratch` and compiles it into the object file at the same
/// position in `objects`, one `clang` per module, all running at once, and waits for every one.
/// A module whose text `kept` holds gets its kept machine code written instead. Returns how
/// many modules `clang` compiled.
///
/// # Errors
///
/// Fails with a message when a module or object file cannot be written or read, a `clang`
/// cannot be started, or one fails; the first failure in module order is the one reported.
fn compile_objects(
    clang: &str,
    scratch: &Path,
    modules: &[String],
    objects: &[PathBuf],
    optimize: bool,
    kept: Option<&Objects>,
) -> Result<usize, String> {
    let written = |path: &Path, bytes: &[u8]| {
        fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
    };
    let mut running = Vec::new();
    for (index, (module, object)) in modules.iter().zip(objects).enumerate() {
        if let Some(bytes) = kept.and_then(|kept| kept.built.get(module)) {
            written(object, bytes)?;
            continue;
        }
        let source = scratch.join(format!("module{index}.ll"));
        written(&source, module.as_bytes())?;
        let child = Command::new(clang)
            .args(if optimize { OPTIMIZE } else { DEBUG })
            .args(["-g", "-Wno-override-module", "-c"])
            .arg(&source)
            .arg("-o")
            .arg(object)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("{clang}: {error}"));
        running.push(child);
    }
    let count = running.len();
    // Every started `clang` is waited for, even after one fails, so none outlives the scratch
    // directory it writes into.
    let finished: Vec<Result<(), String>> = running
        .into_iter()
        .map(|child| {
            child.and_then(|child| {
                child
                    .wait_with_output()
                    .map_err(|error| format!("{clang}: {error}"))
                    .and_then(|finished| clang_failure(clang, &finished))
            })
        })
        .collect();
    finished.into_iter().collect::<Result<(), String>>()?;
    Ok(count)
}

/// Makes `kept` hold the machine code of exactly `modules`, whose object files are at the same
/// positions in `objects`, `compiled` of which `clang` compiled this build: a module `kept`
/// already held keeps its bytes, and the others are read from their object files.
///
/// # Errors
///
/// Fails with a message when an object file cannot be read.
fn keep(
    kept: &mut Objects,
    modules: &[String],
    objects: &[PathBuf],
    compiled: usize,
) -> Result<(), String> {
    let mut built = HashMap::with_capacity(modules.len());
    for (module, object) in modules.iter().zip(objects) {
        let bytes = match kept.built.remove(module) {
            Some(bytes) => bytes,
            None => fs::read(object).map_err(|error| format!("{}: {error}", object.display()))?,
        };
        built.insert(module.clone(), bytes);
    }
    kept.built = built;
    kept.compiled = compiled;
    Ok(())
}

/// The oldest `clang` major version that accepts the LLVM text the backend emits: opaque
/// pointers became the default in LLVM 15 (decision 0020).
const MINIMUM_CLANG: u32 = 15;

/// The newest versioned name, `clang-<n>`, that [`find_clang`] tries. Names past it are not
/// looked for; plain `clang` still finds any newer release.
const NEWEST_CLANG: u32 = 30;

/// How to install a new enough `clang`, the end of every message about a missing or old one.
const INSTALL_CLANG: &str = "Install it with `xcode-select --install` on macOS, or the `clang` \
     package on Linux; where that is older than 15, such as on Ubuntu 22.04, install `clang-15` \
     or newer";

/// The `clang` to compile and link with: the first of `clang`, then `clang-30` down to
/// `clang-15`, whose version is [`MINIMUM_CLANG`] or newer.
///
/// Each name tried runs `<name> --version` once, a few milliseconds; a name that is not
/// installed costs only the failed start. A `clang` whose version cannot be read is taken as
/// new enough, so an unusual build is tried rather than refused.
///
/// # Errors
///
/// Fails with a message saying how to install `clang` when none is found, and naming the
/// version found when every `clang` installed is older than 15.
fn find_clang() -> Result<String, String> {
    let names = std::iter::once("clang".to_owned()).chain(
        (MINIMUM_CLANG..=NEWEST_CLANG)
            .rev()
            .map(|major| format!("clang-{major}")),
    );
    let mut too_old = None;
    for name in names {
        let Ok(finished) = Command::new(&name).arg("--version").output() else {
            continue;
        };
        match major_version(&String::from_utf8_lossy(&finished.stdout)) {
            Some(major) if major < MINIMUM_CLANG => {
                too_old.get_or_insert((name, major));
            }
            _ => return Ok(name),
        }
    }
    Err(match too_old {
        Some((name, major)) => format!(
            "`{name}` is version {major}, and `baryon build` needs clang {MINIMUM_CLANG} or \
             newer. {INSTALL_CLANG}"
        ),
        None => format!(
            "`clang` was not found; `baryon build` uses it to compile and link. {INSTALL_CLANG}"
        ),
    })
}

/// The major version in the first line `clang --version` prints, such as 18 from
/// `Ubuntu clang version 18.1.3 (1ubuntu1)` or 15 from `Apple clang version 15.0.0`, or `None`
/// when the line has no `clang version` followed by a number.
///
/// Apple numbers its `clang` releases its own way, but from Apple clang 15 on they accept opaque
/// pointers as LLVM 15 does, so the same minimum applies.
fn major_version(version: &str) -> Option<u32> {
    let first_line = version.lines().next()?;
    let (_, rest) = first_line.split_once("clang version ")?;
    rest.split(|character: char| !character.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

/// Nothing when the `clang` named `clang` that ended with `finished` succeeded, and otherwise
/// the message saying it failed, with what it printed to standard error.
fn clang_failure(clang: &str, finished: &Output) -> Result<(), String> {
    if finished.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{clang} could not compile the program, which is a bug in baryon:\n{}",
            String::from_utf8_lossy(&finished.stderr)
        ))
    }
}

/// Collects the line tables of the executable `output` into `output.dSYM`, on macOS.
fn dsymutil(output: &Path) -> Result<(), String> {
    match Command::new("dsymutil").arg(output).output() {
        Ok(finished) if finished.status.success() => Ok(()),
        Ok(finished) => Err(format!(
            "dsymutil could not collect the debug information, which is a bug in baryon:\n{}",
            String::from_utf8_lossy(&finished.stderr)
        )),
        Err(error) => Err(format!(
            "dsymutil: {error}; it comes with the command line tools, `xcode-select --install`"
        )),
    }
}

/// The runtime archive next to the running `baryon`.
fn runtime() -> Result<PathBuf, String> {
    let executable = env::current_exe().map_err(|error| format!("baryon: {error}"))?;
    let archive = executable.with_file_name("libruntime.a");
    if archive.is_file() {
        Ok(archive)
    } else {
        Err(format!(
            "the runtime library `{}` is missing; build it with `cargo build --package runtime`",
            archive.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn major_versions_are_read_from_each_vendor() {
        let ubuntu = "Ubuntu clang version 18.1.3 (1ubuntu1)\nTarget: x86_64-pc-linux-gnu\n";
        assert_eq!(major_version(ubuntu), Some(18));
        assert_eq!(
            major_version("Ubuntu clang version 14.0.0-1ubuntu1.1\n"),
            Some(14)
        );
        let apple = "Apple clang version 15.0.0 (clang-1500.3.9.4)\nTarget: arm64-apple-darwin23\n";
        assert_eq!(major_version(apple), Some(15));
        assert_eq!(major_version("Homebrew clang version 19.1.7\n"), Some(19));
        assert_eq!(major_version("clang version 21.0.0git\n"), Some(21));
    }

    #[test]
    fn unreadable_versions_are_none() {
        assert_eq!(major_version(""), None);
        assert_eq!(major_version("gcc (Ubuntu 13.2.0) 13.2.0\n"), None);
    }
}
