//! `for` loops over arrays with missing values (roadmap 3.13.6, decision 0045), built and
//! run: each pass gives the next element as a `T | None`, alone, inside `enumerate`, `zip`
//! and `reversed`, and in a comprehension. Text that may be `None` is borrowed from the array.

use super::{baryon_build, package, panic_report, stderr};
use std::io;
use std::process::Command;

/// A loop over numbers and over `bool`s that may be `None` reads every element in order,
/// the gaps as `None`, and the variable narrows by `is not None` as any `T | None` does.
/// The array can be an expression, and an empty one runs no pass.
#[test]
fn loops_read_each_element_or_none() -> io::Result<()> {
    let program = r#"
def total(values: Array[f64 | None, 1]) -> f64:
    result = 0.0
    for value in values:
        if value is not None:
            result += value
    return result


def main():
    bids: Array[f64 | None, 1] = Array([1.5, None, 2.5])
    counts: Array[i64 | None, 1] = Array([None, 2, 3])
    flags: Array[bool | None, 1] = Array([True, None, False])
    print(total(bids))
    for bid in bids:
        print(bid)
    for at, count in enumerate(counts):
        print(at, count)
    for bid, flag in zip(bids, flags):
        print(bid, flag)
    for flag in reversed(flags):
        print(flag)
    print([bid for bid in bids if bid is not None])
    print([0 if count is None else count for count in counts])
    for bid in bids[1:]:
        print(bid)
    missing = 0
    for flag in bids > 2.0:
        if flag is None:
            missing += 1
    print(missing)
    empty: Array[f64 | None, 1] = Array([])
    for bid in empty:
        print("never")
"#;
    let directory = package("optional_loops", &[("main.byn", program)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output()?;
    assert_eq!(run.status.code(), Some(0), "{}", panic_report(&run));
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "4.0\n1.5\nNone\n2.5\n0 None\n1 2\n2 3\n1.5 True\nNone None\n2.5 False\n\
         False\nNone\nTrue\n[1.5, 2.5]\n[0, 2, 3]\nNone\n2.5\n1\n"
    );
    Ok(())
}

/// A loop over text that may be `None` gives each element as a `str | None` that borrows
/// the array's bytes, alone and inside `enumerate`, `zip` and `reversed`, with `continue`
/// and `break`. An empty array runs no pass.
#[test]
fn loops_over_text_borrow_each_element() -> io::Result<()> {
    let program = r#"
def main():
    names: Array[str | None, 1] = Array(["delta", None, "", "beta"])
    for name in names:
        if name is not None:
            print(name, len(name))
        else:
            print("gone")
    for at, name in enumerate(names):
        print(at, name)
    lengths = [len(name) if name is not None else -1 for name in reversed(names)]
    print(lengths)
    kept: list[str] = []
    ids: Array[i64 | None, 1] = Array([1, None, 3, 4])
    for name, id in zip(names, ids):
        print(name, id)
        if name is not None:
            kept.append(name.copy())
    print(kept)
    total = 0
    for name in names:
        if name is None:
            continue
        if name == "":
            break
        total += len(name)
    print(total)
    for name in names[0:0]:
        print("never")
"#;
    let directory = package("optional_loops_text", &[("main.byn", program)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output()?;
    assert_eq!(run.status.code(), Some(0), "{}", panic_report(&run));
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "delta 5\ngone\n 0\nbeta 4\n\
         0 delta\n1 None\n2 \n3 beta\n\
         [4, 0, -1, 5]\n\
         delta 1\nNone None\n 3\nbeta 4\n\
         ['delta', '', 'beta']\n\
         5\n"
    );
    Ok(())
}
