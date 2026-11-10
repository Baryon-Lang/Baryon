//! `str`'s indexing, slicing and methods built and run (roadmap 1.13.12): each program prints
//! what Python prints for the same source.

use super::examples::prints;
use super::{baryon_build, package, panic_report};
use std::process::Command;

/// Reading a line of an input file: strip it, split it into fields, find and replace, change
/// case, test its start and end, index and slice it by character, and join the fields back.
#[test]
fn text_methods_parse_a_line_as_python_does() {
    let program = "\
def main():
    line = \"  argon, 39.948 ,Ar  \\n\"
    clean = line.strip()
    print(f\"[{clean}]\")
    fields = clean.split(\",\")
    print(len(fields), fields)
    print(fields[1].strip(), fields[2].lower(), fields[0].upper())
    print(\"a  b\\tc \".split(), \"\".split(), \",\".split(\",\"))
    print(f\"[{line.lstrip()}]\", f\"[{line.rstrip()}]\", \"xxhexx\".strip(\"x\"))
    print(\"-\".join(fields), \"\".join([]), \", \".join([\"a\"]))
    print(clean.startswith(\"arg\"), clean.endswith(\"Ar\"), clean.endswith(\"x\"))
    print(clean.find(\"39\"), clean.find(\"\"), clean.find(\"neon\"))
    print(clean.replace(\" \", \"\"), \"ab\".replace(\"\", \"-\"))
    name = \"Ångström\"
    print(name[0], name[-1], name[5], name[1:4], name[::2], name[::-1], name[:0])
    for i in range(len(name)):
        print(name[i], end=\"\")
    print()
    print(\"Straße\".upper(), \"ÅNGSTRÖM\".lower())
";
    prints(
        "text-methods",
        program,
        "\
[argon, 39.948 ,Ar]
3 ['argon', ' 39.948 ', 'Ar']
39.948 ar ARGON
['a', 'b', 'c'] [] ['', '']
[argon, 39.948 ,Ar  
] [  argon, 39.948 ,Ar] he
argon- 39.948 -Ar  a
True True False
7 0 -1
argon,39.948,Ar -a-b-
Å m r ngs Ågtö mörtsgnÅ 
Ångström
STRASSE ångström
",
    )
    .unwrap();
}

/// The worked example of the reference's "Indexing, slicing and methods": a comprehension over
/// the pieces of a split, each stripped and put in upper case.
#[test]
fn the_reference_example_builds_and_runs() {
    let program = "\
def symbols(line: str) -> list[str]:
    return [entry.strip().upper() for entry in line.split(\",\")]


def main():
    print(symbols(\" ar, he ,NE\"))
";
    prints("text-symbols", program, "['AR', 'HE', 'NE']\n").unwrap();
}

/// An index outside a `str` panics as one outside a list does, naming the index and the
/// length in characters, and an empty separator panics with Python's `ValueError` message.
#[test]
fn text_indexes_and_separators_are_checked() {
    for (name, body, report) in [
        (
            "text-index",
            "    name = \"héat\"\n    i = 4\n    print(name[i])\n",
            "panic: index 4 is out of range for dimension 0 of length 4\n  --> main.byn:4:16\n",
        ),
        (
            "text-separator",
            "    sep = \"\"\n    print(\"a,b\".split(sep))\n",
            "panic: empty separator\n  --> main.byn:3:11\n",
        ),
    ] {
        let program = format!("def main():\n{body}");
        let directory = package(name, &[("main.byn", &program)]).unwrap();
        let (output, executable) = baryon_build(&directory).unwrap();
        assert!(output.status.success(), "{}", super::stderr(&output));
        let ran = Command::new(&executable).output().unwrap();
        assert_eq!(panic_report(&ran), report);
        assert_eq!(ran.status.code(), Some(101));
    }
}

/// `ValueError`, the standard library's own class (decision 0037), is raised, caught and read
/// like a program's error, and printed in Python's form when `main` lets one out.
#[test]
fn value_error_is_raised_caught_and_reported() {
    let program = "\
def positive(mass: f64) -> Raises[f64, ValueError]:
    if mass <= 0.0:
        raise ValueError(\"mass must be positive\")
    return mass


def main() -> Raises[None, ValueError]:
    try:
        positive(-1.0)
    except ValueError as error:
        print(error.message)
    positive(0.0)
";
    let directory = package("value-error", &[("main.byn", program)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", super::stderr(&output));
    let ran = Command::new(&executable).output().unwrap();
    assert_eq!(ran.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "mass must be positive\n"
    );
    assert_eq!(
        String::from_utf8_lossy(&ran.stderr),
        "error: ValueError(message='mass must be positive')\n"
    );
}
