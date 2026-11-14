//! The ownership sweep (roadmap 1.18.2): every kind of value that owns memory, crossed with every
//! place a program can own, borrow, move or drop one, built and run under Valgrind.
//!
//! Each hand-written test elsewhere checks one program, and its values are often string
//! literals, which are never freed (decision 0022), so a double free or a use after free of a
//! literal shows nothing. The sweep closes that gap by generating the programs instead:
//!
//! - A *kind* is a type that owns memory, such as `str` built at run time, `list[str]`, a dict,
//!   an `Array`, a struct with a `str` and a list, a union of structs or a tuple. Every value a
//!   kind makes is new, built from a number, never a literal.
//! - A *site* is one place values go through: a temporary borrowed by a call, a conditional
//!   expression, a loop over a temporary list, an `Owned` parameter, a `raise` with locals alive,
//!   and so on. Sites are Baryon functions in `tests/fixtures/ownership/`, written once against
//!   the placeholder type `ITEM`.
//!
//! Each kind gets one program: the prelude, the kind's `show`, `size` and `make`, and every site
//! with `ITEM` replaced by the kind's type, each called from `main` after printing its name. One
//! program per kind, not per site, keeps the sweep to twelve builds.
//!
//! A site the compiler does not build yet for a kind, such as `copy()` of a list of structs, is
//! reported by `baryon build` as not lowered or as a type error. The test drops each site the
//! build names and builds again, so the sweep grows by itself as the compiler builds more. A
//! site in [`KNOWN`] fails today with a bug that has an owner; it is left out until the fix
//! lands, and the fix deletes its entry.
//!
//! The program then runs under Valgrind when `valgrind` is on the search path (Linux), which
//! must report no invalid read, write or free, no use of uninitialised memory and no memory
//! definitely lost. Without Valgrind (macOS) the program still runs, and must exit with status 0
//! and print every site's name, so a double free that the system allocator catches fails here
//! too.

use super::{baryon_build, package, stderr};
use std::io;
use std::process::Command;

/// A kind of value that owns memory, and how the sweep builds, reads and measures one.
struct Kind {
    /// The kind's name, which names its test's scratch package.
    name: &'static str,
    /// The type that replaces `ITEM` in the sites.
    annotation: &'static str,
    /// An expression for a new value, with `@` where the number it is built from goes.
    new: &'static str,
    /// The body of `show(v)`, which reads the value through every pointer it holds, one line per
    /// entry, indented by the test.
    show: &'static [&'static str],
    /// The expression `size(v)` returns.
    size: &'static str,
}

/// Every kind the sweep covers: text, lists, dicts and arrays, structs, unions and tuples that
/// hold them, an optional, and containers nested two deep.
const KINDS: &[Kind] = &[
    Kind {
        name: "str",
        annotation: "str",
        new: r#"f"s{@}""#,
        show: &["print(v)"],
        size: "len(v)",
    },
    Kind {
        name: "list-str",
        annotation: "list[str]",
        new: r#"[f"a{@}", f"b{@}"]"#,
        show: &["print(len(v), v[0], v[-1])"],
        size: "len(v)",
    },
    Kind {
        name: "list-int",
        annotation: "list[int]",
        new: "[@, @ + 1]",
        show: &["print(len(v), v[0])"],
        size: "len(v)",
    },
    Kind {
        name: "dict",
        annotation: "dict[str, str]",
        new: r#"{f"k{@}": f"v{@}"}"#,
        show: &["for key in v:", "    print(key, v[key])"],
        size: "len(v)",
    },
    Kind {
        name: "array",
        annotation: "Array[f64, 1]",
        new: "Array.ones(@ + 2)",
        show: &["print(v.sum())"],
        size: "len(v)",
    },
    Kind {
        name: "struct",
        annotation: "Rec",
        new: r#"Rec(f"r{@}", [f"t{@}"])"#,
        show: &["print(v.name, len(v.tags), v.tags[0])"],
        size: "len(v.tags)",
    },
    Kind {
        name: "union",
        annotation: "Shape",
        new: "make_shape(@)",
        show: &[
            "match v:",
            "    case Circle():",
            "        print(\"circle\")",
            "    case Square(count):",
            "        print(count)",
        ],
        size: "1",
    },
    Kind {
        name: "optional",
        annotation: "str | None",
        new: r#"f"o{@}""#,
        show: &[
            "if v is not None:",
            "    print(v)",
            "else:",
            "    print(\"none\")",
        ],
        size: "1",
    },
    Kind {
        name: "nested-list",
        annotation: "list[list[str]]",
        new: r#"[[f"n{@}"], [f"m{@}", f"p{@}"]]"#,
        show: &["print(len(v), v[1][1])"],
        size: "len(v)",
    },
    Kind {
        name: "dict-of-lists",
        annotation: "dict[str, list[str]]",
        new: r#"{f"d{@}": [f"e{@}"]}"#,
        show: &["for key in v:", "    print(key, v[key][0])"],
        size: "len(v)",
    },
    Kind {
        name: "list-of-structs",
        annotation: "list[Rec]",
        new: r#"[Rec(f"l{@}", [f"u{@}"])]"#,
        show: &["print(v[0].name, v[0].tags[0])"],
        size: "len(v)",
    },
    Kind {
        name: "tuple",
        annotation: "tuple[str, list[str]]",
        new: r#"(f"t{@}", [f"w{@}"])"#,
        show: &["print(v[0], v[1][0])"],
        size: "len(v[1])",
    },
];

/// The types and helpers every site uses.
const PRELUDE: &str = include_str!("../fixtures/ownership/prelude.byn");

/// The sites, one file per theme.
const SITES: &[&str] = &[
    include_str!("../fixtures/ownership/values.byn"),
    include_str!("../fixtures/ownership/loops.byn"),
    include_str!("../fixtures/ownership/containers.byn"),
    include_str!("../fixtures/ownership/calls.byn"),
    include_str!("../fixtures/ownership/errors.byn"),
    include_str!("../fixtures/ownership/managers.byn"),
];

/// Sites that fail today for every kind, each with the bug and who owns its fix. They are left
/// out of the sweep until the fix lands; the pull request with the fix deletes the entry, and
/// the sweep then checks the site for every kind. Empty while no bug the sweep found is open.
const KNOWN: &[(&str, &str)] = &[];

/// One site: a `# site:` block of a sites file.
struct Site {
    /// The name of the function `main` calls.
    name: String,
    /// Whether the site does not use `ITEM`, so it runs only in the first kind's program.
    once: bool,
    /// The block's text: its comments, helpers and the site function.
    text: String,
}

/// Every site in [`SITES`], in file order. A block starts at a line `# site: <name>`, followed
/// by ` once` for a site that does not depend on the kind.
fn sites() -> Vec<Site> {
    let mut sites: Vec<Site> = Vec::new();
    for file in SITES {
        for line in file.lines() {
            if let Some(marker) = line.strip_prefix("# site: ") {
                let mut words = marker.split_whitespace();
                let name = words.next().unwrap_or_default().to_owned();
                let once = words.next() == Some("once");
                sites.push(Site {
                    name,
                    once,
                    text: String::new(),
                });
            }
            if let Some(site) = sites.last_mut() {
                site.text.push_str(line);
                site.text.push('\n');
            }
        }
    }
    sites
}

/// The program sweeping `kind` through `sites`, and the first line of each site's block in it,
/// so a build error's line names the site it is in.
fn program(kind: &Kind, sites: &[&Site]) -> (String, Vec<(usize, String)>) {
    let new = |number: &str| kind.new.replace('@', number);
    let mut lines = vec![format!("def show(v: {}):", kind.annotation)];
    lines.extend(kind.show.iter().map(|line| format!("    {line}")));
    lines.extend([
        String::new(),
        String::new(),
        format!("def size(v: {}) -> int:", kind.annotation),
        format!("    return {}", kind.size),
        String::new(),
        String::new(),
        format!("def make(k: int) -> {}:", kind.annotation),
        format!("    return {}", new("k")),
        String::new(),
    ]);
    lines.extend(PRELUDE.lines().map(str::to_owned));
    let mut starts = Vec::with_capacity(sites.len());
    for site in sites {
        lines.extend([String::new(), String::new()]);
        starts.push((lines.len() + 1, site.name.clone()));
        lines.extend(site.text.lines().map(str::to_owned));
    }
    lines.extend([String::new(), String::new(), "def main():".to_owned()]);
    for site in sites {
        lines.push(format!("    print(\"-- {}\")", site.name));
        lines.push(format!("    {}()", site.name));
    }
    lines.push(String::new());
    let text = lines
        .join("\n")
        .replace("ITEM", kind.annotation)
        .replace("NEW1", &new("1"))
        .replace("NEW2", &new("2"))
        .replace("NEW3", &new("3"));
    (text, starts)
}

/// The names of the sites that the build errors in `errors` point into: each `--> main.byn:L:C`
/// line, mapped to the last site starting at or before line `L`.
fn sites_named(errors: &str, starts: &[(usize, String)]) -> Vec<String> {
    let mut named: Vec<String> = Vec::new();
    for line in errors.lines() {
        let Some(location) = line.trim_start().strip_prefix("--> main.byn:") else {
            continue;
        };
        let Some(number) = location
            .split(':')
            .next()
            .and_then(|number| number.parse::<usize>().ok())
        else {
            continue;
        };
        if let Some((_, name)) = starts.iter().rev().find(|(start, _)| *start <= number)
            && !named.contains(name)
        {
            named.push(name.clone());
        }
    }
    named
}

/// The fewest sites a kind's program may build. Most kinds build about 90 of the 108; the
/// optional builds about 50, since borrowing a `str | None` is not built yet. Fewer means the
/// harness, not the compiler, has gone wrong.
const MINIMUM_BUILT: usize = 40;

/// Whether Valgrind can run here.
fn valgrind() -> bool {
    Command::new("valgrind")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Sweeps `kind`: builds its program, dropping each site the compiler does not build yet, then
/// runs it, under Valgrind where there is one, and checks that every site ran and nothing was
/// read after it was freed, freed twice, read uninitialised or leaked.
///
/// # Errors
///
/// Fails when the scratch package cannot be written or a program cannot be started.
///
/// # Panics
///
/// Panics, failing the test, when a site fails or the harness has dropped so many sites that
/// it, not the compiler, has gone wrong.
fn sweep(index: usize) -> io::Result<()> {
    let kind = &KINDS[index];
    let all = sites();
    let mut kept: Vec<&Site> = all
        .iter()
        .filter(|site| !site.once || index == 0)
        .filter(|site| KNOWN.iter().all(|(name, _)| *name != site.name))
        .collect();
    let mut skipped: Vec<String> = Vec::new();
    let directory = package(&format!("ownership-{}", kind.name), &[])?;
    let source = directory.join("package").join("main.byn");
    std::fs::create_dir_all(directory.join("package"))?;
    let program = loop {
        let (text, starts) = program(kind, &kept);
        std::fs::write(&source, text)?;
        let (output, program) = baryon_build(&directory)?;
        if output.status.success() {
            break program;
        }
        let errors = stderr(&output);
        let named = sites_named(&errors, &starts);
        assert!(
            !named.is_empty(),
            "the `{}` sweep failed outside any site:\n{errors}",
            kind.name
        );
        kept.retain(|site| !named.contains(&site.name));
        skipped.extend(named);
    };
    let checked = valgrind();
    let ran = if checked {
        Command::new("valgrind")
            .args([
                "--quiet",
                "--error-exitcode=99",
                "--leak-check=full",
                "--errors-for-leak-kinds=definite",
                "--show-leak-kinds=definite",
            ])
            .arg(&program)
            .output()?
    } else {
        Command::new(&program).output()?
    };
    let printed = String::from_utf8_lossy(&ran.stdout);
    assert_eq!(
        ran.status.code(),
        Some(0),
        "the `{}` sweep failed{}:\n{}",
        kind.name,
        if checked { " under Valgrind" } else { "" },
        stderr(&ran)
    );
    let ran_sites: Vec<&str> = printed
        .lines()
        .filter_map(|line| line.strip_prefix("-- "))
        .collect();
    let expected: Vec<&str> = kept.iter().map(|site| site.name.as_str()).collect();
    assert_eq!(
        ran_sites, expected,
        "the `{}` sweep stopped early",
        kind.name
    );
    assert!(
        kept.len() >= MINIMUM_BUILT,
        "the `{}` sweep built only {} sites; skipped: {skipped:?}",
        kind.name,
        kept.len()
    );
    Ok(())
}

/// One test per kind, so the kinds build and run in parallel.
macro_rules! sweeps {
    ($($test:ident => $index:expr,)*) => {
        $(
            #[doc = concat!("Sweeps the kind at position ", stringify!($index), " of [`KINDS`].")]
            #[test]
            fn $test() {
                sweep($index).unwrap();
            }
        )*
    };
}

sweeps! {
    ownership_of_str => 0,
    ownership_of_list_of_str => 1,
    ownership_of_list_of_int => 2,
    ownership_of_dict => 3,
    ownership_of_array => 4,
    ownership_of_struct => 5,
    ownership_of_union => 6,
    ownership_of_optional => 7,
    ownership_of_nested_list => 8,
    ownership_of_dict_of_lists => 9,
    ownership_of_list_of_structs => 10,
    ownership_of_tuple => 11,
}

/// The kinds and the test functions agree: one test per kind.
#[test]
fn every_kind_has_a_test() {
    assert_eq!(KINDS.len(), 12);
    assert!(sites().len() > 100);
    assert!(
        KNOWN
            .iter()
            .all(|(name, _)| sites().iter().any(|site| site.name == *name)),
        "a known failure names no site"
    );
}
