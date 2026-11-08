//! Each formatting function against what Python writes for the same value and specification.

use super::{FormatSpec, boolean, float, signed, text};

/// What Python 3 writes for `format(value, spec)` across integers, `bool`s, floats and text.
const PYTHON: &str = include_str!("python.tsv");

/// The [`FormatSpec`] of `text`, a specification the compiler would accept: a small parser for the
/// tests, since the runtime gets specifications already parsed.
fn spec(text: &str) -> FormatSpec {
    let characters: Vec<char> = text.chars().collect();
    let mut spec = FormatSpec::DEFAULT;
    let mut at = 0;
    let is_align = |character: Option<&char>| matches!(character, Some('<' | '>' | '^' | '='));
    if is_align(characters.get(1)) {
        spec.fill = u32::from(characters[0]);
        spec.align = characters[1] as u8;
        at = 2;
    } else if is_align(characters.first()) {
        spec.align = characters[0] as u8;
        at = 1;
    }
    if let Some(&sign @ ('+' | '-' | ' ')) = characters.get(at) {
        spec.sign = sign as u8;
        at += 1;
    }
    if characters.get(at) == Some(&'#') {
        spec.alternate = 1;
        at += 1;
    }
    if characters.get(at) == Some(&'0') {
        spec.zero = 1;
        at += 1;
    }
    let number = |at: &mut usize| {
        let start = *at;
        while characters.get(*at).is_some_and(char::is_ascii_digit) {
            *at += 1;
        }
        let digits: String = characters[start..*at].iter().collect();
        digits.parse::<i64>().unwrap_or(-1)
    };
    spec.width = number(&mut at);
    if let Some(&separator @ (',' | '_')) = characters.get(at) {
        spec.grouping = separator as u8;
        at += 1;
    }
    if characters.get(at) == Some(&'.') {
        at += 1;
        spec.precision = number(&mut at);
    }
    if let Some(&letter) = characters.get(at) {
        spec.presentation = letter as u8;
        at += 1;
    }
    assert_eq!(
        at,
        characters.len(),
        "the test parser does not read `{text}`"
    );
    spec
}

#[test]
fn every_value_formats_as_python_formats_it() {
    let mut failures = Vec::new();
    for line in PYTHON.lines().filter(|line| !line.starts_with('#')) {
        let fields: Vec<&str> = line.split('\t').collect();
        let [kind, value, specification, expected] = fields[..] else {
            panic!("a malformed case: {line}");
        };
        let spec = spec(specification);
        let written = match kind {
            "signed" => signed(value.parse().unwrap(), &spec),
            "unsigned" => super::unsigned(value.parse().unwrap(), &spec),
            "bool" => boolean(value == "true", &spec),
            "float" => float(value.parse().unwrap(), false, &spec),
            "text" => text(value, &spec),
            _ => panic!("an unknown kind: {kind}"),
        };
        if written != expected {
            failures.push(format!(
                "{kind} {value} `{specification}`: {written:?}, Python {expected:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn an_f32_keeps_its_own_shortest_digits() {
    assert_eq!(float(f64::from(0.1_f32), true, &FormatSpec::DEFAULT), "0.1");
    assert_eq!(
        float(f64::from(0.1_f32), false, &FormatSpec::DEFAULT),
        "0.10000000149011612"
    );
}
