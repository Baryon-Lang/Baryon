//! The Baryon twins of the benchmark kernels in `bench/kernels` (roadmap 3.3), built and run at
//! their smoke sizes: each prints the checksum its Fortran version prints, to the last bit, so
//! the compiler's optimizations, the vectorized in-order sums among them, change no result.
//! The threaded `lennard_jones_half` adds its forces in another order than its Fortran twin, so
//! it is checked to the kernel's tolerance instead.

use super::{baryon_build, stderr};
use std::io;
use std::path::Path;
use std::process::Command;

/// Each kernel, its smoke-size arguments, and the checksum `gfortran -O3` prints for them.
const KERNELS: [(&str, &[&str], f64); 8] = [
    ("nbody", &["256", "5"], -0.447_599_460_366_519),
    ("lennard_jones", &["5", "10"], -5.691_759_543_731_037),
    ("heat_stencil", &["24", "5"], 3_016.989_946_168_612_3),
    ("conjugate_gradient", &["16", "20"], 503.192_450_972_452_83),
    ("runge_kutta", &["64", "200"], 74.894_718_131_763_71),
    ("monte_carlo", &["100000"], 0.173_058_467_441_508_2),
    ("matmul", &["64"], -7.348_364_102_722_999),
    ("fft", &["10", "2"], 23_738.074_330_195_806),
];

/// Builds the kernel `name` from `bench/kernels/<name>/baryon` and returns the checksum it
/// prints for `arguments`.
fn checksum(name: &str, arguments: &[&str]) -> io::Result<f64> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../bench/kernels")
        .join(name)
        .join("baryon");
    let directory = std::env::temp_dir().join(format!(
        "baryon-build-test-kernel-{name}-{}",
        std::process::id()
    ));
    let package = directory.join("package");
    std::fs::create_dir_all(&package)?;
    std::fs::copy(source.join("main.byn"), package.join("main.byn"))?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{name}: {}", stderr(&output));
    let run = Command::new(&executable).args(arguments).output()?;
    assert!(run.status.success(), "{name}: {}", stderr(&run));
    let text = String::from_utf8_lossy(&run.stdout).into_owned();
    std::fs::remove_dir_all(&directory)?;
    let value = text
        .strip_prefix("checksum ")
        .and_then(|rest| rest.trim().parse().ok());
    Ok(value.unwrap_or(f64::NAN))
}

/// Every kernel prints its Fortran twin's checksum exactly.
#[test]
fn kernels_match_fortran_to_the_bit() {
    for (name, arguments, expected) in KERNELS {
        let found = checksum(name, arguments).unwrap();
        assert_eq!(found.to_bits(), expected.to_bits(), "{name}: {found:e}");
    }
}

/// The half-list Lennard-Jones twin, whose `prange(atoms, accumulate=(fx, fy, fz))` sums the
/// forces in the runtime's fixed tree rather than in OpenMP thread order: it agrees with
/// `gfortran -O3` on two threads within the kernel's tolerance, `1e-5`, and gives the same bits
/// on every run. Its third argument, the thread count, is read and ignored.
#[test]
fn the_threaded_half_list_kernel_agrees_with_fortran() {
    let fortran = -5.691_759_543_731_019_5;
    let first = checksum("lennard_jones_half", &["5", "10", "2"]).unwrap();
    assert!((first - fortran).abs() <= 1e-5 * fortran.abs(), "{first:e}");
    let again = checksum("lennard_jones_half", &["5", "10", "2"]).unwrap();
    assert_eq!(again.to_bits(), first.to_bits());
}

/// A sum whose terms cancel, `1e16 + 1 - 1e16 + 1 ...`, gives a different total if any two
/// additions are reordered: in index order every `+ 1` is lost against `1e16` and found again
/// only after the cancellation. The loop vectorizer may vectorize it but must keep that order.
#[test]
fn a_float_sum_loop_adds_in_index_order() {
    let program = "\
def total(values: Array[f64, 1]) -> f64:
    sum = 0.0
    for i in range(len(values)):
        sum += values[i] * 1.0
    return sum


def main():
    values = Array.zeros((4000,))
    for i in range(4000):
        if i % 4 == 0:
            values[i] = 1.0e16
        elif i % 4 == 2:
            values[i] = -1.0e16
        else:
            values[i] = 1.0
    print(total(values))
";
    let directory = super::package("ordered-sum", &[("main.byn", program)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&run.stdout), "1.0\n");
}
