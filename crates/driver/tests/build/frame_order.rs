//! The sorts behind `sort_values`, `groupby` and `merge` on frames long enough to take their
//! fast paths (roadmap 3.13.12): the bucket sort of a column of `f64` and the counting sort of
//! a column of `i64`, which the short frames of the other tests never reach.

use super::{baryon_build, package, stderr};
use std::process::Command;

/// A program that sorts 20000 rows five ways and checks each result itself: that it is in
/// order, that rows with equal values kept the order they came in, and that NaN is last. The
/// five tables are plain numbers, a few NaN, one value far from the rest (which crowds the
/// others into one bucket), two infinities (which the bucket sort hands to the merge sort),
/// and integer keys too far apart to count. It then groups and joins the first table.
const PROGRAM: &str = r#"
from math import inf, isnan, nan


class Row(Struct):
    id: i64
    value: f64
    key: i64


class Weight(Struct):
    key: i64
    weight: f64


def table(rows: i64, kind: i64) -> DataFrame[Row]:
    """`rows` rows of made-up numbers with many equal values. `kind` 1 adds a few NaN, 2 one
    value far from the rest, 3 an infinity, and 4 gives keys spread over a wide range."""
    id = Array.fill((rows,), 0)
    value = Array.zeros(rows)
    key = Array.fill((rows,), 0)
    for i in range(rows):
        id[i] = i
        value[i] = f64((i * 7919) % 5003) / 100.0 - 20.0
        key[i] = (i * 31) % 97 - 40
        if kind == 4:
            key[i] = ((i * 7919) % 5003) * 1000000007 - 2500000000000
    if kind == 1:
        for i in range(0, rows, 1001):
            value[i] = nan
    if kind == 2:
        value[rows // 2] = 1.0e12
    if kind == 3:
        value[rows // 3] = inf
        value[rows // 5] = -inf
    return DataFrame({"id": id, "value": value, "key": key})


def float_order(frame: DataFrame[Row], rising: bool) -> bool:
    """Whether `frame` is sorted by `value` in the direction given, NaN last, and rows with
    equal values are in the order of their `id`."""
    for at in range(1, len(frame)):
        before = frame.value[at - 1]
        value = frame.value[at]
        if isnan(before) and not isnan(value):
            return False
        if isnan(value):
            if isnan(before) and frame.id[at - 1] > frame.id[at]:
                return False
            continue
        if (before > value) if rising else (before < value):
            return False
        if before == value and frame.id[at - 1] > frame.id[at]:
            return False
    return True


def key_order(frame: DataFrame[Row], rising: bool) -> bool:
    """`float_order` for the column `key`."""
    for at in range(1, len(frame)):
        before = frame.key[at - 1]
        key = frame.key[at]
        if (before > key) if rising else (before < key):
            return False
        if before == key and frame.id[at - 1] > frame.id[at]:
            return False
    return True


def main():
    for kind in range(5):
        frame = table(20000, kind)
        up = frame.sort_values("value")
        down = frame.sort_values("value", ascending=False)
        keys_up = frame.sort_values("key")
        keys_down = frame.sort_values("key", ascending=False)
        both = frame.sort_values(["key", "value"], ascending=[True, False])
        print(
            kind,
            float_order(up, True) and float_order(down, False),
            key_order(keys_up, True) and key_order(keys_down, False),
            up.id.sum() == frame.id.sum() and keys_down.id.sum() == frame.id.sum(),
            both.id[0], both.id[19999],
        )
    frame = table(20000, 0)
    groups = frame.groupby("key").agg(total=("value", "sum"), rows=("id", "count"))
    total = groups.total.sum()
    print(len(groups), groups.key[0], groups.key[96], groups.rows.sum(), f"{total:.6f}")
    weights: DataFrame[Weight] = DataFrame({
        "key": Array([at - 40 for at in range(0, 97, 2)]),
        "weight": Array([f64(at) for at in range(0, 97, 2)]),
    })
    joined = frame.merge(weights, on="key")
    print(len(joined), joined.id[0], joined.id[len(joined) - 1], f"{joined.weight.sum():.1f}")
"#;

/// What the program prints: for each table its number, three checks, and the first and last
/// row of a sort by two columns, then the group and join results. Every number is what pandas
/// 3.0.5 gives for the same tables with `kind="stable"`.
const EXPECTED: &str = "\
0 True True True 3977 9143
1 True True True 3977 9143
2 True True True 3977 9143
3 True True True 3977 9143
4 True True True 0 15872
97 -40 56 20000 100223.070000
10101 0 19999 484824.0
";

/// Long frames sort, group and join as pandas does.
#[test]
fn long_frames_sort_group_and_join_as_pandas_does() {
    let directory = package("frame-order", &[("main.byn", PROGRAM)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let run = Command::new(&executable).output().unwrap();
    assert!(run.status.success(), "{}", stderr(&run));
    assert_eq!(String::from_utf8_lossy(&run.stdout), EXPECTED);
}
