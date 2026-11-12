//! Choosing rows of a frame (roadmap 3.13.4, decision 0045), built and run: `df[mask]`,
//! `df[i:j]`, `head` and `tail`, and the panics for a mask of another length and a step of 0.

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

/// Rows chosen by a mask, by a slice and by `head` and `tail` are a new frame, printed here
/// as pandas 3.0.5 prints the same rows: a mask built from columns, a slice with a negative
/// step, bounds past the ends, and negative counts. A frame assigned the rows chosen from
/// itself keeps them, and the frame the rows came from keeps all of its own.
#[test]
fn rows_are_chosen_by_masks_slices_head_and_tail() {
    let program = r#"
class Trade(Struct):
    price: f64
    volume: i64
    ok: bool

def big(trades: DataFrame[Trade]) -> DataFrame[Trade]:
    return trades[trades.volume > 15]

def main():
    trades: DataFrame[Trade] = DataFrame({
        "price": Array([1.5, 2.25, -3.0, 4.0, 5.5, 6.0, 7.0]),
        "volume": Array([10, 200, 3, 40, 50, 6, 70]),
        "ok": Array([True, False, True, True, False, True, False]),
    })
    print(trades[(trades.price > 2.0) & trades.ok])
    print(big(trades))
    print(trades[1:3])
    print(trades[::-2])
    print(trades[-2:])
    print(trades[5:2])
    rows = 2
    print(trades.head())
    print(trades.head(rows))
    print(trades.head(n=-5))
    print(trades.tail(3))
    print(trades.tail(0))
    print(trades.tail(-6))
    trades = trades[trades.ok]
    print(len(trades), trades[0].price, trades.head(1)[0])
    kept = trades[1:]
    print(kept.shape, trades.shape)
"#;
    let expected = [
        "   price  volume    ok\n",
        "0    4.0      40  True\n",
        "1    6.0       6  True\n",
        "   price  volume     ok\n",
        "0   2.25     200  False\n",
        "1   4.00      40   True\n",
        "2   5.50      50  False\n",
        "3   7.00      70  False\n",
        "   price  volume     ok\n",
        "0   2.25     200  False\n",
        "1  -3.00       3   True\n",
        "   price  volume     ok\n",
        "0    7.0      70  False\n",
        "1    5.5      50  False\n",
        "2   -3.0       3   True\n",
        "3    1.5      10   True\n",
        "   price  volume     ok\n",
        "0    6.0       6   True\n",
        "1    7.0      70  False\n",
        "Empty DataFrame\n",
        "Columns: [price, volume, ok]\n",
        "Index: []\n",
        "   price  volume     ok\n",
        "0   1.50      10   True\n",
        "1   2.25     200  False\n",
        "2  -3.00       3   True\n",
        "3   4.00      40   True\n",
        "4   5.50      50  False\n",
        "   price  volume     ok\n",
        "0   1.50      10   True\n",
        "1   2.25     200  False\n",
        "   price  volume     ok\n",
        "0   1.50      10   True\n",
        "1   2.25     200  False\n",
        "   price  volume     ok\n",
        "0    5.5      50  False\n",
        "1    6.0       6   True\n",
        "2    7.0      70  False\n",
        "Empty DataFrame\n",
        "Columns: [price, volume, ok]\n",
        "Index: []\n",
        "   price  volume     ok\n",
        "0    7.0      70  False\n",
        "4 1.5 Trade(price=1.5, volume=10, ok=True)\n",
        "(3, 3) (4, 3)\n",
    ];
    let (code, stdout, report) = run("frames-rows-chosen", program).unwrap();
    assert_eq!((code, report.as_str()), (Some(0), ""));
    assert_eq!(stdout, expected.concat());
}

/// A mask of another length than the frame panics at the mask, and a slice with a step of 0
/// panics at the slice, each before any row is read.
#[test]
fn a_mask_of_another_length_and_a_step_of_zero_panic() {
    let masking = r#"
def main():
    frame = DataFrame({"a": Array([1, 2, 3])})
    mask = Array([True, False])
    print(frame[mask])
"#;
    let (code, stdout, report) = run("frames-mask-length", masking).unwrap();
    assert_eq!((code, stdout.as_str()), (Some(101), ""));
    assert_eq!(
        report,
        "panic: array lengths differ in dimension 0: 3 and 2\n  --> main.byn:5:17\n"
    );
    let stepping = r#"
def main():
    frame = DataFrame({"a": Array([1, 2, 3])})
    step = 0
    print(frame[::step])
"#;
    let (code, stdout, report) = run("frames-step-zero", stepping).unwrap();
    assert_eq!((code, stdout.as_str()), (Some(101), ""));
    assert_eq!(
        report,
        "panic: slice step cannot be zero\n  --> main.byn:5:17\n"
    );
}
