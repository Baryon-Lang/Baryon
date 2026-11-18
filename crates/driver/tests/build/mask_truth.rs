//! `any()` and `all()` of arrays of `bool`s, and `sum()`, `any()` and `all()` of a mask that
//! may hold `None` (roadmap 3.13.13), built and run. A plain array answers as `NumPy` does,
//! over the whole array whatever its rank; a mask with missing values skips each `None`, as
//! pandas does.

use super::{baryon_build, package, panic_report, stderr};
use std::io;
use std::process::Command;

/// Builds `program` as the package `name`, runs it, and returns its exit code, standard output
/// and panic report.
fn run(name: &str, program: &str) -> io::Result<(Option<i32>, String, String)> {
    let directory = package(name, &[("main.byn", program)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output()?;
    Ok((
        run.status.code(),
        String::from_utf8_lossy(&run.stdout).into_owned(),
        panic_report(&run),
    ))
}

/// `any()` is whether an element is `True` and `all()` whether every one is, for an `Array`
/// of any rank, a `Fixed` array and a comparison. An empty array gives `False` and `True`.
/// Each is a `bool`, so it is a condition.
#[test]
fn plain_masks_answer_any_and_all() -> io::Result<()> {
    let program = r#"
Vec3 = Fixed[f64, (3,)]


def main():
    mask = Array([True, False, True])
    none = Array([False, False])
    every = Array([True, True])
    print(mask.any(), mask.all())
    print(none.any(), none.all())
    print(every.any(), every.all())
    empty: Array[bool, 1] = Array([])
    print(empty.any(), empty.all())
    grid = Array.zeros((2, 3))
    grid[1, 2] = 4.0
    print((grid >= 0.0).all(), (grid > 3.5).any())
    print((grid > 3.5).all(), (grid > 9.0).any())
    v = Vec3(1.0, 2.0, 3.0)
    print((v > 0.5).all(), (v > 2.5).any(), (v > 2.5).all())
    if (grid > 3.5).any() and not (grid > 3.5).all():
        print("some")
"#;
    let (code, output, report) = run("mask_truth_plain", program)?;
    assert_eq!(code, Some(0), "{report}");
    assert_eq!(
        output,
        "True False\n\
         False False\n\
         True True\n\
         False True\n\
         True True\n\
         False False\n\
         True True False\n\
         some\n"
    );
    Ok(())
}

/// A mask that may hold `None` counts its `True` elements with `sum()`, and `any()` and
/// `all()` answer for the elements that are there. With none there, they give 0, `False`
/// and `True`, which is what pandas gives a `boolean` column.
#[test]
fn masks_with_gaps_skip_none() -> io::Result<()> {
    let program = r"
def main():
    gaps: Array[bool | None, 1] = Array([True, None, False, True])
    yes: Array[bool | None, 1] = Array([True, None])
    gone: Array[bool | None, 1] = Array([None, None])
    empty: Array[bool | None, 1] = Array([])
    print(gaps.sum(), gaps.any(), gaps.all())
    print(yes.sum(), yes.any(), yes.all())
    print(gone.sum(), gone.any(), gone.all())
    print(empty.sum(), empty.any(), empty.all())
    bids: Array[f64 | None, 1] = Array([1.5, None, 2.5, 4.0])
    print((bids > 2.0).sum(), (bids > 2.0).any(), (bids > 1.0).all())
    print(((bids > 2.0) & (bids < 3.0)).sum(), (~(bids > 1.0)).any())
    if (bids > 3.0).any():
        print(len(bids) - (bids > 3.0).sum())
";
    let (code, output, report) = run("mask_truth_gaps", program)?;
    assert_eq!(code, Some(0), "{report}");
    assert_eq!(
        output,
        "2 True False\n\
         1 True True\n\
         0 False True\n\
         0 False True\n\
         2 True True\n\
         1 False\n\
         3\n"
    );
    Ok(())
}
