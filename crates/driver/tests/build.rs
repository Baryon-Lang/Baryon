//! End-to-end tests of `npl build` (roadmap 1.11.3): packages compiled by the `npl` executable,
//! linked with the real runtime, and run. They check that the backend calls the runtime's
//! functions by the names and signatures it exports (decision 0021), which no other test can,
//! since the backend may not depend on the runtime.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Once;

/// The `npl` executable cargo built for these tests.
const NPL: &str = env!("CARGO_BIN_EXE_npl");

/// Builds the runtime archive next to `npl`, where `npl build` looks for it, once per run.
/// `cargo test` builds the runtime only as a Rust library, not as the static library.
fn build_runtime() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let built = Command::new(env!("CARGO"))
            .args(["build", "--quiet", "--package", "runtime"])
            .status()
            .is_ok_and(|status| status.success());
        assert!(built, "building the runtime failed");
        assert!(
            Path::new(NPL).with_file_name("libruntime.a").is_file(),
            "the runtime archive is not next to `npl`"
        );
    });
}

/// A new directory holding the package `files`, each a path and its text, for the test `name`.
fn package(name: &str, files: &[(&str, &str)]) -> io::Result<PathBuf> {
    let directory =
        std::env::temp_dir().join(format!("npl-build-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    for (path, text) in files {
        let path = directory.join("package").join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, text)?;
    }
    Ok(directory)
}

/// Runs `npl build` on the package in `directory`; its output, and the executable's path.
fn npl_build(directory: &Path) -> io::Result<(Output, PathBuf)> {
    build_runtime();
    let program = directory.join("program");
    let output = Command::new(NPL)
        .arg("build")
        .arg(directory.join("package"))
        .arg(&program)
        .output()?;
    Ok((output, program))
}

/// The text of standard error.
fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A program across two modules compiles, calls between them, and runs to the end; a failed
/// `assert` prints its source and location through the runtime and exits with status 101.
#[test]
fn programs_build_and_run() {
    let directory = package(
        "run",
        &[
            (
                "numbers/fibonacci.npl",
                "def fibonacci(n: i64) -> i64:\n\
                 \x20   a = 0\n\
                 \x20   b = 1\n\
                 \x20   for i in range(n):\n\
                 \x20       c = a + b\n\
                 \x20       a = b\n\
                 \x20       b = c\n\
                 \x20   return a\n",
            ),
            (
                "main.npl",
                "from numbers.fibonacci import fibonacci\n\
                 \n\
                 \n\
                 def main():\n\
                 \x20   assert fibonacci(10) == 55\n\
                 \x20   assert fibonacci(20) == 6765, \"fibonacci(20)\"\n",
            ),
        ],
    )
    .unwrap();
    let (output, program) = npl_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(ran.status.code(), Some(0), "{}", stderr(&ran));

    let failing = package(
        "panic",
        &[(
            "main.npl",
            "def main():\n    n = 7\n    assert n * 6 == 41, \"arithmetic\"\n",
        )],
    )
    .unwrap();
    let (output, program) = npl_build(&failing).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(ran.status.code(), Some(101));
    assert_eq!(
        stderr(&ran),
        "panic: assertion failed: n * 6 == 41, \"arithmetic\"\n  --> main.npl:3:5\n"
    );
}

/// A package without `main.npl` builds nothing and says why, and so does one whose `main` is in
/// another module: a program starts only at `main.npl`'s.
#[test]
fn programs_start_at_main_npl() {
    let directory = package(
        "no-main",
        &[("main.npl", "def f() -> i64:\n    return 1\n")],
    )
    .unwrap();
    let (output, program) = npl_build(&directory).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        "error: the package has no `main` function to run\n \
         = note: add `def main():` to `main.npl` at the package's root\n\n"
    );
    assert!(!program.exists());

    let elsewhere = package("main-elsewhere", &[("app.npl", "def main():\n    pass\n")]).unwrap();
    let (output, _) = npl_build(&elsewhere).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        "error: only `main.npl` may define `main`\n \
         --> app.npl:1:5\n  \
         |\n\
         1 | def main():\n  \
         |     ^^^^ only `main.npl` may define `main`\n  \
         = note: a program starts at the `main` function of the `main.npl` at the package's \
         root; rename this function\n\n\
         error: the package has no `main` function to run\n \
         = note: add `def main():` to `main.npl` at the package's root\n\n"
    );
}
