//! Unit tests of the helpers that need no database. The driver's tests lower whole programs,
//! since only an application may wire the parser, resolver and checker in front of lowering.

use crate::constants::{integer, string};

#[test]
fn integer_literals_read_every_base() {
    assert_eq!(integer("42"), Some(42));
    assert_eq!(integer("0xff"), Some(255));
    assert_eq!(integer("0o17"), Some(15));
    assert_eq!(integer("0b101"), Some(5));
    assert_eq!(integer("0xzz"), None);
}

#[test]
fn only_plain_strings_are_read() {
    assert_eq!(string("\"done\"").as_deref(), Some("done"));
    assert_eq!(string("'x'").as_deref(), Some("x"));
    assert_eq!(string("\"a\\n\""), None);
    assert_eq!(string("f\"{x}\""), None);
    assert_eq!(string("\"a\" \"b\""), None);
}
