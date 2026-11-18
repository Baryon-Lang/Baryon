//! `argsort` of an array with missing values (roadmap 3.13.6, decision 0045), built and run:
//! the positions that put the elements in rising order, with those of the `None`s last.

use super::{baryon_build, package, panic_report, stderr};
use std::io;
use std::process::Command;

/// Floats, integers, `bool`s and text that may be `None` are each put in rising order with
/// the `None`s last, after any NaN, and equal elements and `None`s in the order they stand
/// in. The positions choose elements of the array they came from, and an array with no
/// elements gives none. A column of a frame is such an array.
#[test]
fn positions_put_none_last() -> io::Result<()> {
    let program = r#"
import math


class Quote(Struct):
    price: f64
    bid: f64 | None


def main():
    bids: Array[f64 | None, 1] = Array([3.0, None, 1.0, math.nan, None, 2.0])
    print(bids.argsort())
    print(bids[bids.argsort()])
    names: Array[str | None, 1] = Array(["b", None, "a", "b"])
    order = names.argsort()
    print(order, names[order])
    flags: Array[bool | None, 1] = Array([True, None, False])
    print(flags.argsort())
    ids: Array[i64 | None, 1] = Array([5, 4, None, 4])
    print(ids.argsort())
    empty = bids[0:0]
    print(empty.argsort(), len(empty.argsort()))
    gone: Array[i64 | None, 1] = Array([None, None])
    print(gone.argsort())
    quotes: DataFrame[Quote] = DataFrame(
        {"price": Array([1.0, 2.0, 3.0]), "bid": Array([2.5, None, 1.5])}
    )
    print(quotes.bid.argsort())
"#;
    let directory = package("optional_sorts_positions", &[("main.byn", program)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output()?;
    assert_eq!(run.status.code(), Some(0), "{}", panic_report(&run));
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "[2 5 0 3 1 4]\n\
         [1.0 2.0 3.0 nan None None]\n\
         [2 0 3 1] ['a' 'b' 'b' None]\n\
         [2 0 1]\n\
         [1 3 0 2]\n\
         [] 0\n\
         [0 1]\n\
         [2 0 1]\n"
    );
    Ok(())
}
