//! Unit tests of the helpers that need no database. The driver's tests lower whole programs,
//! since only an application may wire the parser, resolver and checker in front of lowering.

use crate::constants::integer;
use crate::escapes::{formatted, string};

#[test]
fn integer_literals_read_every_base() {
    assert_eq!(integer("42"), Some(42));
    assert_eq!(integer("0xff"), Some(255));
    assert_eq!(integer("0o17"), Some(15));
    assert_eq!(integer("0b101"), Some(5));
    assert_eq!(integer("0xzz"), None);
}

#[test]
fn string_literals_read_their_escapes() {
    assert_eq!(string("\"done\"").as_deref(), Some("done"));
    assert_eq!(string("'x'").as_deref(), Some("x"));
    assert_eq!(string("\"it's\"").as_deref(), Some("it's"));
    assert_eq!(string("\"a\\tb\\n\"").as_deref(), Some("a\tb\n"));
    assert_eq!(string("'\\'\\\\\\\"'").as_deref(), Some("'\\\""));
    assert_eq!(
        string("\"\\x41\\u00e9\\U0001F600\\101\\0\"").as_deref(),
        Some("A\u{e9}\u{1F600}A\0")
    );
    assert_eq!(string("\"\\d\"").as_deref(), Some("\\d"));
    assert_eq!(string("\"a\\\nb\"").as_deref(), Some("ab"));
    assert_eq!(string("r\"\\n\\\"\"").as_deref(), Some("\\n\\\""));
    assert_eq!(string("u'x'").as_deref(), Some("x"));
    assert_eq!(
        string("\"\"\"say \"hi\"\n\"\"\"").as_deref(),
        Some("say \"hi\"\n")
    );
    assert_eq!(string("\"\"").as_deref(), Some(""));
    assert_eq!(string("\"\\N{DEGREE SIGN}\""), None);
    assert_eq!(string("\"\\x4\""), None);
    assert_eq!(string("b\"x\""), None);
    assert_eq!(string("f\"{x}\""), None);
    assert_eq!(string("\"a\" \"b\""), None);
}

#[test]
fn f_string_text_reads_braces_and_escapes() {
    assert_eq!(formatted("{{x}}\\t", false).as_deref(), Some("{x}\t"));
    assert_eq!(formatted("\\x7b\\x7b", false).as_deref(), Some("{{"));
    assert_eq!(formatted("\\t", true).as_deref(), Some("\\t"));
}
