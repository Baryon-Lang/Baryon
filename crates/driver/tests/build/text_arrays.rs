//! Text arrays, `Array[str, 1]` (roadmap 3.13.3, decision 0045), built and run: one made
//! from a list, counted, indexed, looped over, copied, moved and kept in a list and a dict,
//! and its distinct elements taken and counted (roadmap 3.13.9).

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

/// A text array made from a list holds the list's strings, an empty one and one of two-byte
/// characters among them, and leaves the list usable. Its elements are read by position, from
/// the end, in `for`, `enumerate`, `reversed`, `zip` and a comprehension, and used as any
/// borrowed `str`: compared, sliced, split and put in an f-string. A function takes one and
/// returns one; a copy and the moved original both read; an empty one has no elements; a list
/// and a dict hold them. Every line is what Python prints for the same lists of strings. An
/// index past the end panics as an array's does.
#[test]
fn text_arrays_are_built_read_and_copied() {
    let program = r#"
def longest(names: Array[str, 1]) -> str:
    best = 0
    for position, name in enumerate(names):
        if len(name) > len(names[best]):
            best = position
    return names[best].copy()


def numbered(count: int) -> Array[str, 1]:
    parts: list[str] = []
    for i in range(count):
        parts.append(f"row{i}")
    return Array(parts)


def main():
    words = ["alpha", "", "βeta", "gamma delta"]
    names: Array[str, 1] = Array(words)
    print(len(names), names[0], names[2], names[-1], len(names[1]), len(names[2]))
    print(longest(names), words[0])
    position = 3
    print(names[position - 1] == "βeta", names[0] < names[3], f"{names[3]}!")
    print(names[0].upper(), names[3].split(), names[0][1:3])
    for name in reversed(names):
        print(name)
    rows = numbered(3)
    for left, right in zip(names, rows):
        print(left + right)
    print([len(name) for name in rows])
    kept = rows.copy()
    moved = rows
    print(kept[2], moved[2], len(numbered(0)))
    empty: Array[str, 1] = Array([])
    for name in empty:
        print(name)
    held = [numbered(1), numbered(2)]
    table = {"two": numbered(2)}
    print(len(held[1]), held[1][1], table["two"][0])
    print(names[4])
"#;
    let expected = [
        "4 alpha βeta gamma delta 0 4\n",
        "gamma delta alpha\n",
        "True True gamma delta!\n",
        "ALPHA ['gamma', 'delta'] lp\n",
        "gamma delta\n",
        "βeta\n",
        "\n",
        "alpha\n",
        "alpharow0\n",
        "row1\n",
        "βetarow2\n",
        "[4, 4, 4]\n",
        "row2 row2 0\n",
        "2 row1 row0\n",
    ];
    let (code, stdout, report) = run("text-arrays", program).unwrap();
    assert_eq!(stdout, expected.concat());
    assert_eq!(code, Some(101));
    assert_eq!(
        report,
        "panic: index 4 is out of range for dimension 0 of length 4\n  --> main.byn:40:17\n"
    );
}

/// A program that takes the distinct elements of text arrays and counts them: texts that
/// repeat, that differ only in case, an empty text and a letter outside ASCII, a column of
/// a frame, an empty array, and the result used as an array and as a column.
const DISTINCT: &str = r#"
def main():
    symbols = Array(["IBM", "AAPL", "", "IBM", "βeta", "ibm", "AAPL", "IBM", ""])
    seen = symbols.unique()
    print(len(seen), symbols.nunique(), len(symbols))
    for symbol in seen:
        print(symbol)
    trades = DataFrame({"symbol": symbols.copy(), "day": Array([1, 1, 2, 2, 1, 1, 2, 1, 2])})
    print(trades.symbol.unique()[3], trades.symbol.nunique(), trades["symbol"].nunique())
    print(DataFrame({"symbol": trades.symbol.unique()}))
    empty: list[str] = []
    none = Array(empty)
    print(len(none.unique()), none.nunique())
    print(symbols.unique().unique().nunique(), trades.day.unique())
"#;

/// `unique` of a text array gives its distinct texts in the order they first appear, and
/// `nunique` counts them, as pandas does for the same strings.
#[test]
fn distinct_texts_are_kept_and_counted() {
    let expected = [
        "5 5 9\n",
        "IBM\n",
        "AAPL\n",
        "\n",
        "βeta\n",
        "ibm\n",
        "βeta 5 5\n",
        "  symbol\n",
        "0    IBM\n",
        "1   AAPL\n",
        "2       \n",
        "3   βeta\n",
        "4    ibm\n",
        "0 0\n",
        "5 [1 2]\n",
    ];
    let (code, stdout, report) = run("text-arrays-distinct", DISTINCT).unwrap();
    assert_eq!(stdout, expected.concat());
    assert_eq!((code, report.as_str()), (Some(0), ""));
}
