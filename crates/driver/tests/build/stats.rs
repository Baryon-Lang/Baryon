//! The `stats` module (roadmap 3.16, decision 0042), built and run: `median`, `quantile`,
//! `percentile` and `average` print what `NumPy` 2.5 prints for the same arrays, bit for bit,
//! and a program that never calls them carries none of their code.

use super::{baryon_build, package, stderr};
use std::io;
use std::process::Command;

/// The program the test builds and runs.
const PROGRAM: &str = "\
from math import inf, nan
from stats import average, median, percentile, quantile


def main():
    for n in range(1, 10):
        a = Array.zeros(n)
        for i in range(n):
            a[i] = ((i * 7919 + n) % 13) * 0.1 - 0.35 + i * 1.0e-3
        print(median(a))
        for k in range(9):
            print(percentile(a, k * 12.5))
    n = 1001
    b = Array.zeros(n)
    w = Array.zeros(n)
    for i in range(n):
        b[i] = ((i * 7919) % 1009) * 0.37 - 50.0
        w[i] = ((i * 31) % 17) * 0.3
    print(median(b))
    print(median(b[0:1000]))
    print(percentile(b, 33.3))
    print(quantile(b[1:], 0.7))
    print(average(b, w))
    print(median(Array([1.0, nan, 2.0])))
    print(percentile(Array([3.0, nan]), 0.0))
    print(percentile(Array([1.0, inf]), 100.0))
    print(quantile(Array([-inf, 1.0, 2.0]), 0.25))
";

/// What `NumPy` 2.5 prints for the same arrays.
const EXPECTED: &str = "-0.24999999999999997\n\
-0.24999999999999997\n\
-0.24999999999999997\n\
-0.24999999999999997\n\
-0.24999999999999997\n\
-0.24999999999999997\n\
-0.24999999999999997\n\
-0.24999999999999997\n\
-0.24999999999999997\n\
-0.24999999999999997\n\
-0.04949999999999996\n\
-0.14999999999999997\n\
-0.12487499999999996\n\
-0.09974999999999996\n\
-0.07462499999999997\n\
-0.04949999999999996\n\
-0.024374999999999952\n\
0.0007500000000000423\n\
0.025875000000000044\n\
0.051000000000000045\n\
0.15100000000000002\n\
-0.04999999999999993\n\
0.00025000000000005573\n\
0.050500000000000045\n\
0.10075000000000003\n\
0.15100000000000002\n\
0.20125000000000004\n\
0.25150000000000006\n\
0.3017500000000001\n\
0.3520000000000001\n\
0.3515000000000001\n\
0.050000000000000044\n\
0.12537500000000007\n\
0.2007500000000001\n\
0.2761250000000001\n\
0.3515000000000001\n\
0.42687500000000006\n\
0.5022500000000001\n\
0.577625\n\
0.653\n\
0.3510000000000001\n\
-0.346\n\
-0.09799999999999998\n\
0.15000000000000002\n\
0.25050000000000006\n\
0.3510000000000001\n\
0.45150000000000007\n\
0.552\n\
0.6525000000000001\n\
0.7530000000000001\n\
0.3505000000000001\n\
-0.24599999999999997\n\
-0.12037499999999995\n\
0.028750000000000074\n\
0.21312500000000012\n\
0.3505000000000001\n\
0.4761250000000001\n\
0.60175\n\
0.7273750000000001\n\
0.8530000000000002\n\
0.2560000000000001\n\
-0.347\n\
-0.19624999999999998\n\
-0.045499999999999964\n\
0.10525000000000007\n\
0.2560000000000001\n\
0.3265000000000001\n\
0.45050000000000007\n\
0.6012500000000001\n\
0.7520000000000001\n\
0.4030000000000001\n\
-0.24699999999999997\n\
-0.07112499999999994\n\
0.10475000000000004\n\
0.28062500000000007\n\
0.4030000000000001\n\
0.49012500000000003\n\
0.5805\n\
0.6761250000000001\n\
0.8520000000000002\n\
0.45600000000000007\n\
-0.348\n\
-0.14699999999999996\n\
0.05400000000000005\n\
0.2550000000000001\n\
0.45600000000000007\n\
0.55\n\
0.657\n\
0.7510000000000001\n\
0.8580000000000002\n\
136.85\n\
137.035\n\
74.68999999999998\n\
211.33099999999996\n\
135.34461010605116\n\
nan\n\
nan\n\
nan\n\
-inf\n\
";

/// Medians and nine percentiles of every length from 1 to 9, so each interpolation weight and
/// both of `NumPy`'s interpolation formulas are reached; a long array of odd and even length and
/// a strided view; a weighted average; a NaN anywhere; and infinite elements, whose
/// interpolation gives infinity or NaN exactly as `NumPy`'s does. Every line is what `NumPy`
/// prints.
#[test]
fn stats_functions_print_what_numpy_prints() -> io::Result<()> {
    let directory = package("stats-functions", &[("main.byn", PROGRAM)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output()?;
    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), EXPECTED);
    Ok(())
}

/// A library function is compiled only into a program that calls it: a program that never
/// calls `median` has no `median` in its module, so the library costs it nothing.
#[test]
fn library_functions_are_compiled_only_when_called() {
    let compiled = |program: &str| {
        let mut session = driver::Session::new();
        session.add_file("main.byn", program).unwrap();
        session.compile(None, driver::Profile::Debug).unwrap()
    };
    let unused = compiled("def main():\n    print(1)\n");
    assert!(!unused.contains("<std>.stats.median"), "{unused}");
    let used = compiled(
        "from stats import median\n\n\ndef main():\n    print(median(Array([2.0, 1.0])))\n",
    );
    assert!(used.contains("@\"<std>.stats.median\""), "{used}");
}
