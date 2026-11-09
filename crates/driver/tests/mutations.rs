//! Mutated programs never panic the compiler (roadmap 1.18.1).
//!
//! The test takes the example programs in `docs/examples/` and the front-end corpus, edits each
//! at random a few times (deletes, copies or swaps a line, or cuts a few characters and puts a
//! token in their place), and runs every result through the whole compiler short of `clang`:
//! [`Session::check`], then [`Session::mir`] for each function and [`Session::compile`] when the
//! program checks clean. Most edits give errors, which is fine; about one in ten still checks
//! and reaches lowering and the LLVM backend. What must never happen is a panic, because the
//! language server runs the same stages on every keystroke, and a panic there ends the user's
//! session.
//!
//! The edits come from a fixed seed, so a run is deterministic and a failure reproduces. The
//! test runs [`DEFAULT_MUTATIONS`] edits, under a second. Set `BARYON_MUTATIONS` to run more,
//! for example `BARYON_MUTATIONS=40000 cargo test --package driver --test mutations`, which
//! takes about a minute and found no panic when this test was written.

use driver::Session;
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

/// How many mutated programs a normal test run checks.
const DEFAULT_MUTATIONS: usize = 300;

/// Text an edit can put in place of the characters it cuts: punctuation, operators, literals,
/// keywords and the types and built-ins that reach the most compiler code.
const PIECES: &[&str] = &[
    ":",
    "(",
    ")",
    "[",
    "]",
    ",",
    "=",
    "+=",
    "-=",
    "*",
    "/",
    "//",
    "%",
    "**",
    "@",
    ".",
    "-",
    "~",
    "<<",
    "^",
    "|",
    "None",
    "0",
    "1",
    "-1",
    "1.0",
    "0.5",
    "True",
    "x",
    "i",
    "n",
    "def ",
    "return ",
    "if ",
    "else:",
    "for ",
    " in ",
    "range(",
    "prange(",
    "accumulate=",
    "simd(",
    "Fixed[f64, 3]",
    "Array[f64, 1]",
    "InOut[",
    "\n",
    "    ",
    "pass",
    "raise ",
    "match ",
    "case ",
    "while ",
    "break",
    "continue",
    "f\"{x}\"",
    "\"s\"",
    "[1, 2]",
    "(1, 2)",
    "not ",
    " and ",
    " or ",
    "is None",
    "Final",
    "i64",
    "f64",
    "u8",
    "bool",
    "str",
    "len(",
    ".copy()",
    ".sum()",
    "with unsafe:",
    "unchecked",
    "a[0]",
    "a[0:2]",
    "a[i, j]",
];

/// A xorshift64 generator: a fixed seed gives the same edits on every machine, with no
/// dependency.
struct Random(u64);

impl Random {
    /// The next 64 random bits.
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A number in `0..bound`, or 0 when `bound` is 0.
    fn below(&mut self, bound: usize) -> usize {
        let bound = u64::try_from(bound.max(1)).unwrap_or(u64::MAX);
        usize::try_from(self.next() % bound).unwrap_or(0)
    }
}

/// `text` after one to four random edits, each on one line.
fn mutate(text: &str, random: &mut Random) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    for _ in 0..=random.below(4) {
        if lines.is_empty() {
            break;
        }
        let line = random.below(lines.len());
        match random.below(6) {
            0 => {
                lines.remove(line);
            }
            1 => {
                let copy = lines[line].clone();
                let at = random.below(lines.len());
                lines.insert(at, copy);
            }
            2 => {
                let other = random.below(lines.len());
                lines.swap(line, other);
            }
            _ => {
                let characters: Vec<char> = lines[line].chars().collect();
                let start = random.below(characters.len() + 1);
                let end = (start + random.below(4)).min(characters.len());
                let piece = if random.below(3) == 0 {
                    ""
                } else {
                    PIECES[random.below(PIECES.len())]
                };
                let mut edited: String = characters[..start].iter().collect();
                edited.push_str(piece);
                edited.extend(&characters[end..]);
                lines[line] = edited;
            }
        }
    }
    lines.join("\n") + "\n"
}

/// Runs `text`, as the package's `main.byn`, through every stage that a clean check reaches.
/// The results do not matter, only that each stage returns.
fn compile(text: &str) {
    let mut session = Session::new();
    if session.add_file("main.byn", text).is_err() || !session.check().is_empty() {
        return;
    }
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("def ") {
            let name: String = rest
                .chars()
                .take_while(|&character| character.is_alphanumeric() || character == '_')
                .collect();
            let _ = session.mir(&name);
        }
    }
    let _ = session.compile(None, driver::Profile::Debug);
}

/// Every `.byn` file directly in `directory`, read in name order.
fn programs(directory: &Path) -> std::io::Result<Vec<String>> {
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "byn") {
            paths.push(path);
        }
    }
    paths.sort();
    paths.iter().map(std::fs::read_to_string).collect()
}

#[test]
fn mutated_programs_never_panic() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut sources = programs(&crates.join("../docs/examples")).unwrap();
    sources.extend(programs(&crates.join("front_end_tests/tests/corpus")).unwrap());
    assert!(!sources.is_empty());
    let count = std::env::var("BARYON_MUTATIONS")
        .ok()
        .and_then(|count| count.parse().ok())
        .unwrap_or(DEFAULT_MUTATIONS);

    let mut random = Random(0x9E37_79B9_7F4A_7C15);
    // The first program for each distinct panic message.
    let mut panics = BTreeMap::new();
    let mut total = 0;
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    for _ in 0..count {
        let source = &sources[random.below(sources.len())];
        let mutated = mutate(source, &mut random);
        if let Err(payload) = catch_unwind(AssertUnwindSafe(|| compile(&mutated))) {
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|&text| text.to_owned()))
                .unwrap_or_default();
            panics.entry(message).or_insert(mutated);
            total += 1;
        }
    }
    std::panic::set_hook(hook);

    let report: Vec<String> = panics
        .iter()
        .map(|(message, program)| format!("panic: {message}\nprogram:\n{program}"))
        .collect();
    assert!(
        panics.is_empty(),
        "{total} of {count} mutated programs panicked, with {} distinct messages:\n\n{}",
        panics.len(),
        report.join("\n")
    );
}
