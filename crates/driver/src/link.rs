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

use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

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

/// The C math library, linked before the runtime. LLVM calls `floor` and `trunc` for those
/// operations on CPUs without an instruction for them, and the runtime, a Rust static library,
/// carries portable versions under the same names. Naming the system library first makes the
/// linker take its versions, which pick the fastest instruction the CPU has: the Lennard-Jones
/// benchmark kernel ran in 6.1 s with them and 8.0 s with the runtime's (roadmap 3.3). Both are
/// exact, so results do not change.
const MATH_LIBRARY: &str = "-lm";

/// Compiles the LLVM module `module` at `-O2` and links it with the runtime into the executable
/// `output`, keeping the module's line tables (`-g`) so debuggers and panic backtraces show
/// `.byn` lines.
///
/// The module is compiled to an object file in a scratch directory, then linked. On Linux the
/// line tables are copied into the executable. On macOS the linker leaves them in the object file
/// and records where it was, so `dsymutil` collects them into `output.dSYM` beside the
/// executable before the scratch directory, and the object file with it, is removed. `clang`
/// runs `dsymutil` itself only for source files it compiles, not for an LLVM module.
pub(crate) fn link(module: &str, output: &Path) -> Result<(), String> {
    let runtime = runtime()?;
    let clang = find_clang()?;
    let scratch = env::temp_dir().join(format!("baryon-build-{}", std::process::id()));
    fs::create_dir_all(&scratch).map_err(|error| format!("{}: {error}", scratch.display()))?;
    let source = scratch.join("module.ll");
    let object = scratch.join("module.o");
    let result = fs::write(&source, module)
        .map_err(|error| format!("{}: {error}", source.display()))
        .and_then(|()| {
            run_clang(&clang, |command| {
                command
                    .args(OPTIMIZE)
                    .args(["-g", "-Wno-override-module", "-c"])
                    .arg(&source)
                    .arg("-o")
                    .arg(&object);
            })
        })
        .and_then(|()| {
            run_clang(&clang, |command| {
                command
                    .arg("-g")
                    .arg(&object)
                    .arg(MATH_LIBRARY)
                    .arg(&runtime)
                    .args(SYSTEM_LIBRARIES)
                    .arg(DROP_UNUSED)
                    .arg("-o")
                    .arg(output);
            })
        })
        .and_then(|()| {
            if cfg!(target_os = "macos") {
                dsymutil(output)
            } else {
                Ok(())
            }
        });
    let _ = fs::remove_dir_all(&scratch);
    result
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

/// Runs the `clang` named `clang` with the arguments `arguments` adds.
fn run_clang(clang: &str, arguments: impl FnOnce(&mut Command)) -> Result<(), String> {
    let mut command = Command::new(clang);
    arguments(&mut command);
    match command.output() {
        Ok(finished) if finished.status.success() => Ok(()),
        Ok(finished) => Err(format!(
            "{clang} could not compile the program, which is a bug in baryon:\n{}",
            String::from_utf8_lossy(&finished.stderr)
        )),
        Err(error) => Err(format!("{clang}: {error}")),
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
