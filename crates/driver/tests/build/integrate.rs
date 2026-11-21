//! The fixed rules of the `integrate` module (roadmap 3.19.3), built and run: `trapezoid`,
//! `simpson`, `cumulative_trapezoid` and `romb` give `SciPy` 1.18's bits for the same samples,
//! evenly and unevenly spaced, at every length that takes a different path through `simpson`.
//!
//! The samples are built from additions, multiplications and divisions alone, which round the
//! same everywhere, so both languages integrate the same numbers. The expected lines were
//! printed by this script with `SciPy` 1.18.1 and `NumPy` 2:
//!
//! ```text
//! import numpy as np, scipy.integrate as I
//! for n in [0, 1, 2, 3, 4, 5, 10, 11, 1000, 1001]:
//!     i = np.arange(float(n)); x = i * i / 1000.0 + 0.1 * i; y = x * x - 2.0 / (1.0 + x)
//!     line = f"{n} {float(I.trapezoid(y))!r} {float(I.trapezoid(y, x))!r} {float(I.trapezoid(y, dx=0.37))!r}"
//!     if n > 0:
//!         line += f" {float(I.simpson(y))!r} {float(I.simpson(y, x=x))!r} {float(I.simpson(y, dx=0.37))!r}"
//!         r = I.cumulative_trapezoid(y, x); s = I.cumulative_trapezoid(y, dx=0.37, initial=0)
//!         line += f" {len(r)} {float(r.sum())!r} {len(s)} {float(s[n - 1])!r}"
//!     print(line)
//! for k in [0, 1, 2, 5, 10]:
//!     c = (1 << k) + 1; i = np.arange(float(c)); s = 1.0 / (1.0 + (i / 37.0) * (i / 37.0)) - 0.25 * i / c
//!     print(k, repr(float(I.romb(s))), repr(float(I.romb(s, 0.01))))
//! ```

use super::examples::prints;
use super::{baryon_build, package, panic_report, stderr};
use std::process::Command;

/// Every fixed rule at lengths 0 to 5, 10, 11, 1000 and 1001, with `x`, with `dx` and with
/// neither; `cumulative_trapezoid` with and without `initial=0.0`; and `romb` at 2, 3, 5, 33 and
/// 1025 samples. Each number is `SciPy`'s, to the last bit.
#[test]
fn fixed_rules_give_scipys_bits() {
    let program = "\
from integrate import cumulative_trapezoid, romb, simpson, trapezoid


def main():
    for n in [0, 1, 2, 3, 4, 5, 10, 11, 1000, 1001]:
        y: Array[f64, 1] = Array.zeros(n)
        x: Array[f64, 1] = Array.zeros(n)
        for i in range(n):
            x[i] = f64(i) * f64(i) / 1000.0 + 0.1 * f64(i)
            y[i] = x[i] * x[i] - 2.0 / (1.0 + x[i])
        line = f\"{n} {trapezoid(y)} {trapezoid(y, x)} {trapezoid(y, dx=0.37)}\"
        if n > 0:
            line += f\" {simpson(y)} {simpson(y, x)} {simpson(y, dx=0.37)}\"
            running = cumulative_trapezoid(y, x)
            started = cumulative_trapezoid(y, dx=0.37, initial=0.0)
            line += f\" {len(running)} {running.sum()} {len(started)} {started[n - 1]}\"
        print(line)
    for k in [0, 1, 2, 5, 10]:
        count = (1 << k) + 1
        samples: Array[f64, 1] = Array.zeros(count)
        for i in range(count):
            samples[i] = 1.0 / (1.0 + (f64(i) / 37.0) * (f64(i) / 37.0)) - 0.25 * f64(i) / f64(count)
        print(k, romb(samples), romb(samples, 0.01))
";
    prints("integrate-fixed", program, "\
0 0.0 0.0 0.0
1 0.0 0.0 0.0 0.0 0.0 0.0 0 0.0 1 0.0
2 -1.903164713442325 -0.19221963605767484 -0.7041709439736603 -1.903164713442325 -0.19221963605767484 -0.7041709439736603 1 -0.19221963605767484 2 -0.7041709439736603
3 -3.6160862109378065 -0.36865055029970945 -1.3379518980469884 -3.614943758548304 -0.3684705365611813 -1.3375291906628726 2 -0.5608701863573843 3 -1.3379518980469884
4 -5.142044435403491 -0.5288761638686064 -1.902556441099292 -5.140926552055898 -0.5286667089285529 -1.9021428242606824 3 -1.0897463502259908 4 -1.902556441099292
5 -6.477932565081557 -0.6718161937441594 -2.396835049080176 -6.477776581860842 -0.6716774037109232 -2.396777335288512 4 -1.76156254397015 5 -2.396835049080176
10 -9.884057560376212 -1.0519603882671955 -3.657101297339198 -9.89164840623718 -1.0525071475596013 -3.659909910307757 9 -6.515975637809593 10 -3.657101297339199
11 -9.778863094365779 -1.039442246811954 -3.618179344915338 -9.78879033665505 -1.0402112440421543 -3.6218524245623684 10 -7.555417884621547 11 -3.618179344915339
1000 252125973.6434751 441132109.583899 93286610.24808578 252125589.7619073 441131705.9610395 93286468.21190572 999 66147500366.96671 1000 93286610.24808577
1001 253333666.9445573 443667057.8228706 93733456.7694862 253333281.96132475 443666652.658715 93733314.32569017 1000 66591167424.78959 1001 93733456.76948619
0 0.9371350364963504 0.009371350364963504
1 1.8313889878310057 0.018313889878310056
2 3.5845252186408456 0.03584525218640845
5 22.50446293698346 0.2250446293698346
10 -71.09199056154306 -0.7109199056154302
")
    .unwrap();
}

/// Samples that do not fit a rule panic with a message that says what the rule needs, where
/// `SciPy` raises `ValueError`.
#[test]
fn samples_that_do_not_fit_panic() {
    let program = "\
from integrate import romb


def main():
    print(romb(Array([1.0, 2.0, 3.0, 4.0])))
";
    let directory = package("integrate-romb-panic", &[("main.byn", program)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output().unwrap();
    assert_eq!(run.status.code(), Some(101));
    assert_eq!(
        panic_report(&run),
        "panic: assertion failed: n == intervals, \"romb needs one plus a power of 2 samples\"\n  \
         --> <std>/integrate.byn:152:5\n"
    );
}
