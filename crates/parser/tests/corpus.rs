//! The parser test corpus, and a check that every example program parses cleanly.
//!
//! `tests/corpus/` holds small programs, each covering one area of the grammar. Next to each
//! `name.npl` sits `name.tree`, the expected tree as `SyntaxTree::display` prints it, and, for
//! a program with syntax errors, `name.errors`, the expected diagnostics as rendered text.
//!
//! After an intended change to the tree or the messages, regenerate the expected files with
//!
//! ```sh
//! BLESS=1 cargo test --package parser --test corpus
//! ```
//!
//! and review the difference in version control like any other change.

use source::SourceMap;
use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use syntax::{NodeId, SyntaxKind, SyntaxTree};

/// What a helper returns: its value, or the error that stops the test.
type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// The source file extension, a working choice until roadmap decision D.6 names the language.
const EXTENSION: &str = "npl";

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

/// Parses `path` and returns its tree, its text and its rendered diagnostics.
fn parse(path: &Path) -> Result<(SyntaxTree, String, String)> {
    let text = fs::read_to_string(path)?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut sources = SourceMap::new();
    let file = sources.add(name, text.clone())?;
    let parsed = parser::parse(file, &text);
    let mut rendered = String::new();
    for diagnostic in parsed.diagnostics() {
        writeln!(rendered, "{}\n", diagnostics::render(diagnostic, &sources))?;
    }
    Ok((parsed.tree().clone(), text, rendered))
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
            "{} differs from the parser's output. Expected:\n{found}\nActual:\n{actual}",
            expected.display()
        )
    }))
}

#[test]
fn corpus_matches_expected_trees_and_diagnostics() {
    let mut failures = Vec::new();
    for path in programs(&crate_directory().join("tests/corpus")).unwrap() {
        let (tree, text, rendered) = parse(&path).unwrap();
        let tree_text = tree.display(&text).to_string();
        failures.extend(compare(&path.with_extension("tree"), &tree_text).unwrap());
        failures.extend(compare(&path.with_extension("errors"), &rendered).unwrap());
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_example_program_parses_without_errors() {
    let examples = crate_directory().join("../../docs/examples");
    for path in programs(&examples).unwrap() {
        let (tree, _, rendered) = parse(&path).unwrap();
        assert!(rendered.is_empty(), "{}:\n{rendered}", path.display());
        check_shape(&tree, tree.root());
    }
}

/// Checks what every tree of a valid program promises: each child lies inside its parent,
/// children come in source order, and no expression is missing.
fn check_shape(tree: &SyntaxTree, id: NodeId) {
    let parent = tree.node(id);
    assert_ne!(
        parent.kind(),
        SyntaxKind::Missing,
        "missing node at {}",
        parent.start()
    );
    let mut previous_end = parent.start();
    for child in tree.children(id) {
        let node = tree.node(child);
        assert!(
            previous_end <= node.start() && node.end() <= parent.end(),
            "{:?} at {}..{} escapes its parent {:?} at {}..{} or overlaps a sibling",
            node.kind(),
            node.start(),
            node.end(),
            parent.kind(),
            parent.start(),
            parent.end(),
        );
        previous_end = node.end();
        check_shape(tree, child);
    }
}
