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

#[test]
fn probe() {
    let program = "\
        def report(step: i64, name: str, energy: f64) -> str:\n\
        \x20   line = f\"step {step:4d}: {name} = {energy:10.4f} {{ok}}\"\n\
        \x20   copied = line.copy()\n\
        \x20   return copied + \"!\"\n";
    println!("{}", lower(program, "report"));
    let bad = "\
        def show(name: str, count: i64, flag: bool) -> str:\n\
        \x20   return f\"{name:+} {count:.2} {flag:s} {name:q}\"\n";
    println!("{}", lower(bad, "show"));
    panic!();
}
