//! `str` values built at run time, end to end (decision 0022): f-strings, `copy()` and `+`
//! lowered to the mid-level IR, and the checker's diagnostics for a format specification that
//! does not fit its value.

use driver::Session;

/// The IR text of the function `name` in `program`, or the program's diagnostics, rendered and
/// joined by blank lines, when it has any.
fn lower(program: &str, name: &str) -> String {
    let mut session = Session::new();
    if let Err(error) = session.add_file("main.byn", program) {
        return error.to_string();
    }
    let mut errors = session.check();
    if errors.is_empty() {
        match session.mir(name) {
            None => return format!("no function `{name}`"),
            Some(Ok(text)) => return text,
            Some(Err(found)) => errors = found,
        }
    }
    errors
        .iter()
        .map(|error| diagnostics::render(error, &session).to_string())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Checks `program` and returns its diagnostics, rendered and joined by blank lines.
fn check(program: &str) -> String {
    let mut session = Session::new();
    if let Err(error) = session.add_file("main.byn", program) {
        return error.to_string();
    }
    session
        .check()
        .iter()
        .map(|diagnostic| diagnostics::render(diagnostic, &session).to_string())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// An f-string becomes one `format` rvalue with its literal pieces and each value's checked
/// specification, doubled braces become single ones, and `copy()` and `+` are `format` too.
#[test]
fn f_strings_copies_and_joins_lower_to_format() {
    let program = "\
        def report(step: i64, name: str, energy: f64) -> str:\n\
        \x20   line = f\"step {step:4d}: {name} = {energy:10.4f} {{ok}}\"\n\
        \x20   copied = line.copy()\n\
        \x20   return copied + \"!\"\n";
    assert_eq!(
        lower(program, "report"),
        "\
fn(_1: i64, _2: str, _3: f64) -> str {
    let _0: str; // return
    let _1: i64; // parameter step
    let _2: str; // parameter name
    let _3: f64; // parameter energy
    let _4: str; // line
    let _5: str; // copied
    let _6: str;
    let _7: str;
    let _8: str;

    bb0: {
        _6 = format(\"step \", {_1:4d}, \": \", {_2:}, \" = \", {_3:10.4f}, \" {ok}\");
        drop(_4);
        _4 = move _6;
        _7 = format({_4:});
        drop(_5);
        _5 = move _7;
        _8 = \"!\";
        _0 = format({_5:}, {_8:});
        drop(_8);
        drop(_5);
        drop(_4);
        return;
    }
}
"
    );
}

/// A specification that does not fit its value's type is reported at the specification, with
/// the type it was asked to format.
#[test]
fn specifications_that_do_not_fit_their_values_are_reported() {
    let program = "\
        def show(name: str, count: i64, flag: bool) -> str:\n\
        \x20   return f\"{name:+} {count:.2} {flag:s} {name:q}\"\n";
    let note = "  = note: an f-string's format specification follows Python's format mini-language";
    let line = "2 |     return f\"{name:+} {count:.2} {flag:s} {name:q}\"";
    assert_eq!(
        check(program),
        format!(
            "\
error: a sign cannot format a `str`
 --> main.byn:2:20
  |
{line}
  |                    ^ formats a value of type `str`
{note}

error: a precision cannot format an integer
 --> main.byn:2:30
  |
{line}
  |                              ^^ formats a value of type `i64`
{note}

error: format code `s` cannot format a `bool`
 --> main.byn:2:40
  |
{line}
  |                                        ^ formats a value of type `bool`
{note}

error: `q` is not a format specification
 --> main.byn:2:49
  |
{line}
  |                                                 ^ formats a value of type `str`
{note}"
        )
    );
}
