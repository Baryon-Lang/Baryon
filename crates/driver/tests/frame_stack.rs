//! Stacking frames (roadmap 3.13.9, decision 0045), through every stage up to the type
//! checker: the built-in `concat`, the type it gives, and the mistakes it reports.
//!
//! These tests live in the driver because they need several implementations, and only an
//! application crate may depend on more than one (decision 0001).

use driver::Session;

/// Checks `main.byn` and returns every diagnostic, rendered and joined by blank lines. An empty
/// string means the program is clean. The program's first newline is dropped.
fn check_one(text: &str) -> String {
    let mut session = Session::new();
    if let Err(error) = session.add_file("main.byn", text.strip_prefix('\n').unwrap_or(text)) {
        return error.to_string();
    }
    session
        .check()
        .iter()
        .map(|diagnostic| diagnostics::render(diagnostic, &session).to_string())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// `concat` gives a frame of the type of its first frame, and reads frames borrowed from
/// the caller without moving them. It reports frames not written out in a list, pandas's
/// other arguments, an empty list, a value that is no frame, and a frame whose columns are
/// in another order or of another type than the first frame's.
#[test]
fn concat_keeps_the_type_and_reports_mistakes() {
    let program = r#"
class Trade(Struct):
    price: f64
    volume: i64


def stacking(first: DataFrame[Trade], second: DataFrame[Trade], several: list[DataFrame[Trade]]):
    swapped = second[["volume", "price"]]
    whole = DataFrame({"price": Array([1]), "volume": Array([2])})
    a = concat(first, second)
    b = concat([first, second], axis=1)
    c = concat(several)
    d = concat([])
    e = concat([first, 3])
    f = concat([first, swapped])
    g = concat([first, whole, first])
    wrong: int = concat([first, first])
"#;
    assert_eq!(
        check_one(program),
        r"error: `concat` takes the frames to stack, written out in a list
 --> main.byn:9:9
  |
9 |     a = concat(first, second)
  |         ^^^^^^^^^^^^^^^^^^^^^ wrong arguments
  = note: for example `concat([january, february])`; pandas's `axis=`, `join=` and `ignore_index=` are not built

error: `concat` takes the frames to stack, written out in a list
  --> main.byn:10:9
   |
10 |     b = concat([first, second], axis=1)
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wrong arguments
   = note: for example `concat([january, february])`; pandas's `axis=`, `join=` and `ignore_index=` are not built

error: `concat` takes the frames to stack, written out in a list
  --> main.byn:11:9
   |
11 |     c = concat(several)
   |         ^^^^^^^^^^^^^^^ wrong arguments
   = note: for example `concat([january, february])`; pandas's `axis=`, `join=` and `ignore_index=` are not built

error: `concat` needs at least one frame to stack
  --> main.byn:12:16
   |
12 |     d = concat([])
   |                ^^ no frames

error: `concat` stacks frames, found `i64`
  --> main.byn:13:24
   |
13 |     e = concat([first, 3])
   |                        ^ not a `DataFrame`

error: `concat` stacks frames of one schema: this frame is `DataFrame[volume: i64, price: f64]`, the first is `DataFrame[Trade]`
  --> main.byn:14:24
   |
14 |     f = concat([first, swapped])
   |                        ^^^^^^^ other columns
   = note: give the frames the same columns, in one order and of one type, with `df[[...]]`, `rename` or `assign` first

error: `concat` stacks frames of one schema: this frame is `DataFrame[price: i64, volume: i64]`, the first is `DataFrame[Trade]`
  --> main.byn:15:24
   |
15 |     g = concat([first, whole, first])
   |                        ^^^^^ other columns
   = note: give the frames the same columns, in one order and of one type, with `df[[...]]`, `rename` or `assign` first

error: expected `i64`, found `DataFrame[Trade]`
  --> main.byn:16:18
   |
16 |     wrong: int = concat([first, first])
   |                  ^^^^^^^^^^^^^^^^^^^^^^ the annotation says `i64`"
    );
}
