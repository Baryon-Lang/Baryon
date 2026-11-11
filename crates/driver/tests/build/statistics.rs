//! The statistics methods (roadmap 3.16), built and run: `mean`, `var` and `std` print what
//! `NumPy` 2.5 prints for the same program, bit for bit.

use super::{baryon_build, package, stderr};
use std::io;
use std::process::Command;

/// Means, variances and standard deviations of floats over several leaves of the pairwise tree,
/// with `ddof` of 0, 1, `n` and past `n`; of a whole-array expression, a matrix and a strided
/// view; the mean of integers and of an empty array; `f32` arrays, whose results stay `f32`;
/// and a `Fixed` vector. Every line is what `NumPy` prints, each float in full so a different
/// order of additions would show.
#[test]
fn statistics_print_what_numpy_prints() -> io::Result<()> {
    let program = "\
Vec3 = Fixed[f64, (3,)]


def main():
    n = 1000
    a = Array.zeros(n)
    for i in range(n):
        a[i] = 1.0 / (i + 1.0) + 0.1 * i
    print(a.mean())
    print(a.var())
    print(a.std())
    print(a.var(ddof=1))
    print(a.std(ddof=1))
    print(a.var(ddof=n))
    print(a.var(ddof=n + 1))
    print((a * 3.0 - 1.0).var())
    b = Array.zeros((7, 13))
    for i in range(7):
        for j in range(13):
            b[i, j] = 0.3 * i - 1.0 / (j + 2.0)
    print(b.mean())
    print(b.var())
    print(b[:, 1:12:3].std())
    m = Array.zeros(5, dtype=i64)
    for i in range(5):
        m[i] = i * i
    print(m.mean())
    print(Array.zeros(0).mean())
    f = Array.zeros(300, dtype=f32)
    for i in range(300):
        f[i] = f32(0.1 * i)
    print(f.mean())
    print(f.std())
    v = Vec3(1.0, 2.5, 4.25)
    print(v.mean())
    print(v.var(ddof=1))
";
    let directory = package("statistics", &[("main.byn", program)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output()?;
    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "49.957485470860554\n\
832.7847922691517\n\
28.85801088552625\n\
833.6184106798315\n\
28.872450721749125\n\
inf\n\
inf\n\
7495.063130422365\n\
0.726802897956744\n\
0.3743101360746289\n\
0.6077860932805103\n\
6.0\n\
nan\n\
14.95\n\
8.660206\n\
2.5833333333333335\n\
2.6458333333333335\n\
"
    );
    Ok(())
}
