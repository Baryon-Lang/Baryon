//! A list or tuple written with `None` among its items (roadmap 3.13.6 and 1.9.68, decision
//! 0045 part 3), built and run: with no annotation the list takes the type `list[T | None]`,
//! wherever it is written, and a tuple takes the `T | None` its context gives.

use super::{baryon_build, package, panic_report, stderr};
use std::io;
use std::process::Command;

/// A list of numbers and `None` with no annotation is a `list[f64 | None]`: it is passed where
/// one is expected, looped over with each item checked, and grown with numbers and `None`.
/// `None` may come first, the items may be integers or text, and a list of such lists works
/// too. `Array` of such a list is an array with missing values, with no type written anywhere.
#[test]
fn a_list_with_none_needs_no_annotation() -> io::Result<()> {
    let program = r#"
def total(values: list[f64 | None]) -> f64:
    result = 0.0
    for value in values:
        if value is not None:
            result += value
    return result


def main():
    bids = [1.5, None, 2.5]
    print(total(bids), len(bids))
    bids.append(None)
    bids.append(7.0)
    print(total(bids), len(bids))
    print(total([None, 0.5]))
    for count in [None, 4, None]:
        if count is None:
            print("gap")
        else:
            print(count + 1)
    for name in ["a", None]:
        if name is not None:
            print(name)
    nested = [[1.5, None], [2.5]]
    print(len(nested), len(nested[0]), total(nested[1]))
    array = Array([1.5, None, 2.5])
    print(len(array), array.count(), array.sum(), array.isna())
"#;
    let directory = package("optional_lists", &[("main.byn", program)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output()?;
    assert_eq!(run.status.code(), Some(0), "{}", panic_report(&run));
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "4.0 3\n11.0 5\n0.5\ngap\n5\ngap\na\n2 2 2.5\n3 2 4.0 [False  True False]\n"
    );
    Ok(())
}

/// A tuple written with `None` or a plain value where its type has a `T | None` takes that
/// type (roadmap 1.9.68): `return (x, None)` from a function that returns
/// `tuple[f64, f64 | None]`, an annotated variable, and an argument, as in Python.
#[test]
fn a_tuple_takes_none_where_its_type_allows() -> io::Result<()> {
    let program = r#"
def root(x: f64) -> tuple[f64, f64 | None]:
    if x < 0.0:
        return (x, None)
    return (x, x ** 0.5)


def show(pair: tuple[i64 | None, str]):
    first = pair[0]
    if first is None:
        print("none", pair[1])
    else:
        print(first + 1, pair[1])


def main():
    print(root(-4.0), root(9.0))
    value, maybe = root(-1.0)
    print(value, maybe is None)
    t: tuple[f64, f64 | None] = (1.5, None)
    print(t, t[1] is None)
    show((None, "a"))
    show((2, "b"))
"#;
    let directory = package("optional_tuples", &[("main.byn", program)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output()?;
    assert_eq!(run.status.code(), Some(0), "{}", panic_report(&run));
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "(-4.0, None) (9.0, 3.0)\n-1.0 True\n(1.5, None) True\nnone a\n3 b\n"
    );
    Ok(())
}
