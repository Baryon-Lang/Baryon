//! The statistics methods (roadmap 3.16), built and run: `mean`, `var` and `std`, of the whole
//! array and along an axis, print what `NumPy` 2.5 prints for the same program, bit for bit.

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

/// The program of [`statistics_along_an_axis_print_what_numpy_prints`].
const ALONG_AN_AXIS: &str = "\
Mat = Fixed[f64, (3, 4)]


def show(v: Array[f64, 1]):
    for i in range(v.shape[0]):
        print(v[i])


def main():
    a = Array.zeros((5, 300))
    for i in range(5):
        for j in range(300):
            a[i, j] = 1.0 / (i + j + 1.0) + 0.1 * j - 0.37 * i
    show(a.mean(axis=1))
    show(a.mean(axis=0)[0:4])
    show(a.var(axis=1))
    show(a.var(0)[0:4])
    show(a.std(axis=-1, ddof=1))
    show(a.std(ddof=1, axis=0)[0:4])
    show(a[:, 1:200:3].var(axis=1))
    show(a.var(axis=0, ddof=5)[0:2])
    c = Array.zeros((3, 4, 5))
    for i in range(3):
        for j in range(4):
            for k in range(5):
                c[i, j, k] = 0.1 * i + 0.3 * j * j - 0.7 * k + 1.0 / (i + j + k + 1.0)
    spread = c.var(axis=1)
    middle = c.mean(axis=2)
    for i in range(3):
        for k in range(5):
            print(spread[i, k])
        for j in range(4):
            print(middle[i, j])
    m = Mat.zeros()
    for i in range(3):
        for j in range(4):
            m[i, j] = 0.5 * i - 0.25 * j * j
    deviations = m.std(axis=0)
    for j in range(4):
        print(deviations[j])
    rows = m.mean(axis=1)
    for i in range(3):
        print(rows[i])
    print(a.var(axis=0).shape)
    v = Array([1.0, 2.0, 4.0])
    print(v.var(axis=0))
";

/// What `NumPy` 2.5 prints for [`ALONG_AN_AXIS`].
const ALONG_AN_AXIS_EXPECTED: &str = "14.970942212934332\n\
14.597619953798121\n\
14.225964324659047\n\
13.854864214648048\n\
13.484041846226994\n\
-0.2833333333333333\n\
-0.35\n\
-0.32142857142857134\n\
-0.26309523809523805\n\
74.57383950200368\n\
74.66711032821281\n\
74.71330533251974\n\
74.74354942564726\n\
74.76580324109527\n\
0.6317777777777775\n\
0.40637777777777784\n\
0.3471002267573695\n\
0.3212240362811791\n\
8.65004337194089\n\
8.655451063071771\n\
8.658128123785149\n\
8.659880360133158\n\
8.661169441521418\n\
0.888663165784552\n\
0.7127217004008102\n\
0.6586921006408927\n\
0.6336639845781626\n\
33.309009642649805\n\
33.37605206555165\n\
33.415595987467256\n\
33.4428311009106\n\
33.463248226460735\n\
inf\n\
inf\n\
0.7058854166666665\n\
0.9117187499999997\n\
0.9901909722222221\n\
1.028223852040816\n\
1.0496282419217686\n\
-0.9433333333333331\n\
-0.8099999999999999\n\
0.018571428571428638\n\
1.476904761904762\n\
0.91171875\n\
0.9901909722222217\n\
1.0282238520408162\n\
1.0496282419217686\n\
1.0629020514455783\n\
-1.0099999999999998\n\
-0.7814285714285714\n\
0.07690476190476207\n\
1.549126984126984\n\
0.990190972222222\n\
1.0282238520408165\n\
1.0496282419217686\n\
1.0629020514455783\n\
1.0717148821334088\n\
-0.9814285714285713\n\
-0.723095238095238\n\
0.14912698412698414\n\
1.6291269841269842\n\
0.408248290463863\n\
0.408248290463863\n\
0.408248290463863\n\
0.408248290463863\n\
-0.875\n\
-0.375\n\
0.125\n\
(300,)\n\
1.5555555555555554\n\
";

/// Means, variances and standard deviations along the last axis, whose sums are pairwise per
/// row, and the first, whose sums add one element at a time, as `NumPy`'s do; an axis written
/// by position, by keyword, or as -1, before or after `ddof`; a strided view; `ddof` equal to
/// the length; a rank-3 array along its middle axis; a `Fixed` matrix; and a vector, whose axis
/// reduces it to one number. Every value is printed in full.
#[test]
fn statistics_along_an_axis_print_what_numpy_prints() -> io::Result<()> {
    let directory = package("statistics-axes", &[("main.byn", ALONG_AN_AXIS)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output()?;
    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), ALONG_AN_AXIS_EXPECTED);
    Ok(())
}
