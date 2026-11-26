//! `.copy()` on a value whose type is a type parameter (roadmap 1.9.60, decision 0066, cards 51
//! and 70), through the type checker: what checks clean, and the call that fixes `T` to a type
//! with no copy, or to one not copied through a type parameter yet.
//!
//! These tests live in the driver because they need several implementations, and only an
//! application crate may depend on more than one (decision 0001). The build tests in
//! `tests/build/parameter_copies.rs` run the copies.

use driver::Session;

/// Checks `main.byn` and returns every diagnostic, rendered and joined by blank lines. An empty
/// string means the program is clean. The program's first newline is dropped.
fn check_one(text: &str) -> String {
    let mut session = Session::new();
    if let Err(error) = session.add_file("main.byn", text.strip_prefix('\n').unwrap_or(text)) {
        return error.to_string();
    }
    session
        .check()
        .iter()
        .map(|diagnostic| diagnostics::render(diagnostic, &session).to_string())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// `.copy()` on a `T`, bound or not, checks clean, and so does fixing `T` to each type a
/// generic function copies: copy types, `str`, lists and arrays of those, and a struct with its
/// own `copy`.
#[test]
fn copies_of_a_type_parameter_check_clean() {
    let program = r#"
class Tag(Struct):
    label: str

    def copy(self) -> Tag:
        return Tag(self.label.copy())


def same[T](x: T) -> T:
    return x.copy()


def scaled[T: Number](x: T) -> T:
    return x.copy() * 2


def last[T](xs: list[T]) -> T:
    return xs[len(xs) - 1].copy()


def main():
    print(same(1), same(2.5), same(True), same("a"), same([1, 2]), same(["a"]))
    print(same(Array([1.0, 2.0])), same(Tag("t")).label, scaled(3), last([1.5]))
"#;
    assert_eq!(check_one(program), "");
}

/// A call that fixes `T` to a type with no copy, or to one with a copy that is not built through
/// a type parameter yet, is an error at the call, naming the type.
#[test]
fn types_a_type_parameter_cannot_copy_are_reported_at_the_call() {
    let program = r#"
class Plain(Struct):
    x: f64


def last[T](xs: list[T]) -> T:
    return xs[len(xs) - 1].copy()


def same[T](x: T) -> T:
    return x.copy()


def main():
    print(last([([1.0], "a")]))
    print(same({"a": 1}))
    print(same(Plain(1.0)).x)
    print(same([[1, 2]]))
"#;
    assert_eq!(
        check_one(program),
        "error: `last` calls `.copy()` on `T`, and `tuple[list[f64], str]` has no copy
  --> main.byn:14:11
   |
14 |     print(last([([1.0], \"a\")]))
   |           ^^^^^^^^^^^^^^^^^^^^ a type with no copy
   = note: a type parameter can be copied when it is a copy type, a `str`, a list or array of \
those, or a struct with its own `copy` method

error: `same` copies its `T`, `dict[str, i64]` here, which cannot be copied through a type \
parameter yet
  --> main.byn:15:11
   |
15 |     print(same({\"a\": 1}))
   |           ^^^^^^^^^^^^^^ copied through a type parameter
   = note: copying this type through a type parameter is idea I.76

error: `same` calls `.copy()` on `T`, and `Plain` has no copy
  --> main.byn:16:11
   |
16 |     print(same(Plain(1.0)).x)
   |           ^^^^^^^^^^^^^^^^ a type with no copy
   = note: a type parameter can be copied when it is a copy type, a `str`, a list or array of \
those, or a struct with its own `copy` method

error: `same` copies its `T`, `list[list[i64]]` here, which cannot be copied through a type \
parameter yet
  --> main.byn:17:11
   |
17 |     print(same([[1, 2]]))
   |           ^^^^^^^^^^^^^^ copied through a type parameter
   = note: copying this type through a type parameter is idea I.76"
    );
}
