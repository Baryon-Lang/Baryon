//! Parallel code generation (roadmap 2.6): a program with enough code is written as several
//! LLVM modules, one `clang` each, and the results do not change.

use super::{BARYON, baryon_build, build_runtime, package, stderr};
use driver::{Profile, Session};
use std::fmt::Write as _;
use std::process::Command;

/// How many copies of the summing function [`program`] writes: about 11 KB of LLVM IR each, so
/// 120 make about 1.3 MB, enough for several modules.
const COPIES: usize = 120;

/// A program of [`COPIES`] functions, each adding `x[i] * x[i] + x[i] / 3.0` over an array in
/// index order and then its own number, and a `main` that prints each one's result.
fn program() -> String {
    let mut text = String::new();
    for copy in 0..COPIES {
        let _ = write!(
            text,
            "def total_{copy}(x: Array[f64, 1]) -> f64:\n    \
             s = 0.0\n    \
             for i in range(len(x)):\n        \
             s += x[i] * x[i] + x[i] / 3.0\n    \
             return s + {copy}.0\n\n\n"
        );
    }
    text.push_str(
        "def main():\n    x = Array.zeros((10,))\n    for i in range(10):\n        \
         x[i] = 0.1 * i\n",
    );
    for copy in 0..COPIES {
        let _ = writeln!(text, "    print(total_{copy}(x))");
    }
    text
}

/// What the program prints: the same additions, in the same order, in Rust. Each result has a
/// fraction, so Rust's shortest form is the one Python's `print` writes.
fn expected() -> String {
    let x: Vec<f64> = (0..10).map(|i| 0.1 * f64::from(i)).collect();
    let mut sum = 0.0;
    for value in &x {
        sum += value * value + value / 3.0;
    }
    let mut text = String::new();
    for copy in 0..COPIES {
        let _ = writeln!(
            text,
            "{}",
            sum + f64::from(u32::try_from(copy).unwrap_or(0))
        );
    }
    text
}

/// Split four ways, the program becomes several modules, and each function is defined in
/// exactly one of them.
#[test]
fn each_function_is_defined_in_one_module() {
    let mut session = Session::new();
    session.add_file("main.byn", program()).unwrap();
    let modules = session.compile_parts(None, Profile::Debug, 4).unwrap();
    assert!(modules.len() > 1, "{} module(s)", modules.len());
    for copy in 0..COPIES {
        let definition = format!("define double @main.total_{copy}(");
        let defining = modules
            .iter()
            .filter(|module| module.contains(&definition))
            .count();
        assert_eq!(defining, 1, "total_{copy}");
    }
}

/// Built split, in a debug and a release build, the program prints what one module would: the
/// float sums keep their order across modules, so every bit is the same.
#[test]
fn split_programs_print_the_same_bits() {
    build_runtime();
    let directory = package("parts", &[("main.byn", &program())]).unwrap();
    let (built, executable) = baryon_build(&directory).unwrap();
    assert!(built.status.success(), "{}", stderr(&built));
    let ran = Command::new(&executable).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&ran.stdout), expected());

    let release = directory.join("release");
    let built = Command::new(BARYON)
        .arg("build")
        .arg("--release")
        .arg(directory.join("package"))
        .arg(&release)
        .output()
        .unwrap();
    assert!(built.status.success(), "{}", stderr(&built));
    let ran = Command::new(&release).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&ran.stdout), expected());
}
