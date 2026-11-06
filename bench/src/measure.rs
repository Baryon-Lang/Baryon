//! Running a kernel, reading what it reports, and summarizing repeated runs.
//!
//! Every kernel prints exactly two lines on success:
//!
//! ```text
//! checksum -4.58485193057788398e-01
//! seconds 7.06986239999991994e-01
//! ```
//!
//! The checksum proves the computation happened and lets the two languages be compared. The time
//! covers only the kernel's timed region: setup, input generation and the checksum itself are
//! excluded, so the number measures the computation the language must make fast.

use std::path::Path;
use std::process::Command;

/// What one run of a kernel reported.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Report {
    /// A number summarizing the result, used to check correctness.
    pub(crate) checksum: f64,
    /// Wall-clock seconds spent in the timed region.
    pub(crate) seconds: f64,
}

/// The result of running one kernel in one language several times.
#[derive(Debug)]
pub(crate) struct Measurement {
    /// The checksum every run reported; [`summarize`] rejects runs that disagree.
    pub(crate) checksum: f64,
    /// Seconds for every run, in the order they ran.
    pub(crate) runs: Vec<f64>,
}

impl Measurement {
    /// The fastest run: the best estimate of the kernel's cost on a quiet machine.
    pub(crate) fn minimum(&self) -> f64 {
        self.runs.iter().copied().fold(f64::INFINITY, f64::min)
    }

    /// The middle run, or the mean of the two middle runs: how long a typical run takes.
    pub(crate) fn median(&self) -> f64 {
        let mut sorted = self.runs.clone();
        sorted.sort_by(f64::total_cmp);
        let middle = sorted.len() / 2;
        match sorted.len() {
            0 => f64::NAN,
            length if length % 2 == 1 => sorted[middle],
            _ => f64::midpoint(sorted[middle - 1], sorted[middle]),
        }
    }
}

/// Runs `executable` with `arguments` the given number of times.
///
/// # Errors
///
/// Fails if a run cannot start, exits unsuccessfully, prints output that [`parse`] rejects, or
/// reports a checksum different from the first run's (see [`summarize`]).
pub(crate) fn measure(
    executable: &Path,
    arguments: &[u64],
    repetitions: usize,
) -> Result<Measurement, String> {
    let reports = (0..repetitions)
        .map(|_| run(executable, arguments))
        .collect::<Result<Vec<_>, _>>()?;
    summarize(&reports).map_err(|error| format!("{}: {error}", executable.display()))
}

/// Combines the reports of repeated runs of one executable into a [`Measurement`].
///
/// The same binary on the same input must compute the same result, so every checksum has to match
/// the first one exactly. A mismatch means the kernel is nondeterministic, which would make its
/// checksum meaningless as a correctness check.
///
/// # Errors
///
/// Fails if there are no reports or any checksum differs from the first.
fn summarize(reports: &[Report]) -> Result<Measurement, String> {
    let Some(first) = reports.first() else {
        return Err(String::from("no runs to summarize"));
    };
    if let Some((index, report)) = reports
        .iter()
        .enumerate()
        .find(|(_, report)| report.checksum.to_bits() != first.checksum.to_bits())
    {
        return Err(format!(
            "run {} reported checksum {:e}, but run 1 reported {:e}; the kernel is not deterministic",
            index + 1,
            report.checksum,
            first.checksum
        ));
    }
    Ok(Measurement {
        checksum: first.checksum,
        runs: reports.iter().map(|report| report.seconds).collect(),
    })
}

/// Runs `executable` once and reads its report.
fn run(executable: &Path, arguments: &[u64]) -> Result<Report, String> {
    let output = Command::new(executable)
        .args(arguments.iter().map(u64::to_string))
        .output()
        .map_err(|error| format!("cannot run {}: {error}", executable.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} failed ({}):\n{}",
            executable.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    parse(&String::from_utf8_lossy(&output.stdout))
        .map_err(|error| format!("{}: {error}", executable.display()))
}

/// Reads a kernel's report: one `checksum <number>` line and one `seconds <number>` line, in any
/// order, with any spacing and either exponent letter (Fortran prints `1.5E+000`).
///
/// # Errors
///
/// Fails if either line is missing or its value is not a finite number.
pub(crate) fn parse(output: &str) -> Result<Report, String> {
    let mut checksum = None;
    let mut seconds = None;
    for line in output.lines() {
        let mut words = line.split_whitespace();
        let (Some(key), Some(value)) = (words.next(), words.next()) else {
            continue;
        };
        let slot = match key {
            "checksum" => &mut checksum,
            "seconds" => &mut seconds,
            _ => continue,
        };
        let number: f64 = value
            .parse()
            .map_err(|_| format!("`{key}` value `{value}` is not a number"))?;
        if !number.is_finite() {
            return Err(format!("`{key}` value `{value}` is not finite"));
        }
        *slot = Some(number);
    }
    match (checksum, seconds) {
        (Some(checksum), Some(seconds)) => Ok(Report { checksum, seconds }),
        _ => Err(String::from(
            "expected a `checksum` line and a `seconds` line",
        )),
    }
}

/// The relative difference between two checksums: `|a - b| / max(|a|, |b|)`, or zero when both
/// are zero.
pub(crate) fn relative_difference(a: f64, b: f64) -> f64 {
    let scale = a.abs().max(b.abs());
    if scale == 0.0 {
        0.0
    } else {
        (a - b).abs() / scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cpp_output() {
        let report = parse("checksum -4.25000000000000000e-01\nseconds 1.50000000000000000e+00\n");
        assert_eq!(
            report,
            Ok(Report {
                checksum: -0.425,
                seconds: 1.5,
            })
        );
    }

    #[test]
    fn parses_fortran_output() {
        let report =
            parse("checksum  2.37380000000000000E+004\nseconds  1.75000000000000000E-005\n");
        assert_eq!(
            report,
            Ok(Report {
                checksum: 23_738.0,
                seconds: 1.75e-5,
            })
        );
    }

    #[test]
    fn summarizes_runs_that_agree() {
        let reports = [
            Report {
                checksum: 2.5,
                seconds: 0.3,
            },
            Report {
                checksum: 2.5,
                seconds: 0.2,
            },
        ];
        let measurement =
            summarize(&reports).map(|measurement| (measurement.checksum, measurement.runs));
        assert_eq!(measurement, Ok((2.5, vec![0.3, 0.2])));
    }

    #[test]
    fn rejects_runs_whose_checksums_differ() {
        let reports = [
            Report {
                checksum: 2.5,
                seconds: 0.3,
            },
            Report {
                checksum: 2.500_000_000_000_001,
                seconds: 0.2,
            },
        ];
        assert!(summarize(&reports).is_err());
        assert!(summarize(&[]).is_err());
    }

    #[test]
    fn rejects_missing_lines() {
        assert!(parse("checksum 1.0\n").is_err());
        assert!(parse("").is_err());
    }

    #[test]
    fn rejects_values_that_are_not_finite_numbers() {
        assert!(parse("checksum NaN\nseconds 1.0\n").is_err());
        assert!(parse("checksum 1.0\nseconds fast\n").is_err());
    }

    #[test]
    fn summarizes_runs() {
        let odd = Measurement {
            checksum: 0.0,
            runs: vec![3.0, 1.0, 2.0],
        };
        assert!((odd.minimum() - 1.0).abs() < f64::EPSILON);
        assert!((odd.median() - 2.0).abs() < f64::EPSILON);
        let even = Measurement {
            checksum: 0.0,
            runs: vec![4.0, 1.0, 3.0, 2.0],
        };
        assert!((even.median() - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn relative_difference_is_scaled_by_the_larger_value() {
        assert!((relative_difference(100.0, 99.0) - 0.01).abs() < 1e-15);
        assert!((relative_difference(-2.0, -2.0)).abs() < f64::EPSILON);
        assert!((relative_difference(0.0, 0.0)).abs() < f64::EPSILON);
    }
}
