//! `str` values built and run (decision 0022): literals and their escapes, f-strings, `copy()`,
//! `+`, comparisons, `in` and `len`.

use super::{baryon_build, package, stderr};
use std::process::Command;

/// `str` values end to end (decision 0022): literals held in variables and fields, returned,
/// borrowed and printed, and a `str` field of an error that ends `main` in its report.
#[test]
fn text_values_build_and_run() {
    let program = "\
class Probe(Struct):
    name: str
    value: f64


class MissingError(Exception):
    name: str


def size(n: i64) -> str:
    if n > 2:
        return \"big\"
    return \"small\"


def show(probe: Probe):
    print(probe.name, probe.value)


def main() -> Raises[None, MissingError]:
    name = \"heat\"
    print(name, size(3), size(1))
    show(Probe(\"energy\", 2.5))
    raise MissingError(\"pressure\")
";
    let directory = package("text", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "heat big small\nenergy 2.5\n"
    );
    assert_eq!(stderr(&ran), "error: MissingError(name='pressure')\n");
    assert_eq!(ran.status.code(), Some(1));
}

/// f-strings, `copy()` and `+` build their text through the runtime's string builder and print
/// what Python prints for the same program, and every text they build is freed.
#[test]
fn f_strings_format_as_python_does() {
    let program = "\
def describe(step: i64, name: str, energy: f64) -> str:
    return f\"step {step:4d}: {name} = {energy:10.4f} {{ok}}\"


def main():
    line = describe(7, \"argon\", -1.25)
    print(line)
    copied = line.copy()
    print(copied + \"!\")
    flag = True
    small: f32 = 0.1
    count = 255
    print(f\"{flag} {flag:>3} {small} {count:#x} {1234567:,} {-0.5:+.2%} {'mid':^7}|\")
";
    let directory = package("f-strings", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "\
step    7: argon =    -1.2500 {ok}
step    7: argon =    -1.2500 {ok}!
True   1 0.1 0xff 1,234,567 -50.00%   mid  |
"
    );
    assert_eq!(ran.status.code(), Some(0));
}

/// Backslash escapes in strings and f-string text, read as Python reads them, and a raw string
/// that keeps them. Before this, any backslash was reported as not lowered.
#[test]
fn escapes_build_and_run() {
    let program = r#"
def main():
    print("a\tb|\"q\"|\\|\x41\u00e9|\d")
    x = 2.5
    print(f"x =\t{x:.1f}\n{{x}}")
    print(r"C:\new", """say "hi" """)
"#;
    let directory = package("escapes", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "a\tb|\"q\"|\\|A\u{e9}|\\d\nx =\t2.5\n{x}\nC:\\new say \"hi\" \n"
    );
}

/// Comparisons, `in` and `len` on `str` (roadmap 1.13.2) print what CPython prints for the same
/// program: order by code point with a prefix first, so `"z" > "Ångström"` is false; the empty
/// string in every string; `len` counting characters, not bytes; and chains reading each string
/// once, including ones a call returns.
#[test]
fn comparisons_in_and_len_of_strings_build_and_run() {
    let program = r#"
def label(n: int) -> str:
    return f"run{n}"


def main():
    name = "argon"
    other = name.copy()
    print(name == other, name != "neon", name < "neon", "b" <= "a", "z" > "Ångström", "" >= "")
    print("go" in name, "rg" in name, "" in name, "x" not in name, name in "argon gas")
    print(len(name), len("Ångström"), len(""), len(label(42)), label(1) == "run1")
    print("a" < "b" < "c", "a" < "c" < "b", label(2) == label(2) != "x")
"#;
    let directory = package("text-comparisons", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "True True True False False True\nTrue True True True True\n5 8 0 5 True\nTrue False True\n"
    );
}

/// A loop variable that borrows each `str` can also hold its own value outside the loop
/// (roadmap 1.13.9): assigned before the loop, again after it, and copied inside it, through
/// plain, nested, `enumerate` and `dict` loops. Each value prints once and is freed once.
#[test]
fn loop_variables_hold_their_own_value_outside_the_loop() {
    let program = "\
def main():
    names = [\"ada\", \"bo\"]
    name = \"none\"
    print(name)
    for name in names:
        print(name)
    name = \"after\"
    print(name)
    last = \"\"
    for group in [names.copy(), [\"cy\"]]:
        for name in group:
            last = name.copy()
        print(last)
    for i, word in enumerate(names):
        print(i, word)
    word = \"z\"
    print(word)
    for key in {\"x\": 1}:
        print(key)
    key = \"q\"
    print(key)
";
    let directory = package("loop-variables-outside", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert!(
        ran.status.success(),
        "{}",
        String::from_utf8_lossy(&ran.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "none\nada\nbo\nafter\nbo\ncy\n0 ada\n1 bo\nz\nx\nq\n"
    );
}
