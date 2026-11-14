//! End-to-end tests of `baryon build` (roadmap 1.11.3): packages compiled by the `baryon` executable,
//! linked with the real runtime, and run. They check that the backend calls the runtime's
//! functions by the names and signatures it exports (decision 0021), which no other test can,
//! since the backend may not depend on the runtime.
//!
//! This file holds the helpers every test shares and the tests of the core language; each
//! submodule tests one kind of value.

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Mutex, Once, PoisonError};

mod array_files;
mod array_methods;
mod axes;
mod broadcast;
mod builtins;
mod bz2;
mod clang;
mod comparisons;
mod complex;
mod defaults;
mod dicts;
mod errors;
mod examples;
mod fft;
mod files;
mod fits;
mod float_generics;
mod frame_columns;
mod frame_distinct;
mod frame_groups;
mod frame_joins;
mod frame_rows;
mod frame_sort;
mod frame_stack;
mod frames;
mod gzip_open;
mod fusion;
mod generic_methods;
mod guide;
mod h5by;
mod hoisting;
mod kernels;
mod linalg;
mod list_slices;
mod lzma;
mod managers;
mod math;
mod modules;
mod netcdf;
mod netcdf_writing;
mod npy;
mod one_pass;
mod optional_arguments;
mod overflow;
mod ownership;
mod parallel;
mod parts;
mod paths;
mod plotting;
mod plotting_contour;
mod plotting_figures;
mod plotting_images;
mod plotting_log;
mod plotting_pdf;
mod plotting_png;
mod print;
mod process;
mod reductions;
mod shapes;
mod slices;
mod sorting;
mod special;
mod spectral;
mod statements;
mod statistics;
mod stats;
mod streams;
mod structs;
mod subprocess;
mod temporaries;
mod testing;
mod text;
mod text_arrays;
mod text_columns;
mod text_keys;
mod text_methods;
mod text_rows;
mod text_tables;
mod timings;
mod traits;
mod unions;
mod user_iterators;
mod vector;
mod watching;
mod zlib;
mod zstd;

/// The `baryon` executable cargo built for these tests.
const BARYON: &str = env!("CARGO_BIN_EXE_baryon");

/// Builds the runtime archive next to `baryon`, where `baryon build` looks for it, once per run.
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
            Path::new(BARYON).with_file_name("libruntime.a").is_file(),
            "the runtime archive is not next to `baryon`"
        );
    });
}

/// A test's scratch directory, removed when it goes out of scope unless the test is panicking,
/// so a failed test leaves its package and executable behind to inspect.
///
/// Each test builds an executable of several megabytes, so without the removal every run of
/// the suite left about 100 MB in the system's temporary directory, and a machine with a small
/// disk filled up after a few dozen runs.
struct Scratch(
    /// The directory.
    PathBuf,
);

impl std::ops::Deref for Scratch {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

/// A new directory holding the package `files`, each a path and its text, for the test `name`.
/// It is removed when the test passes; see [`Scratch`].
///
/// The tests run in parallel and each name is one directory, which this function first empties,
/// so two tests sharing a name would delete each other's files. A name used twice panics, so
/// that mistake fails every run instead of one run in a few.
fn package(name: &str, files: &[(&str, &str)]) -> io::Result<Scratch> {
    static NAMES: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
    let fresh = NAMES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(name.to_owned());
    assert!(fresh, "two tests use the package name `{name}`");
    let directory =
        std::env::temp_dir().join(format!("baryon-build-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    for (path, text) in files {
        let path = directory.join("package").join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, text)?;
    }
    Ok(Scratch(directory))
}

/// Runs `baryon build` on the package in `directory`; its output, and the executable's path.
fn baryon_build(directory: &Path) -> io::Result<(Output, PathBuf)> {
    build_runtime();
    let program = directory.join("program");
    let output = Command::new(BARYON)
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

/// The text of standard error before a panic's backtrace: its message and location. The test
/// `panics_print_a_backtrace_of_baryon_frames` checks the backtrace itself.
fn panic_report(output: &Output) -> String {
    let text = stderr(output);
    match text.split_once(BACKTRACE) {
        Some((report, _)) => report.to_owned(),
        None => text,
    }
}

/// The line a panic's backtrace starts with.
const BACKTRACE: &str = "backtrace, most recent call last:\n";

/// A program across two modules compiles, calls between them, and runs to the end; a failed
/// `assert` prints its source and location through the runtime and exits with status 101.
#[test]
fn programs_build_and_run() {
    let directory = package(
        "fibonacci",
        &[
            (
                "numbers/fibonacci.byn",
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
                "main.byn",
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
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(ran.status.code(), Some(0), "{}", stderr(&ran));

    let failing = package(
        "panic",
        &[(
            "main.byn",
            "def main():\n    n = 7\n    assert n * 6 == 41, \"arithmetic\"\n",
        )],
    )
    .unwrap();
    let (output, program) = baryon_build(&failing).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(ran.status.code(), Some(101));
    assert_eq!(
        panic_report(&ran),
        "panic: assertion failed: n * 6 == 41, \"arithmetic\"\n  --> main.byn:3:5\n"
    );
}

/// A test's scratch directory, with the executable built in it, is gone once the test is done
/// with it, so repeated runs of the suite do not fill the disk.
#[test]
fn scratch_directories_are_removed() {
    let directory = package("scratch", &[("main.byn", "def main():\n    pass\n")]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(program.is_file());
    let path = directory.to_path_buf();
    drop(directory);
    assert!(!path.exists(), "{} was left behind", path.display());
}

/// A package without `main.byn` builds nothing and says why, and so does one whose `main` is in
/// another module: a program starts only at `main.byn`'s.
#[test]
fn programs_start_at_main_byn() {
    let directory = package(
        "no-main",
        &[("main.byn", "def f() -> i64:\n    return 1\n")],
    )
    .unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        "error: the package has no `main` function to run\n \
         = note: add `def main():` to `main.byn` at the package's root\n\n"
    );
    assert!(!program.exists());

    let elsewhere = package("main-elsewhere", &[("app.byn", "def main():\n    pass\n")]).unwrap();
    let (output, _) = baryon_build(&elsewhere).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        "error: only `main.byn` may define `main`\n \
         --> app.byn:1:5\n  \
         |\n\
         1 | def main():\n  \
         |     ^^^^ only `main.byn` may define `main`\n  \
         = note: a program starts at the `main` function of the `main.byn` at the package's \
         root; rename this function\n\n\
         error: the package has no `main` function to run\n \
         = note: add `def main():` to `main.byn` at the package's root\n\n"
    );
}

/// Arrays end to end: allocated with run-time lengths, filled, combined with whole-array
/// arithmetic, summed, and freed; a `Fixed` array built and indexed from both ends; and an index
/// one past the end stopped by the bounds check, which names the index, dimension and length.
#[test]
fn array_programs_build_and_run() {
    let program = "\
Vec3 = Fixed[f64, (3,)]


def axpy(a: Array[f64, 1], b: Array[f64, 1]) -> Array[f64, 1]:
    return -a + b * 2


def total(a: Array[f64, 1]) -> f64:
    s = 0.0
    for i in range(len(a)):
        s += a[i]
    return s


def main():
    n = 1000
    a = Array.zeros(n)
    b = Array.zeros(n)
    for i in range(n):
        a[i] = 1.0
        b[i] = f64(i)
    assert total(axpy(a, b)) == 998000.0
    v = Vec3(1.0, 2.0, 3.0)
    assert v[0] + v[-1] == 4.0
    assert a[n] == 0.0
";
    let directory = package("arrays", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(ran.status.code(), Some(101));
    assert_eq!(
        panic_report(&ran),
        "panic: index 1000 is out of range for dimension 0 of length 1000\n  \
         --> main.byn:25:14\n"
    );
}

/// `print` end to end: each argument as Python writes it, separated by spaces, one line per
/// call. A string literal prints as it is; an `f32` prints its own shortest digits.
#[test]
fn programs_print_as_python_does() {
    let program = "\
def mean(a: f64, b: f64) -> f64:
    return (a + b) / 2.0


def main():
    print(\"mean:\", mean(1.0, 2.0))
    print(1e16, 0.1 + 0.2, 1.5e-05, -0.0)
    small: i8 = -5
    large: u32 = 4294967295
    print(small, large, 3 > 2, f32(0.1))
    print()
    print('done')
";
    let directory = package("printing", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert!(ran.status.success(), "{}", stderr(&ran));
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "mean: 1.5\n1e+16 0.30000000000000004 1.5e-05 -0.0\n-5 4294967295 True 0.1\n\ndone\n"
    );
}

/// `baryon run` builds the package, runs it with the terminal's output, and exits with the
/// program's own status: the lines it printed reach standard output, then the panic of its
/// failed `assert` exits with 101.
#[test]
fn run_builds_and_runs_with_the_programs_status() {
    let program = "\
def main():
    print(\"started\")
    assert 1 > 2
";
    let directory = package("run", &[("main.byn", program)]).unwrap();
    build_runtime();
    let ran = Command::new(BARYON)
        .arg("run")
        .arg(directory.join("package"))
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&ran.stdout), "started\n");
    assert_eq!(ran.status.code(), Some(101));
    assert_eq!(
        panic_report(&ran),
        "panic: assertion failed: 1 > 2\n  --> main.byn:3:5\n"
    );
}

/// Optional values end to end: absent and present values passed on, narrowed by `is None` and
/// `is not None`, given a default, and holding an array, which is freed only when present.
/// Each argument is a variable already of the parameter's `T | None` type.
#[test]
fn optional_values_build_and_run() {
    let program = "\
class Particle(Struct):
    mass: f64


def scaled(distance: f64 | None, particle: Particle | None) -> f64:
    if distance is None:
        distance = 1.0
    assert particle is not None, 'needs a particle'
    return distance * particle.mass


def nearest(distance: f64 | None, limit: f64) -> f64:
    if distance is not None and distance < limit:
        return distance
    return limit


def size(values: Array[f64, 1] | None) -> i64:
    if values is not None:
        return len(values)
    return -1


def main():
    p: Particle | None = Particle(2.5)
    absent: f64 | None = None
    four: f64 | None = 4.0
    print(scaled(absent, p), scaled(four, p))
    near: f64 | None = 1.5
    far: f64 | None = 7.0
    print(nearest(absent, 3.0), nearest(near, 3.0), nearest(far, 3.0))
    none: Array[f64, 1] | None = None
    ones: Array[f64, 1] | None = Array.ones(4)
    print(size(none), size(ones))
    nobody: Particle | None = None
    print(scaled(four, nobody))
";
    let directory = package("optionals", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "2.5 10.0\n3.0 1.5 3.0\n-1 4\n"
    );
    assert_eq!(
        panic_report(&ran),
        "panic: assertion failed: particle is not None, 'needs a particle'\n  \
         --> main.byn:8:5\n"
    );
}

/// A generic function compiles once per type it is called at (roadmap 1.9.5): `twice` and the
/// `square` it calls each at `f64` and at `i64`, with the literal `2` an `f64` in one and an
/// `i64` in the other.
#[test]
fn generic_functions_build_at_each_type() {
    let program = "\
def square[T: Number](x: T) -> T:
    return x * x


def twice[T: Number](x: T) -> T:
    return square(x) * 2 - x


def main():
    print(twice(3.0), twice(4))
";
    let directory = package("generics", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert!(ran.status.success(), "{}", stderr(&ran));
    assert_eq!(String::from_utf8_lossy(&ran.stdout), "15.0 28\n");
}
