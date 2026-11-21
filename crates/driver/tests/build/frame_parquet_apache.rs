//! `DataFrame.read_parquet` (roadmap 3.14.6.3) against files other writers made: ten files of
//! Apache's `parquet-testing` collection, written by Impala, by parquet-mr and by Arrow's C++
//! library among others, which the repository carries under the Apache License 2.0
//! (`THIRD-PARTY-NOTICES.md`). The reader's other fixtures were written by polars or by hand
//! from the specification, so these are the check that it reads what it did not write.
//!
//! The files are in `tests/fixtures/parquet/apache`, each beside the CSV of the values polars
//! reads from it; the `README.md` one directory up says which file holds what.

use super::{Scratch, baryon_build, package};
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Output};

/// The directory of the Apache files and of the CSV file of each.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/parquet/apache");

/// The files, by name without `.parquet`, and how many rows each holds.
const FILES: [(&str, usize); 10] = [
    ("alltypes_plain", 8),
    ("alltypes_plain.snappy", 2),
    ("alltypes_dictionary", 2),
    ("datapage_v2.snappy", 5),
    ("delta_binary_packed", 200),
    ("delta_byte_array", 1000),
    ("delta_length_byte_array", 1000),
    ("delta_encoding_optional_column", 100),
    ("lz4_raw_compressed", 4),
    ("rle_boolean_encoding", 68),
];

/// Reads every file into a frame whose schema names the columns a frame has a type for, each
/// with `| None`, writes the frame as CSV and prints the file's name and its rows. The three
/// `alltypes` files share a schema; their timestamp column, the list column of
/// `datapage_v2.snappy` and most columns of the two widest files are left out.
const PROGRAM: &str = r#"
class AlltypesPlain(Struct):
    id: i32 | None
    bool_col: bool | None
    tinyint_col: i32 | None
    smallint_col: i32 | None
    int_col: i32 | None
    bigint_col: i64 | None
    float_col: f32 | None
    double_col: f64 | None
    date_string_col: str | None
    string_col: str | None


class DatapageV2Snappy(Struct):
    a: str | None
    b: i32 | None
    c: f64 | None
    d: bool | None


class DeltaBinaryPacked(Struct):
    bitwidth0: i64 | None
    bitwidth1: i64 | None
    bitwidth7: i64 | None
    bitwidth8: i64 | None
    bitwidth31: i64 | None
    bitwidth32: i64 | None
    bitwidth33: i64 | None
    bitwidth63: i64 | None
    bitwidth64: i64 | None
    int_value: i32 | None


class DeltaByteArray(Struct):
    c_customer_id: str | None
    c_first_name: str | None
    c_email_address: str | None


class DeltaLengthByteArray(Struct):
    FRUIT: str | None


class DeltaEncodingOptionalColumn(Struct):
    c_customer_sk: i64 | None
    c_current_cdemo_sk: i64 | None
    c_current_hdemo_sk: i64 | None
    c_current_addr_sk: i64 | None
    c_first_shipto_date_sk: i64 | None
    c_first_sales_date_sk: i64 | None
    c_birth_day: i64 | None
    c_birth_month: i64 | None
    c_birth_year: i64 | None
    c_customer_id: str | None
    c_salutation: str | None
    c_first_name: str | None
    c_last_name: str | None
    c_preferred_cust_flag: str | None
    c_birth_country: str | None
    c_email_address: str | None
    c_last_review_date: str | None


class Lz4RawCompressed(Struct):
    c0: i64 | None
    c1: str | None
    v11: f64 | None


class RleBooleanEncoding(Struct):
    datatype_boolean: bool | None


def main() -> Raises[None, OSError | ValueError]:
    t0: DataFrame[AlltypesPlain] = DataFrame.read_parquet(
        "alltypes_plain.parquet"
    )
    t0.to_csv("alltypes_plain.csv", na_rep="<none>")
    print("alltypes_plain", len(t0))
    t1: DataFrame[AlltypesPlain] = DataFrame.read_parquet(
        "alltypes_plain.snappy.parquet"
    )
    t1.to_csv("alltypes_plain.snappy.csv", na_rep="<none>")
    print("alltypes_plain.snappy", len(t1))
    t2: DataFrame[AlltypesPlain] = DataFrame.read_parquet(
        "alltypes_dictionary.parquet"
    )
    t2.to_csv("alltypes_dictionary.csv", na_rep="<none>")
    print("alltypes_dictionary", len(t2))
    t3: DataFrame[DatapageV2Snappy] = DataFrame.read_parquet(
        "datapage_v2.snappy.parquet"
    )
    t3.to_csv("datapage_v2.snappy.csv", na_rep="<none>")
    print("datapage_v2.snappy", len(t3))
    t4: DataFrame[DeltaBinaryPacked] = DataFrame.read_parquet(
        "delta_binary_packed.parquet"
    )
    t4.to_csv("delta_binary_packed.csv", na_rep="<none>")
    print("delta_binary_packed", len(t4))
    t5: DataFrame[DeltaByteArray] = DataFrame.read_parquet(
        "delta_byte_array.parquet"
    )
    t5.to_csv("delta_byte_array.csv", na_rep="<none>")
    print("delta_byte_array", len(t5))
    t6: DataFrame[DeltaLengthByteArray] = DataFrame.read_parquet(
        "delta_length_byte_array.parquet"
    )
    t6.to_csv("delta_length_byte_array.csv", na_rep="<none>")
    print("delta_length_byte_array", len(t6))
    t7: DataFrame[DeltaEncodingOptionalColumn] = DataFrame.read_parquet(
        "delta_encoding_optional_column.parquet"
    )
    t7.to_csv("delta_encoding_optional_column.csv", na_rep="<none>")
    print("delta_encoding_optional_column", len(t7))
    t8: DataFrame[Lz4RawCompressed] = DataFrame.read_parquet(
        "lz4_raw_compressed.parquet"
    )
    t8.to_csv("lz4_raw_compressed.csv", na_rep="<none>")
    print("lz4_raw_compressed", len(t8))
    t9: DataFrame[RleBooleanEncoding] = DataFrame.read_parquet(
        "rle_boolean_encoding.parquet"
    )
    t9.to_csv("rle_boolean_encoding.csv", na_rep="<none>")
    print("rle_boolean_encoding", len(t9))
"#;

/// Builds [`PROGRAM`] with the Apache files beside it and runs it there. Returns what it
/// printed and the directory it ran in, which is removed when dropped.
///
/// # Errors
///
/// Fails when the package cannot be written or the program cannot be run.
fn run() -> io::Result<(Output, Scratch)> {
    let directory = package("frame_parquet_apache", &[("main.byn", PROGRAM)])?;
    for (name, _) in FILES {
        let file = format!("{name}.parquet");
        fs::copy(Path::new(FIXTURES).join(&file), directory.join(&file))?;
    }
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{output:?}");
    let ran = Command::new(&executable)
        .current_dir(&*directory)
        .output()?;
    Ok((ran, directory))
}

/// Files written by Impala (`alltypes_*`: plain values, dictionaries, Snappy), by parquet-mr
/// (`datapage_v2.snappy`: version 2 pages; `delta_binary_packed`: differences of every bit
/// width from 0 to 64; `delta_byte_array` and `delta_encoding_optional_column`: text as
/// shared prefixes, with missing values), by Arrow's C++ library (`lz4_raw_compressed`) and
/// two that name no writer (`delta_length_byte_array`, `rle_boolean_encoding`) are read with
/// every row, and each frame written as CSV is byte for byte the file kept beside the Parquet
/// file, whose values are those polars 2.1.0 reads from it.
#[test]
fn files_of_other_writers_are_read() {
    let (ran, directory) = run().unwrap();
    assert_eq!(String::from_utf8_lossy(&ran.stderr), "");
    let printed: Vec<String> = FILES
        .iter()
        .map(|(name, rows)| format!("{name} {rows}"))
        .collect();
    let stdout = String::from_utf8_lossy(&ran.stdout);
    assert_eq!(stdout.lines().collect::<Vec<_>>(), printed);
    for (name, _) in FILES {
        let file = format!("{name}.csv");
        let expected = fs::read(Path::new(FIXTURES).join(&file)).unwrap();
        let written = fs::read(directory.join(&file)).unwrap();
        assert!(written == expected, "{name} was read differently");
    }
}
