//! Printed arrays against what `NumPy` 2.5 prints for the same arrays, byte for byte.

use super::{Elements, Plan, array};

/// What `NumPy` prints for each array; the file's header says how it was made.
const NUMPY: &str = include_str!("numpy.tsv");

/// The elements of one case, `values` as the file writes them, of `kind` and `bits` bits.
fn elements(kind: &str, bits: &str, values: &str) -> Elements {
    let values: Vec<&str> = values
        .split(' ')
        .filter(|value| !value.is_empty())
        .collect();
    let parse = |value: &str| -> f64 { value.parse().unwrap() };
    let pair = |value: &str| {
        let (real, imaginary) = value.split_once(',').unwrap();
        (real.to_owned(), imaginary.to_owned())
    };
    match (kind, bits) {
        ("bool", _) => Elements::Bool(values.iter().map(|&value| value == "1").collect()),
        ("signed", _) => {
            Elements::Signed(values.iter().map(|value| value.parse().unwrap()).collect())
        }
        ("unsigned", _) => {
            Elements::Unsigned(values.iter().map(|value| value.parse().unwrap()).collect())
        }
        ("float", "32") => {
            Elements::F32(values.iter().map(|value| value.parse().unwrap()).collect())
        }
        ("float", _) => Elements::F64(values.iter().map(|&value| parse(value)).collect()),
        ("complex", "64") => Elements::Complex64(
            values
                .iter()
                .map(|&value| {
                    let (real, imaginary) = pair(value);
                    (real.parse().unwrap(), imaginary.parse().unwrap())
                })
                .collect(),
        ),
        ("complex", _) => Elements::Complex128(
            values
                .iter()
                .map(|&value| {
                    let (real, imaginary) = pair(value);
                    (parse(&real), parse(&imaginary))
                })
                .collect(),
        ),
        _ => panic!("an unknown kind: {kind}"),
    }
}

#[test]
fn every_array_prints_as_numpy_prints_it() {
    let mut failures = Vec::new();
    let cases = NUMPY.lines().filter(|line| !line.starts_with('#'));
    for line in cases {
        let fields: Vec<&str> = line.split('\t').collect();
        let [kind, bits, shape, values, expected] = fields[..] else {
            panic!("a malformed case: {line}");
        };
        let lengths: Vec<usize> = shape
            .split(',')
            .map(|length| length.parse().unwrap())
            .collect();
        let plan = Plan::new(&lengths);
        let kept: usize = plan.kept.iter().map(Vec::len).product();
        let elements = elements(kind, bits, values);
        let count = values.split(' ').filter(|value| !value.is_empty()).count();
        assert_eq!(
            kept, count,
            "the plan keeps other elements than NumPy for {shape}"
        );
        let written = array(&lengths, &plan, &elements);
        let expected = expected.replace("\\n", "\n");
        if written != expected {
            failures.push(format!(
                "{kind}{bits} {shape}:\n{written}\nNumPy:\n{expected}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} differ:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

#[test]
fn a_huge_array_keeps_only_its_corners() {
    let plan = Plan::new(&[100_000_000]);
    assert_eq!(
        plan.kept,
        vec![vec![0, 1, 2, 99_999_997, 99_999_998, 99_999_999]]
    );
    let plan = Plan::new(&[usize::MAX, 2]);
    assert_eq!(plan.kept[1], vec![0, 1]);
}

#[test]
fn an_array_without_elements_prints_empty_brackets() {
    let plan = Plan::new(&[4, 0]);
    assert!(plan.is_empty());
    assert_eq!(array(&[4, 0], &plan, &Elements::F64(Vec::new())), "[]");
}
