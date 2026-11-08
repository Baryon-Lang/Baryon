//! The syntax check's test corpus, run through the real parser, and a check that every example
//! program uses only what the language supports.
//!
//! `tests/corpus/` holds small programs, each covering one group of constructs. Next to each
//! `name.byn` with unsupported constructs sits `name.errors`, the syntax check's diagnostics as
//! rendered text; a program the check accepts has no `.errors` file. Every corpus program must
//! parse without syntax errors, so each expected diagnostic comes from the syntax check alone.
//!
//! After an intended change to the messages, regenerate the expected files with
//!
//! ```sh
//! BLESS=1 cargo test --package front_end_tests --test corpus
//! ```
//!
//! and review the difference in version control like any other change.

use source::SourceMap;
use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// What a helper returns: its value, or the error that stops the test.
type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// The source file extension, fixed by decision 0023.
const EXTENSION: &str = "byn";

/// The directory of this crate.
fn crate_directory() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Every source file directly inside `directory`, sorted by name.
fn programs(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_some_and(|found| found == EXTENSION) {
            files.push(path);
        }
    }
    files.sort();
    if files.is_empty() {
        return Err(format!("no programs in {}", directory.display()).into());
    }
    Ok(files)
}

/// Parses and checks `path`. Returns the parser's rendered diagnostics and the syntax check's,
/// each diagnostic followed by a blank line.
fn check(path: &Path) -> Result<(String, String)> {
    let text = fs::read_to_string(path)?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut sources = SourceMap::new();
    let file = sources.add(name, text.clone())?;
    let parsed = parser::parse(file, &text);
    let mut syntax_errors = String::new();
    for diagnostic in parsed.diagnostics() {
        writeln!(
            syntax_errors,
            "{}\n",
            diagnostics::render(diagnostic, &sources)
        )?;
    }
    let mut unsupported = String::new();
    for diagnostic in syntax_check::check(parsed.tree(), &text) {
        writeln!(
            unsupported,
            "{}\n",
            diagnostics::render(&diagnostic, &sources)
        )?;
    }
    Ok((syntax_errors, unsupported))
}

/// Compares `actual` with the contents of `expected`, or writes it there when `BLESS` is set.
/// An empty `actual` means the file should not exist. Returns a description of any difference.
fn compare(expected: &Path, actual: &str) -> Result<Option<String>> {
    if std::env::var_os("BLESS").is_some() {
        if actual.is_empty() {
            let _ = fs::remove_file(expected);
        } else {
            fs::write(expected, actual)?;
        }
        return Ok(None);
    }
    let found = fs::read_to_string(expected).unwrap_or_default();
    Ok((found != actual).then(|| {
        format!(
            "{} differs from the syntax check's output. Expected:\n{found}\nActual:\n{actual}",
            expected.display()
        )
    }))
}

#[test]
fn corpus_matches_expected_diagnostics() {
    let mut failures = Vec::new();
    for path in programs(&crate_directory().join("tests/corpus")).unwrap() {
        let (syntax_errors, unsupported) = check(&path).unwrap();
        assert!(
            syntax_errors.is_empty(),
            "{} has syntax errors:\n{syntax_errors}",
            path.display()
        );
        failures.extend(compare(&path.with_extension("errors"), &unsupported).unwrap());
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_example_program_passes_the_syntax_check() {
    let examples = crate_directory().join("../../docs/examples");
    for path in programs(&examples).unwrap() {
        let (syntax_errors, unsupported) = check(&path).unwrap();
        assert!(
            syntax_errors.is_empty() && unsupported.is_empty(),
            "{}:\n{syntax_errors}{unsupported}",
            path.display()
        );
    }
}
