//! The kernel suite: which kernels exist, the problem size each runs at, and how closely the two
//! languages must agree.
//!
//! This table is the one place a kernel is registered. Adding a kernel means adding its two source
//! files under `bench/kernels/<name>/` and one entry to [`KERNELS`].

use std::fmt;
use std::str::FromStr;

/// How big a problem every kernel solves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Size {
    /// Tiny problems that finish in milliseconds. CI runs these to prove every kernel builds and
    /// both languages agree; the times mean nothing.
    Smoke,
    /// Problems big enough to time: roughly 0.3 to 3 seconds each on a current laptop. Recorded
    /// baselines use this size.
    Reference,
}

impl Size {
    /// The size as written on the command line and in baseline files.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Smoke => "smoke",
            Self::Reference => "reference",
        }
    }
}

impl FromStr for Size {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "smoke" => Ok(Self::Smoke),
            "reference" => Ok(Self::Reference),
            _ => Err(format!(
                "unknown size `{text}`; expected `smoke` or `reference`"
            )),
        }
    }
}

impl fmt::Display for Size {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// One physics kernel, written once in C++ and once in Fortran.
///
/// Every kernel runs on one thread except those marked [`threaded`](Kernel::threaded), the
/// parallel baselines that Baryon's `prange` loops are compared against.
#[derive(Debug)]
pub(crate) struct Kernel {
    /// Directory name under `bench/kernels/`, and the stem of both source files.
    pub(crate) name: &'static str,
    /// What the kernel computes, in a few words.
    pub(crate) summary: &'static str,
    /// What each command-line argument means, in order.
    pub(crate) arguments: &'static [&'static str],
    /// Arguments for [`Size::Smoke`].
    pub(crate) smoke: &'static [u64],
    /// Arguments for [`Size::Reference`].
    pub(crate) reference: &'static [u64],
    /// The largest relative difference allowed between the C++ and Fortran checksums.
    ///
    /// Most kernels agree to the last bit on one machine; the tolerance absorbs fused
    /// multiply-add and math-library differences between compilers. Molecular dynamics is
    /// chaotic, so rounding differences grow; its checksum is the total energy, which the
    /// integrator conserves to about 1e-5 whatever the trajectory.
    pub(crate) tolerance: f64,
    /// Whether the kernel runs on several threads, its last argument saying how many. A threaded
    /// kernel is built with each language's thread flags
    /// ([`Language::thread_flags`](crate::toolchain::Language::thread_flags)).
    pub(crate) threaded: bool,
}

impl Kernel {
    /// The arguments for a problem of the given size.
    pub(crate) fn parameters(&self, size: Size) -> &'static [u64] {
        match size {
            Size::Smoke => self.smoke,
            Size::Reference => self.reference,
        }
    }
}

/// Tolerance for kernels whose result does not depend on chaotic dynamics.
const DETERMINISTIC: f64 = 1e-9;

/// Every kernel in the suite, in the order they run and are reported.
pub(crate) const KERNELS: &[Kernel] = &[
    Kernel {
        name: "nbody",
        summary: "All-pairs gravity, leapfrog",
        arguments: &["bodies", "steps"],
        smoke: &[256, 5],
        reference: &[4096, 20],
        tolerance: DETERMINISTIC,
        threaded: false,
    },
    Kernel {
        name: "lennard_jones",
        summary: "Lennard-Jones liquid, cell list, velocity Verlet",
        arguments: &["cells per side", "steps"],
        smoke: &[5, 10],
        reference: &[10, 200],
        tolerance: 1e-5,
        threaded: false,
    },
    Kernel {
        name: "lennard_jones_half",
        summary: "The same liquid on threads, half neighbour list",
        arguments: &["cells per side", "steps", "threads"],
        smoke: &[5, 10, 2],
        reference: &[10, 200, 4],
        tolerance: 1e-5,
        threaded: true,
    },
    Kernel {
        name: "heat_stencil",
        summary: "3D heat equation, 7-point Jacobi stencil",
        arguments: &["points per side", "steps"],
        smoke: &[24, 5],
        reference: &[200, 100],
        tolerance: DETERMINISTIC,
        threaded: false,
    },
    Kernel {
        name: "conjugate_gradient",
        summary: "Conjugate gradient, 3D Poisson matrix in CSR",
        arguments: &["points per side", "iterations"],
        smoke: &[16, 20],
        reference: &[96, 100],
        tolerance: DETERMINISTIC,
        threaded: false,
    },
    Kernel {
        name: "runge_kutta",
        summary: "RK4 on an ensemble of stiff Van der Pol oscillators",
        arguments: &["oscillators", "steps"],
        smoke: &[64, 200],
        reference: &[8192, 20000],
        tolerance: DETERMINISTIC,
        threaded: false,
    },
    Kernel {
        name: "monte_carlo",
        summary: "Monte Carlo integral of exp(-|x|^2) over a 6D cube",
        arguments: &["samples"],
        smoke: &[100_000],
        reference: &[50_000_000],
        tolerance: DETERMINISTIC,
        threaded: false,
    },
    Kernel {
        name: "matmul",
        summary: "Dense matrix multiply, plain loops",
        arguments: &["n"],
        smoke: &[64],
        reference: &[1024],
        tolerance: DETERMINISTIC,
        threaded: false,
    },
    Kernel {
        name: "fft",
        summary: "Radix-2 complex FFT",
        arguments: &["log2 of points", "repetitions"],
        smoke: &[10, 2],
        reference: &[20, 10],
        tolerance: DETERMINISTIC,
        threaded: false,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::Path;

    #[test]
    fn names_are_unique() {
        let names: HashSet<&str> = KERNELS.iter().map(|kernel| kernel.name).collect();
        assert_eq!(names.len(), KERNELS.len());
    }

    #[test]
    fn every_size_gives_every_argument() {
        for kernel in KERNELS {
            assert_eq!(
                kernel.smoke.len(),
                kernel.arguments.len(),
                "{}",
                kernel.name
            );
            assert_eq!(
                kernel.reference.len(),
                kernel.arguments.len(),
                "{}",
                kernel.name
            );
        }
    }

    #[test]
    fn threaded_kernels_take_their_thread_count_last() {
        for kernel in KERNELS.iter().filter(|kernel| kernel.threaded) {
            assert_eq!(kernel.arguments.last(), Some(&"threads"), "{}", kernel.name);
        }
    }

    #[test]
    fn every_kernel_has_both_sources() {
        let kernels = Path::new(env!("CARGO_MANIFEST_DIR")).join("kernels");
        for kernel in KERNELS {
            for extension in ["cpp", "f90"] {
                let source = kernels
                    .join(kernel.name)
                    .join(format!("{}.{extension}", kernel.name));
                assert!(source.is_file(), "missing {}", source.display());
            }
        }
    }

    #[test]
    fn sizes_round_trip_through_their_names() {
        for size in [Size::Smoke, Size::Reference] {
            assert_eq!(size.name().parse::<Size>(), Ok(size));
        }
        assert!("huge".parse::<Size>().is_err());
    }
}
