//! Baseline files: a JSON record of one run of the suite on one machine.
//!
//! A baseline is written to `bench/baselines/<os>-<architecture>.json`, for example
//! `macos-aarch64.json`, so each platform keeps its own numbers and recording on one machine never
//! overwrites another's. The format is described in `bench/README.md`; [`SCHEMA`] changes whenever
//! it does.

use crate::measure::Measurement;
use crate::suite::{Kernel, Size};
use crate::toolchain::{Compiler, Language};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Version of the baseline file format.
const SCHEMA: u32 = 1;

/// One kernel's measurements in both languages.
pub(crate) struct KernelResult<'a> {
    /// The kernel measured.
    pub(crate) kernel: &'a Kernel,
    /// One measurement per language, in [`Language::ALL`] order.
    pub(crate) measurements: Vec<(Language, Measurement)>,
}

/// Everything a baseline records besides the measurements.
pub(crate) struct Context<'a> {
    /// The problem size the suite ran at.
    pub(crate) size: Size,
    /// Runs per kernel and language.
    pub(crate) repetitions: usize,
    /// The compilers used, each with its `--version` line.
    pub(crate) compilers: &'a [(Compiler, String)],
}

/// The baseline path for the machine this runs on, inside `directory`.
pub(crate) fn path(directory: &Path) -> PathBuf {
    directory.join(format!(
        "{}-{}.json",
        std::env::consts::OS,
        std::env::consts::ARCH
    ))
}

/// Builds the baseline document. `repository` is used to look up the current commit.
pub(crate) fn document(
    context: &Context<'_>,
    results: &[KernelResult<'_>],
    repository: &Path,
) -> Value {
    let compilers: serde_json::Map<String, Value> = context
        .compilers
        .iter()
        .map(|(compiler, version)| {
            (
                compiler.language.key().to_owned(),
                json!({
                    "command": compiler.command,
                    "version": version,
                    "flags": compiler.flags,
                }),
            )
        })
        .collect();
    let kernels: Vec<Value> = results
        .iter()
        .map(|result| {
            let languages: serde_json::Map<String, Value> = result
                .measurements
                .iter()
                .map(|(language, measurement)| {
                    (
                        language.key().to_owned(),
                        json!({
                            "checksum": measurement.checksum,
                            "seconds": {
                                "minimum": measurement.minimum(),
                                "median": measurement.median(),
                                "runs": measurement.runs,
                            },
                        }),
                    )
                })
                .collect();
            json!({
                "name": result.kernel.name,
                "summary": result.kernel.summary,
                "arguments": result.kernel.arguments,
                "parameters": result.kernel.parameters(context.size),
                "results": languages,
            })
        })
        .collect();
    json!({
        "schema": SCHEMA,
        "recorded": utc_timestamp(SystemTime::now()),
        "commit": commit(repository),
        "size": context.size.name(),
        "repetitions": context.repetitions,
        "machine": machine(),
        "compilers": compilers,
        "kernels": kernels,
    })
}

/// The operating system, architecture, processor model and logical core count.
fn machine() -> Value {
    json!({
        "os": std::env::consts::OS,
        "architecture": std::env::consts::ARCH,
        "processor": processor().unwrap_or_else(|| String::from("unknown")),
        "logical_cores": std::thread::available_parallelism().map_or(0, usize::from),
    })
}

/// The processor's marketing name, such as `Apple M3 Pro`, if the system reports one.
fn processor() -> Option<String> {
    if cfg!(target_os = "macos") {
        let output = Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
            .ok()?;
        let name = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (!name.is_empty()).then_some(name)
    } else {
        let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").ok()?;
        cpuinfo
            .lines()
            .find(|line| line.starts_with("model name"))
            .and_then(|line| line.split_once(':'))
            .map(|(_, name)| name.trim().to_owned())
    }
}

/// The commit checked out in `repository`, or `unknown` outside a Git checkout.
fn commit(repository: &Path) -> String {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repository)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map_or_else(
            || String::from("unknown"),
            |output| String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        )
}

/// Formats a time as an ISO 8601 UTC timestamp such as `2026-11-06T05:30:00Z`.
fn utc_timestamp(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let days = seconds / 86_400;
    let remainder = seconds % 86_400;
    let (year, month, day) = civil_date(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        remainder / 3600,
        remainder % 3600 / 60,
        remainder % 60
    )
}

/// The Gregorian date `days` after 1970-01-01, using Howard Hinnant's `civil_from_days`
/// algorithm restricted to dates after the epoch.
fn civil_date(days: u64) -> (u64, u64, u64) {
    let shifted = days + 719_468;
    let era = shifted / 146_097;
    let day_of_era = shifted % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn formats_utc_timestamps() {
        let at = |seconds| UNIX_EPOCH + Duration::from_secs(seconds);
        assert_eq!(utc_timestamp(at(0)), "1970-01-01T00:00:00Z");
        assert_eq!(utc_timestamp(at(951_782_400)), "2000-02-29T00:00:00Z");
        assert_eq!(utc_timestamp(at(1_793_943_000)), "2026-11-06T05:30:00Z");
    }

    #[test]
    fn baseline_path_names_the_platform() {
        let path = path(Path::new("baselines"));
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        assert_eq!(
            name,
            Some(format!(
                "{}-{}.json",
                std::env::consts::OS,
                std::env::consts::ARCH
            ))
        );
    }
}
