//! Text that may be `None` (roadmap 3.13.6, decision 0045 part 3), built and run: an array
//! `Array[str | None, 1]` and a frame column `name: str | None`, through every operation a
//! column of numbers that may hold `None` has.

use super::{baryon_build, package, panic_report, stderr};
use std::io;
use std::process::Command;

/// An array of text with missing values: built, counted, read, shown, and its gaps taken out.
const ARRAY: &str = r#"
def initial(name: str | None) -> str:
    if name is None:
        return "-"
    return name[0:1]


def main():
    names = Array(["Vega", None, "it's", "", None, "Deneb", "Altair", "Rigel", None])
    print(names)
    print(len(names), names.count())
    print(names.isna())
    print(names.notna())
    print(names[0], names[1], names[3], names[-1])
    first = names[0]
    if first is not None:
        print(first.upper())
    kept = [names[at] for at in range(3)]
    print(kept)
    print(initial(names[2]), initial(names[4]))
    print(names.dropna())
    print(names.fillna("?"))
    twin = names.copy()
    print(len(twin), twin[5])
    typed: Array[str | None, 1] = Array([None, "x"])
    print(typed, typed.count())
    items: list[str | None] = []
    empty = Array(items)
    print(len(empty), empty.count(), empty)
    print(f"{typed} of {len(typed)}")
"#;

/// What [`ARRAY`] prints.
const ARRAY_PRINTED: &str = r#"
['Vega' None "it's" '' None 'Deneb' 'Altair' 'Rigel' None]
9 6
[False  True False False  True False False False  True]
[ True False  True  True False  True  True  True False]
Vega None  None
VEGA
['Vega', None, "it's"]
i -
['Vega' "it's" '' 'Deneb' 'Altair' 'Rigel']
['Vega' '?' "it's" '' '?' 'Deneb' 'Altair' 'Rigel' '?']
9 Deneb
[None 'x'] 1
0 0 []
[None 'x'] of 2
"#;

/// A frame with a text column that may hold `None`, through the frame's operations.
const FRAME: &str = r#"
class Star(Struct):
    name: str | None
    kind: str
    mass: f64 | None


class Home(Struct):
    name: str | None
    galaxy: str


def main():
    stars: DataFrame[Star] = DataFrame({
        "name": Array(["Vega", None, "Sirius", "Vega", None, ""]),
        "kind": Array(["A", "M", "A", "A", "M", "K"]),
        "mass": Array([2.1, 0.3, None, 2.1, 0.3, 1.0]),
    })
    homes: DataFrame[Home] = DataFrame({
        "name": Array(["Sirius", None, "Vega", "Rigel"]),
        "galaxy": Array(["Milky Way", "Unknown", "Milky Way", "Milky Way"]),
    })
    print(stars)
    print(stars.name)
    print(stars["name"].count(), len(stars))
    print(stars.dropna())
    print(stars.fillna({"name": "?"}))
    print(stars[stars.kind == "A"])
    print(stars[1:3])
    print(stars.tail(2))
    print(stars.sort_values("name"))
    print(stars.sort_values("name", ascending=False))
    print(concat([stars.head(2), stars.tail(1)]))
    print(stars.drop_duplicates())
    print(stars.drop_duplicates(subset=["name"]))
    print(stars.value_counts("name"))
    print(stars.groupby("name").agg(mass=("mass", "mean"), rows=("kind", "count")))
    print(stars.groupby("kind").agg(
        named=("name", "count"),
        first=("name", "first"),
        low=("name", "min"),
        high=("name", "max"),
        last=("name", "last"),
    ))
    print(stars.merge(homes, on="name"))
    print(stars.describe())
    print(stars.copy().rename(columns={"name": "star"}))
    print(stars.copy().assign(label=stars.name.copy()))
    for star in stars.itertuples():
        print(star.name, star.kind)
    print(stars[1].name, stars[2].name)
    rows = [Star("Deneb", "A", 19.0), Star(None, "K", None)]
    made = DataFrame(rows)
    print(made)
    again = DataFrame([star for star in stars.itertuples() if star.mass is not None])
    print(again)
    stars.name = Array(["a", "b", None, None, "e", "f"])
    print(stars.name)
"#;

/// What [`FRAME`] prints. pandas 3.0.5 prints the same tables for the same frames with the
/// column as its `string` type.
const FRAME_PRINTED: &str = r"
     name kind  mass
0    Vega    A   2.1
1    <NA>    M   0.3
2  Sirius    A  <NA>
3    Vega    A   2.1
4    <NA>    M   0.3
5            K   1.0
['Vega' None 'Sirius' 'Vega' None '']
4 6
   name kind  mass
0  Vega    A   2.1
1  Vega    A   2.1
2          K   1.0
     name kind  mass
0    Vega    A   2.1
1       ?    M   0.3
2  Sirius    A  <NA>
3    Vega    A   2.1
4       ?    M   0.3
5            K   1.0
     name kind  mass
0    Vega    A   2.1
1  Sirius    A  <NA>
2    Vega    A   2.1
     name kind  mass
0    <NA>    M   0.3
1  Sirius    A  <NA>
   name kind  mass
0  <NA>    M   0.3
1          K   1.0
     name kind  mass
0            K   1.0
1  Sirius    A  <NA>
2    Vega    A   2.1
3    Vega    A   2.1
4    <NA>    M   0.3
5    <NA>    M   0.3
     name kind  mass
0    Vega    A   2.1
1    Vega    A   2.1
2  Sirius    A  <NA>
3            K   1.0
4    <NA>    M   0.3
5    <NA>    M   0.3
   name kind  mass
0  Vega    A   2.1
1  <NA>    M   0.3
2          K   1.0
     name kind  mass
0    Vega    A   2.1
1    <NA>    M   0.3
2  Sirius    A  <NA>
3            K   1.0
     name kind  mass
0    Vega    A   2.1
1    <NA>    M   0.3
2  Sirius    A  <NA>
3            K   1.0
     name  count
0    Vega      2
1  Sirius      1
2              1
     name  mass  rows
0           1.0     1
1  Sirius   NaN     1
2    Vega   2.1     2
  kind  named first     low  high  last
0    A      3  Vega  Sirius  Vega  Vega
1    K      1                          
2    M      0  <NA>    <NA>  <NA>  <NA>
     name kind  mass     galaxy
0    Vega    A   2.1  Milky Way
1    <NA>    M   0.3    Unknown
2  Sirius    A  <NA>  Milky Way
3    Vega    A   2.1  Milky Way
4    <NA>    M   0.3    Unknown
  statistic      mass
0     count  5.000000
1      mean  1.160000
2       std  0.904434
3       min  0.300000
4       25%  0.300000
5       50%  1.000000
6       75%  2.100000
7       max  2.100000
     star kind  mass
0    Vega    A   2.1
1    <NA>    M   0.3
2  Sirius    A  <NA>
3    Vega    A   2.1
4    <NA>    M   0.3
5            K   1.0
     name kind  mass   label
0    Vega    A   2.1    Vega
1    <NA>    M   0.3    <NA>
2  Sirius    A  <NA>  Sirius
3    Vega    A   2.1    Vega
4    <NA>    M   0.3    <NA>
5            K   1.0        
Vega A
None M
Sirius A
Vega A
None M
 K
None Sirius
    name kind  mass
0  Deneb    A  19.0
1   <NA>    K  <NA>
   name kind  mass
0  Vega    A   2.1
1  <NA>    M   0.3
2  Vega    A   2.1
3  <NA>    M   0.3
4          K   1.0
['a' 'b' None None 'e' 'f']
";

/// Builds `program` as the package `name`, runs it, and compares what it prints with
/// `printed`.
fn prints(name: &str, program: &str, printed: &str) -> io::Result<()> {
    let directory = package(name, &[("main.byn", program)])?;
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output()?;
    assert_eq!(run.status.code(), Some(0), "{}", panic_report(&run));
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        printed.strip_prefix('\n').unwrap_or(printed)
    );
    Ok(())
}

/// An array of text keeps each `None` apart from an empty text, reads an element as a new
/// `str | None` that can be kept, and gives a text array from `dropna` and `fillna`.
#[test]
fn text_arrays_with_missing_values_build_and_run() -> io::Result<()> {
    prints("missing_text_array", ARRAY, ARRAY_PRINTED)
}

/// A text column that may hold `None` goes through every frame operation: a `None` stays
/// with its row, sorts last, equals a `None` in `drop_duplicates` and `merge`, leaves
/// `value_counts` and the groups of `groupby`, and is `None` where `agg` picks a text from a
/// group that has none.
#[test]
fn text_columns_with_missing_values_build_and_run() -> io::Result<()> {
    prints("missing_text_frame", FRAME, FRAME_PRINTED)
}
