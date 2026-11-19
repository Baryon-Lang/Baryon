//! The sums and means of groups by one key of whole numbers (roadmap 3.13.21), built and
//! run. The library reads such groups in one pass over the rows when the keys lie close
//! together, and keeps each sum exactly as two floats while it can; this pins that every
//! way through gives the bits the slower way gave.

use super::{baryon_build, package, stderr};
use std::process::Command;

/// The program of `sums_by_one_key_keep_their_bits`.
const PROGRAM: &str = r#"
from math import inf, nan


class Row(Struct):
    key: i64
    far: i64
    flag: bool
    name: str
    maybe: i64 | None
    value: f64
    wide: f64
    whole: i64


def table(rows: int) -> DataFrame[Row]:
    key: list[int] = []
    far: list[int] = []
    flag: list[bool] = []
    name: list[str] = []
    maybe: list[i64 | None] = []
    value: list[f64] = []
    wide: list[f64] = []
    whole: list[int] = []
    state = 12345
    for i in range(rows):
        state = (state * 1103515245 + 12345) % 2147483648
        key.append(state % 7 - 3)
        far.append((state % 5) * 1000000007 - 2000000000)
        flag.append(state % 3 == 0)
        name.append(["b", "a", "c"][state % 3].copy())
        if state % 11 == 0:
            maybe.append(None)
        else:
            maybe.append(state % 4)
        x = f64(state) / 2147483648.0
        if state % 13 == 0:
            x = nan
        value.append(x)
        scale = 2.0 ** f64(state % 200 - 100)
        if state % 17 == 0:
            scale = -scale
        if state % 101 == 0 and state % 7 == 2:
            scale = inf
        wide.append(x * scale)
        whole.append(state % 1000)
    return DataFrame({
        "key": Array(key), "far": Array(far), "flag": Array(flag), "name": Array(name),
        "maybe": Array(maybe), "value": Array(value), "wide": Array(wide),
        "whole": Array(whole),
    })


def show(values: Array[f64, 1]):
    print(" ".join([f"{value}" for value in values]))


def main():
    for rows in [0, 3, 60]:
        data = table(rows)
        key_out = data.groupby("key").agg(
            a=("value", "sum"), b=("value", "mean"), c=("wide", "sum"), d=("whole", "mean"),
        )
        print(rows, "key", len(key_out))
        show(key_out.a)
        show(key_out.b)
        show(key_out.c)
        show(key_out.d)
        far_out = data.groupby("far").agg(
            a=("value", "sum"), b=("value", "mean"), c=("wide", "sum"), d=("whole", "mean"),
        )
        print(rows, "far", len(far_out))
        show(far_out.a)
        show(far_out.b)
        show(far_out.c)
        show(far_out.d)
        maybe_out = data.groupby("maybe").agg(
            a=("value", "sum"), b=("value", "mean"), c=("wide", "sum"), d=("whole", "mean"),
        )
        print(rows, "maybe", len(maybe_out))
        show(maybe_out.a)
        show(maybe_out.b)
        show(maybe_out.c)
        show(maybe_out.d)
        pair = data.groupby(["key", "flag"]).agg(a=("value", "sum"), b=("wide", "mean"))
        show(pair.a)
        show(pair.b)
"#;

/// What `PROGRAM` prints.
const EXPECTED: &str = "\
         0 key 0\n\
         \n\
         \n\
         \n\
         \n\
         0 far 0\n\
         \n\
         \n\
         \n\
         \n\
         0 maybe 0\n\
         \n\
         \n\
         \n\
         \n\
         \n\
         \n\
         3 key 3\n\
         0.6551540484651923 0.6749606337398291 0.30481432331725955\n\
         0.6551540484651923 0.6749606337398291 0.30481432331725955\n\
         3.3076826606813584e-29 11323960.34375 1.1515559551456143e+22\n\
         606.0 924.0 775.0\n\
         3 far 3\n\
         0.30481432331725955 0.6551540484651923 0.6749606337398291\n\
         0.30481432331725955 0.6551540484651923 0.6749606337398291\n\
         1.1515559551456143e+22 3.3076826606813584e-29 11323960.34375\n\
         775.0 606.0 924.0\n\
         3 maybe 3\n\
         0.6749606337398291 0.6551540484651923 0.30481432331725955\n\
         0.6749606337398291 0.6551540484651923 0.30481432331725955\n\
         11323960.34375 3.3076826606813584e-29 1.1515559551456143e+22\n\
         924.0 606.0 775.0\n\
         0.6551540484651923 0.6749606337398291 0.30481432331725955\n\
         3.3076826606813584e-29 11323960.34375 1.1515559551456143e+22\n\
         60 key 7\n\
         4.722396272234619 3.8898760522715747 4.201649050228298 3.6157437707297504 5.645277447532862 2.911849529016763 4.0295963026583195\n\
         0.47223962722346186 0.3889876052271575 0.6002355786040425 0.5165348243899643 0.5645277447532863 0.5823699058033526 0.5036995378322899\n\
         2.9834540036587505e+27 3.212819820118621e+26 5.3604440085626984e+16 3.663854459932987e+27 2.740677737270225e+28 6.516093806879738e+23 1.1515675377017289e+22\n\
         477.4 551.0 518.5714285714286 456.375 431.0 649.8 396.0\n\
         60 far 5\n\
         5.468830508179963 6.750337611883879 7.95669313473627 3.8164314310997725 5.024095738772303\n\
         0.4971664098345421 0.6750337611883879 0.46804077263154525 0.4240479367888636 0.5024095738772303\n\
         3.887722315201339e+25 1.5195871435026174e+21 3.0351861153289386e+28 3.6663858142668177e+27 3.189052331201608e+26\n\
         462.72727272727275 589.6363636363636 376.4117647058824 612.0 470.8181818181818\n\
         60 maybe 4\n\
         8.409549692645669 6.910862768534571 5.366416286677122 5.7928774347528815\n\
         0.6468884378958207 0.5316048283488132 0.4472013572230935 0.41377695962520583\n\
         2.9839494316713553e+27 3.135132195925652e+28 1.561262026926718e+23 2.3893280728056125e+24\n\
         534.2857142857143 599.1538461538462 414.57142857142856 397.57142857142856\n\
         4.612827705685049 0.10956856654956937 2.5490718809887767 1.340804171282798 2.584213215392083 1.6174358348362148 2.4134759199805558 1.2022678507491946 3.650023929774761 1.9952535177581012 1.5831960090436041 1.3286535199731588 2.8996096989139915 1.129986603744328\n\
         3.314948892954167e+26 3.50619412958622 2.9709361185172284e+23 1.5945261655852415e+26 1.0283372921015522e+16 1093787740274688.0 2264792.068750472 1.8319272299664935e+27 3.915253910386036e+27 40443698.132679164 458491203365.27594 3.258046903432992e+23 1.9192792295028815e+21 99.86354219913483\n\
         ";

/// A frame of 0, 3 and 60 rows grouped by a key in a small range (one pass), by a key
/// whose values lie thousands of millions apart (each group gathered), by a key that may
/// be `None` (rows left out, so gathered), and by two keys. `value` holds numbers of like
/// size and some NaN, whose sums stay exact as two floats; `wide` holds numbers from
/// 2**-100 to 2**100 of both signs, whose sums do not and go through `math.fsum`. Every
/// digit of every sum and mean is what the compiler printed before roadmap 3.13.21.
#[test]
fn sums_by_one_key_keep_their_bits() {
    let directory = package("frame_group_sums", &[("main.byn", PROGRAM)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&executable).output().unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(String::from_utf8_lossy(&ran.stdout), EXPECTED);
}
