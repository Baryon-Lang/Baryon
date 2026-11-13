//! Text arrays, `Array[str, 1]` (roadmap 3.13.3, decision 0045), built and run: one made
//! from a list, counted, indexed, looped over, copied, moved and kept in a list and a dict,
//! its distinct elements taken and counted (roadmap 3.13.9), parts of it chosen by a slice,
//! a mask and positions, its elements compared and searched, the array printed, and its
//! order, smallest and largest element found.

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

/// A program that takes parts of a text array: slices with either bound left out, a
/// negative bound, a step, a step of -1, bounds that take nothing and bounds past the end;
/// a mask written out and one computed; positions that repeat; a part kept while the array
/// is replaced by another part of itself; parts of parts; and parts of a frame's column.
const PARTS: &str = r#"
def show(names: Array[str, 1]):
    for name in names:
        print(name, end="|")
    print(len(names))


def main():
    names = Array(["delta", "", "alpha", "βeta", "gamma", "al"])
    show(names[1:3])
    show(names[:2])
    show(names[4:])
    show(names[::-1])
    show(names[-2:])
    show(names[1:5:2])
    show(names[3:1])
    show(names[:100])
    keep = Array([True, False, True, False, False, True])
    show(names[keep])
    lengths = Array([len(name) for name in names])
    show(names[lengths > 2])
    show(names[Array([5, 0, 0, 2])])
    first = names[:3]
    names = names[3:]
    show(first)
    show(names)
    show(names[1:][Array([True, False])])
    show(first[:0][:])
    trades = DataFrame({"symbol": Array(["x", "y", "z"]), "price": Array([3.0, 1.0, 2.0])})
    show(trades.symbol[trades.price > 1.5])
    show(trades.symbol[1:])
    show(trades.symbol[trades.price.argsort()])
"#;

/// A slice of a text array holds the elements Python's slice of the same list holds, and
/// a mask or an array of positions chooses elements as for an array of numbers. Each is a
/// new array, so it stays whole after the array it came from is replaced. The program
/// leaves no memory behind (checked under Valgrind when written).
#[test]
fn parts_of_a_text_array_are_new_arrays() {
    let expected = [
        "|alpha|2\n",
        "delta||2\n",
        "gamma|al|2\n",
        "al|gamma|βeta|alpha||delta|6\n",
        "gamma|al|2\n",
        "|βeta|2\n",
        "0\n",
        "delta||alpha|βeta|gamma|al|6\n",
        "delta|alpha|al|3\n",
        "delta|alpha|βeta|gamma|4\n",
        "al|delta|delta|alpha|4\n",
        "delta||alpha|3\n",
        "βeta|gamma|al|3\n",
        "gamma|1\n",
        "0\n",
        "x|z|2\n",
        "y|z|2\n",
        "y|z|x|3\n",
    ];
    let (code, stdout, report) = run("text-arrays-parts", PARTS).unwrap();
    assert_eq!(stdout, expected.concat());
    assert_eq!((code, report.as_str()), (Some(0), ""));
}

/// A mask of another length than the text array stops the program at the mask, and a
/// position past the end stops it too, as for an array of numbers.
#[test]
fn a_wrong_mask_or_position_of_a_text_array_panics() {
    let mask = r#"
def main():
    names = Array(["a", "b", "c"])
    print(len(names[Array([True, False])]))
"#;
    let (code, stdout, report) = run("text-arrays-short-mask", mask).unwrap();
    assert_eq!((code, stdout.as_str()), (Some(101), ""));
    assert!(
        report.starts_with(
            "panic: array lengths differ in dimension 0: 3 and 2\n  --> main.byn:4:21\n"
        ),
        "{report}"
    );
    let position = r#"
def main():
    names = Array(["a", "b", "c"])
    print(len(names[Array([0, 3])]))
"#;
    let (code, stdout, report) = run("text-arrays-far-position", position).unwrap();
    assert_eq!((code, stdout.as_str()), (Some(101), ""));
    assert!(
        report.starts_with("panic: index 3 is out of range for dimension 0 of length 3\n"),
        "{report}"
    );
}

/// A program that compares text arrays: with one `str` on either side and with another
/// array, by all six operators, with an empty text, capitals and a letter outside ASCII;
/// searches one with `in` and `not in`; and uses the masks to choose elements, rows of a
/// frame and, joined with `&`, `|` and `~` to masks of numbers, elements of a column.
const COMPARED: &str = r#"
def count(mask: Array[bool, 1]) -> int:
    return len(mask[mask])


def main():
    names = Array(["delta", "", "alpha", "βeta", "alpha", "Al"])
    other = Array(["delta", "x", "alpha", "beta", "al", "Al"])
    print(names == "alpha")
    print(names != "alpha")
    print(names < "b", names <= "alpha", names > "alpha", names >= "delta")
    print("b" > names, "alpha" >= names, "alpha" < names, "delta" <= names)
    print("alpha" == names, "" != names)
    print(names == other, names != other)
    print(names < other, names <= other, names > other, names >= other)
    print("alpha" in names, "beta" in names, "" in names, "x" not in names, "Al" not in names)
    word = "al" + "pha"
    print(word in names, count(names == word), count(names == other))
    for name in names[names != "alpha"]:
        print(name, end=" ")
    print()
    trades = DataFrame({"symbol": names.copy(), "price": Array([3.0, 1.0, 2.0, 5.0, 4.0, 6.0])})
    print(trades[trades.symbol == "alpha"])
    print(trades[(trades.symbol >= "a") & (trades.price > 2.5)])
    print(trades.price[~(trades.symbol == "alpha") | (trades.price < 2.0)])
    if "βeta" in trades.symbol and not ("zz" in trades.symbol):
        print("found")
    empty: Array[str, 1] = Array([])
    print(empty == "a", "a" in empty, len(empty == empty))
"#;

/// Comparing a text array gives the mask numpy gives for the same strings, and `in` what
/// numpy's `in` gives; the frames are what pandas prints for the same masks. The program
/// leaves no memory behind (checked under Valgrind when written). Two arrays of different
/// lengths stop the program at the comparison.
#[test]
fn text_arrays_are_compared_and_searched() {
    let expected = [
        "[False False  True False  True False]\n",
        "[ True  True False  True False  True]\n",
        "[False  True  True False  True  True] [False  True  True False  True  True] [ True False False  True False False] [ True False False  True False False]\n",
        "[False  True  True False  True  True] [False  True  True False  True  True] [ True False False  True False False] [ True False False  True False False]\n",
        "[False False  True False  True False] [ True False  True  True  True  True]\n",
        "[ True False  True False False  True] [False  True False  True  True False]\n",
        "[False  True False False False False] [ True  True  True False False  True] [False False False  True  True False] [ True False  True  True  True  True]\n",
        "True False True True False\n",
        "True 2 3\n",
        "delta  βeta Al \n",
        "  symbol  price\n",
        "0  alpha    2.0\n",
        "1  alpha    4.0\n",
        "  symbol  price\n",
        "0  delta    3.0\n",
        "1   βeta    5.0\n",
        "2  alpha    4.0\n",
        "[3. 1. 5. 6.]\n",
        "found\n",
        "[] False 0\n",
    ];
    let (code, stdout, report) = run("text-arrays-compared", COMPARED).unwrap();
    assert_eq!(stdout, expected.concat());
    assert_eq!((code, report.as_str()), (Some(0), ""));
    let lengths = r#"
def main():
    names = Array(["a", "b"])
    other = Array(["a"])
    print(names == other)
"#;
    let (code, stdout, report) = run("text-arrays-compared-lengths", lengths).unwrap();
    assert_eq!((code, stdout.as_str()), (Some(101), ""));
    assert!(
        report.starts_with(
            "panic: array lengths differ in dimension 0: 2 and 1\n  --> main.byn:5:11\n"
        ),
        "{report}"
    );
}

/// A program that prints text arrays: one with an empty text, a letter outside ASCII, both
/// kinds of quote, a new line, a tab and a backslash; one element and none; two in an
/// f-string; 40 elements, which take several lines; elements wider than a line; 1001
/// elements, which are cut short, and 1000, which are not; and a frame's column.
const SHOWN: &str = r#"
def main():
    names = Array(["delta", "", "alpha", "βeta", "it's", "say \"hi\"", "a\nb", "tab\there", "back\\slash"])
    print(names)
    print(names[:1], names[:0], len(names))
    print(f"names: {names[1:3]} and {names[names == 'alpha']}")
    long = Array([f"element-{at}" for at in range(40)])
    print(long)
    wide = Array([
        "".join(["x" for at in range(80)]),
        "y",
        "".join(["z" for at in range(73)]),
        "w",
        "".join(["v" for at in range(72)]),
        "u",
    ])
    print(wide)
    many = Array([f"n{at}" for at in range(1001)])
    print(many)
    print(many[:1000][990:])
    trades = DataFrame({"symbol": Array(["IBM", "AAPL"]), "price": Array([1.0, 2.0])})
    print(trades.symbol, trades.symbol.unique())
    print(trades["symbol"][::-1], sep="")
"#;

/// `print` and an f-string show a text array as numpy shows an array of the same strings:
/// each element as Python's `repr` writes it, lines of at most 75 characters, and `...` in
/// place of the middle of more than 1000 elements. The program leaves no memory behind
/// (checked under Valgrind when written).
#[test]
fn a_text_array_is_printed_as_numpy_prints_it() {
    let expected = [
        "['delta' '' 'alpha' 'βeta' \"it's\" 'say \"hi\"' 'a\\nb' 'tab\\there'\n",
        " 'back\\\\slash']\n",
        "['delta'] [] 9\n",
        "names: ['' 'alpha'] and ['alpha']\n",
        "['element-0' 'element-1' 'element-2' 'element-3' 'element-4' 'element-5'\n",
        " 'element-6' 'element-7' 'element-8' 'element-9' 'element-10' 'element-11'\n",
        " 'element-12' 'element-13' 'element-14' 'element-15' 'element-16'\n",
        " 'element-17' 'element-18' 'element-19' 'element-20' 'element-21'\n",
        " 'element-22' 'element-23' 'element-24' 'element-25' 'element-26'\n",
        " 'element-27' 'element-28' 'element-29' 'element-30' 'element-31'\n",
        " 'element-32' 'element-33' 'element-34' 'element-35' 'element-36'\n",
        " 'element-37' 'element-38' 'element-39']\n",
        "['xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'\n",
        " 'y'\n",
        " 'zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz'\n",
        " 'w'\n",
        " 'vvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvv'\n",
        " 'u']\n",
        "['n0' 'n1' 'n2' ... 'n998' 'n999' 'n1000']\n",
        "['n990' 'n991' 'n992' 'n993' 'n994' 'n995' 'n996' 'n997' 'n998' 'n999']\n",
        "['IBM' 'AAPL'] ['IBM' 'AAPL']\n",
        "['AAPL' 'IBM']\n",
    ];
    let (code, stdout, report) = run("text-arrays-shown", SHOWN).unwrap();
    assert_eq!(stdout, expected.concat());
    assert_eq!((code, report.as_str()), (Some(0), ""));
}

/// A program that puts a text array in order with `argsort`, takes its `min` and `max`,
/// and makes its text with `str`: with texts that repeat, an empty text, a capital and a
/// letter outside ASCII, one element, a frame's column, and an empty array.
const ORDERED: &str = r#"
def main():
    names = Array(["delta", "", "alpha", "βeta", "alpha", "Al"])
    print(names.argsort())
    print(names[names.argsort()])
    print(names.min(), names.max(), names[1:].min(), len(names.min()))
    text = str(names)
    print(text, len(text), str(names[:2]) == "['delta' '']")
    low = names.max()
    names = names[:1]
    print(low, names.min(), names.max(), names.argsort())
    trades = DataFrame({"symbol": Array(["IBM", "AAPL", "MSFT"]), "price": Array([3.0, 1.0, 2.0])})
    print(trades.symbol.min(), trades.symbol.max(), trades.price[trades.symbol.argsort()])
    empty: Array[str, 1] = Array([])
    print(empty.argsort(), str(empty))
"#;

/// `argsort` gives the positions numpy's stable `argsort` gives for the same strings,
/// `min` and `max` the strings Python's `min` and `max` give, each a new `str` that
/// outlives the array, and `str` the text `print` writes. The program leaves no memory
/// behind (checked under Valgrind when written). `max` of an empty array stops the program.
#[test]
fn a_text_array_is_ordered_and_reduced() {
    let expected = [
        "[1 5 2 4 0 3]\n",
        "['' 'Al' 'alpha' 'alpha' 'delta' 'βeta']\n",
        " βeta  0\n",
        "['delta' '' 'alpha' 'βeta' 'alpha' 'Al'] 40 True\n",
        "βeta delta delta [0]\n",
        "AAPL MSFT [1. 3. 2.]\n",
        "[] []\n",
    ];
    let (code, stdout, report) = run("text-arrays-ordered", ORDERED).unwrap();
    assert_eq!(stdout, expected.concat());
    assert_eq!((code, report.as_str()), (Some(0), ""));
    let empty = r"
def main():
    empty: Array[str, 1] = Array([])
    print(empty.max())
";
    let (code, stdout, report) = run("text-arrays-empty-max", empty).unwrap();
    assert_eq!((code, stdout.as_str()), (Some(101), ""));
    assert!(
        report.starts_with("panic: `empty.max()` of an empty array\n  --> main.byn:4:11\n"),
        "{report}"
    );
}
