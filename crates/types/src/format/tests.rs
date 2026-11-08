//! Tests of parsing, writing and checking format specifications.

use super::*;

/// The parsed specification of `text`, which must be valid.
fn spec(text: &str) -> FormatSpec {
    FormatSpec::parse(text).unwrap()
}

#[test]
fn parses_every_part_in_order() {
    let parsed = spec("*^+#012,.3f");
    assert_eq!(parsed.fill, '*');
    assert_eq!(parsed.align, Some(Align::Center));
    assert_eq!(parsed.sign, Sign::Always);
    assert!(parsed.alternate && parsed.zero);
    assert_eq!(parsed.width, Some(12));
    assert_eq!(parsed.grouping, Some(','));
    assert_eq!(parsed.precision, Some(3));
    assert_eq!(parsed.presentation, Presentation::Fixed);
}

#[test]
fn a_specification_writes_back_as_written() {
    for text in ["", "*^+#012,.3f", " >5", "<8", "_x", ".6f", "+.6f", "%"] {
        let written = spec(text).to_string();
        assert_eq!(spec(&written), spec(text), "{text}");
    }
    assert_eq!(spec("*^+#012,.3f").to_string(), "*^+#012,.3f");
    assert_eq!(spec("<8").to_string(), " <8");
}

#[test]
fn the_empty_specification_is_the_default() {
    assert_eq!(spec(""), FormatSpec::default());
}

#[test]
fn the_examples_specifications_parse() {
    for text in [
        ".6f", "10.4f", "9.4f", "5.1f", "4d", "+.6f", ".3e", "_x", "<8", "=+08.2f",
    ] {
        assert!(FormatSpec::parse(text).is_ok(), "{text}");
    }
    assert_eq!(spec("<").align, Some(Align::Left));
    assert_eq!(spec("0>5").fill, '0');
    assert_eq!(spec(" d").sign, Sign::Space);
}

#[test]
fn malformed_and_unsupported_specifications_are_rejected() {
    let invalid = |text: &str| FormatError::Invalid(text.to_owned());
    assert_eq!(FormatSpec::parse(".f"), Err(invalid(".f")));
    assert_eq!(FormatSpec::parse("10.4q"), Err(invalid("10.4q")));
    assert_eq!(FormatSpec::parse("ff"), Err(invalid("ff")));
    assert_eq!(
        FormatSpec::parse("99999999999"),
        Err(invalid("99999999999"))
    );
    assert_eq!(
        FormatSpec::parse("n").unwrap_err().to_string(),
        "format code `n` is not supported yet"
    );
    assert_eq!(
        FormatSpec::parse("z.2f").unwrap_err().to_string(),
        "the `z` flag is not supported yet"
    );
}

#[test]
fn each_kind_of_value_allows_what_python_allows() {
    let message = |text: &str, class| spec(text).check(class).unwrap_err().to_string();
    assert_eq!(
        message("d", FormatClass::Float),
        "format code `d` cannot format a float"
    );
    assert_eq!(
        message(".2", FormatClass::Integer),
        "a precision cannot format an integer"
    );
    assert_eq!(
        message("+", FormatClass::Text),
        "a sign cannot format a `str`"
    );
    assert_eq!(
        message("=10", FormatClass::Text),
        "`=` alignment cannot format a `str`"
    );
    assert_eq!(
        message(",x", FormatClass::Integer),
        "the grouping `,` with format code `x` cannot format an integer"
    );
    assert_eq!(
        message("s", FormatClass::Bool),
        "format code `s` cannot format a `bool`"
    );
    for (text, class) in [
        (".2f", FormatClass::Integer),
        ("_x", FormatClass::Integer),
        (",", FormatClass::Integer),
        (".3g", FormatClass::Float),
        ("#.0f", FormatClass::Float),
        ("^20.5", FormatClass::Text),
        (">5", FormatClass::Bool),
        ("08", FormatClass::Text),
    ] {
        assert!(spec(text).check(class).is_ok(), "{text}");
    }
}
