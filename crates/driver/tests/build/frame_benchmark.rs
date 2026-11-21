//! The `DataFrame` benchmark in `bench/dataframe` (roadmap 3.13.10 and 3.13.20), built and run
//! on a table of 1000 rows: every operation prints the checksum pandas 3.0 prints for the same
//! table, so a time the benchmark reports is the time of the right answer.

use super::{baryon_build, stderr};
use std::process::Command;

/// The benchmark program.
const PROGRAM: &str = include_str!("../../../../bench/dataframe/baryon/main.byn");

/// Each operation and what `python3 bench/dataframe/frames.py pandas 1000 1` prints as its
/// checksum. `setup` and `missing_setup` only build the table: the benchmark runs them to take
/// that time off the others. `write` comes before `read`, which reads the file it makes.
const CHECKSUMS: [(&str, &str); 11] = [
    ("setup", "502.959445"),
    ("filter", "504 254.718177"),
    ("assign", "1502.511327"),
    ("groupby", "1000 502.959445 496.592437"),
    ("sort", "0.000000 0.439043"),
    ("merge", "1000 1791.840672"),
    ("missing_setup", "1000"),
    ("missing", "454 229.315889 0.505525"),
    ("counts", "1000 1000 1000"),
    ("write", "1000"),
    ("read", "1000 502.959445 499500"),
];

/// The first three lines of the file `write` makes, as pandas's `to_csv(path, index=False)`
/// writes them.
const WRITTEN: &str = "key,value,other\n0,0.0,0.0\n919,0.4277977166068502,0.040503688562705566\n";

/// Every operation of the benchmark gives pandas's answer.
#[test]
fn the_frame_benchmark_prints_what_pandas_prints() {
    let directory = super::package("frame-benchmark", &[("main.byn", PROGRAM)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let file = directory.join("table.csv");
    for (operation, expected) in CHECKSUMS {
        let run = Command::new(&executable)
            .args([operation, "1000"])
            .arg(&file)
            .output()
            .unwrap();
        assert!(run.status.success(), "{operation}: {}", stderr(&run));
        let printed = String::from_utf8_lossy(&run.stdout);
        assert_eq!(printed.trim_end(), expected, "{operation}");
    }
    let written = std::fs::read_to_string(&file).unwrap();
    assert!(written.starts_with(WRITTEN), "{written:.200}");
}
