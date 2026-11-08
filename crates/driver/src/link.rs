//! `baryon build`: hands the compiled module to `clang`, which optimizes it, compiles it and links
//! it with the runtime into an executable (decisions 0020 and 0021).
//!
//! The runtime is the static library `libruntime.a`, found next to the `baryon` executable, where
//! `cargo build` puts both. The module is written to a scratch directory and removed after.

use std::io;
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
    let scratch = env::temp_dir().join(format!("baryon-build-{}", std::process::id()));
    fs::create_dir_all(&scratch).map_err(|error| format!("{}: {error}", scratch.display()))?;
    let source = scratch.join("module.ll");
    let object = scratch.join("module.o");
    let result = fs::write(&source, module)
        .map_err(|error| format!("{}: {error}", source.display()))
        .and_then(|()| {
            clang(|command| {
                command
                    .args(["-O2", "-g", "-Wno-override-module", "-c"])
                    .arg(&source)
                    .arg("-o")
                    .arg(&object);
            })
        })
        .and_then(|()| {
            clang(|command| {
                command
                    .arg("-g")
                    .arg(&object)
                    .arg(&runtime)
                    .args(SYSTEM_LIBRARIES)
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

/// Runs `clang` with the arguments `arguments` adds.
fn clang(arguments: impl FnOnce(&mut Command)) -> Result<(), String> {
    let mut command = Command::new("clang");
    arguments(&mut command);
    match command.output() {
        Ok(finished) if finished.status.success() => Ok(()),
        Ok(finished) => Err(format!(
            "clang could not compile the program, which is a bug in baryon:\n{}",
            String::from_utf8_lossy(&finished.stderr)
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Err(
            "`clang` was not found; `baryon build` uses it to compile and link. Install it with \
             `xcode-select --install` on macOS, or the `clang` package on Linux"
                .to_owned(),
        ),
        Err(error) => Err(format!("clang: {error}")),
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
