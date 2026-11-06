//! The parser: turns a source file's text into a [`SyntaxTree`] and its syntax errors.
//!
//! This is the compiler's first stage. It reads text and produces the tree every later stage
//! works from, plus a [`Diagnostic`] for each syntax error. It is an implementation crate: the
//! rest of the compiler sees only the `syntax` interface, so this crate can be replaced by any
//! other that builds the same tree (decision 0001).
//!
//! # How it works
//!
//! Every source file is valid Python 3.12 (decision 0004), so this crate does not parse by
//! itself. It runs Ruff's Python parser (`ruff_python_parser`, pinned to one exact version),
//! then walks Ruff's tree once and writes the flat [`SyntaxTree`]. Ruff's parser is fast,
//! maintained, follows each new Python grammar, and recovers from errors: a file with a typo
//! still yields a whole tree, with [`SyntaxKind::Missing`](syntax::SyntaxKind::Missing) where
//! an expression was expected, and one diagnostic per error. The language server needs exactly
//! that for half-typed code.
//!
//! Ruff's error messages are rewritten in this language's voice, and syntax newer than Python
//! 3.12 is reported as an error, so every file stays readable by every Python 3.12 tool.
//!
//! The parser accepts all of Python 3.12. Rejecting the constructs this language does not
//! support, such as `lambda` or `global`, is the next stage's job, the `syntax_check` crate's, so
//! that the list can change without touching the parser.
//!
//! # Example
//!
//! ```
//! use source::SourceMap;
//! use syntax::{Role, SyntaxKind};
//!
//! let mut sources = SourceMap::new();
//! let text = "def area(r: f64) -> f64:\n    return 3.14159 * r**2\n";
//! let file = sources.add("circle.npl", text)?;
//!
//! let parsed = parser::parse(file, text);
//! assert!(parsed.diagnostics().is_empty());
//!
//! let tree = parsed.tree();
//! let function = tree.children(tree.root()).next().unwrap();
//! assert_eq!(tree.kind(function), SyntaxKind::FunctionDefinition);
//! let name = tree.child(function, Role::Name).unwrap();
//! assert_eq!(tree.text(name, text), "area");
//! # Ok::<(), source::SourceError>(())
//! ```

mod errors;
mod expressions;
mod lower;
mod patterns;
mod statements;

use diagnostics::Diagnostic;
use ruff_python_ast::PythonVersion;
use ruff_python_parser::{Mode, ParseOptions};
use source::FileId;
use syntax::SyntaxTree;

/// The Python grammar every source file follows (decision 0004).
const GRAMMAR: PythonVersion = PythonVersion::PY312;

/// The result of parsing one file: its tree and its syntax errors.
#[derive(Clone, Debug)]
pub struct Parsed {
    /// The file's syntax tree. Complete even when there are errors.
    tree: SyntaxTree,
    /// One error diagnostic per syntax error, in source order. Empty for a valid file.
    diagnostics: Vec<Diagnostic>,
}

impl Parsed {
    /// The file's syntax tree. It is complete even when the file has errors.
    #[must_use]
    pub fn tree(&self) -> &SyntaxTree {
        &self.tree
    }

    /// One error per syntax error, in source order. Empty for a valid file.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Splits the result into its tree and diagnostics, for a caller that keeps both.
    #[must_use]
    pub fn into_parts(self) -> (SyntaxTree, Vec<Diagnostic>) {
        (self.tree, self.diagnostics)
    }
}

/// Parses `text`, the contents of `file`, into a syntax tree and its syntax errors.
///
/// It never fails: a file with syntax errors still gets a whole tree, and the errors come back
/// as diagnostics. `text` must be the file's full contents, at most 4 GiB, which
/// [`source::SourceMap`] already guarantees.
///
/// Cost: one pass for Ruff's parser and one over its tree, both linear in the file's size.
#[must_use]
pub fn parse(file: FileId, text: &str) -> Parsed {
    let options = ParseOptions::from(Mode::Module).with_target_version(GRAMMAR);
    let parsed = ruff_python_parser::parse_unchecked(text, options);
    let mut diagnostics =
        errors::diagnostics(file, parsed.errors(), parsed.unsupported_syntax_errors());
    diagnostics.sort_by_key(|diagnostic| diagnostic.primary().map(|label| label.span().start()));
    // Ruff always returns a module in module mode. Were a future version not to, the file gets
    // an empty tree instead of a crash.
    let tree = match parsed.try_into_module() {
        Some(module) => lower::module(file, text, module.syntax()),
        None => lower::empty(file, text),
    };
    Parsed { tree, diagnostics }
}

#[cfg(test)]
mod tests {
    use super::*;
    use source::SourceMap;
    use syntax::{Role, SyntaxKind};

    fn parse_text(text: &str) -> Parsed {
        let file = SourceMap::new().add("test.npl", text).unwrap();
        parse(file, text)
    }

    #[test]
    fn a_valid_file_has_no_diagnostics() {
        let parsed = parse_text("x = 1\n");
        assert!(parsed.diagnostics().is_empty());
        assert_eq!(parsed.tree().len(), 4);
    }

    #[test]
    fn an_empty_file_is_an_empty_module() {
        let parsed = parse_text("");
        assert_eq!(parsed.tree().len(), 1);
        assert_eq!(parsed.tree().kind(parsed.tree().root()), SyntaxKind::Module);
    }

    #[test]
    fn a_missing_expression_becomes_a_missing_node() {
        let parsed = parse_text("x = \n");
        let tree = parsed.tree();
        let assign = tree.children(tree.root()).next().unwrap();
        let value = tree.child(assign, Role::Value).unwrap();
        assert_eq!(tree.kind(value), SyntaxKind::Missing);
        assert_eq!(parsed.diagnostics().len(), 1);
    }

    #[test]
    fn diagnostics_come_in_source_order() {
        let (_, diagnostics) = parse_text("def f[T = int](:\n").into_parts();
        let starts: Vec<u32> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.primary().unwrap().span().start())
            .collect();
        assert!(starts.len() >= 2);
        assert!(starts.is_sorted());
    }

    #[test]
    fn dictionary_unpacking_starts_at_its_stars() {
        let text = "{1: 2, ** more}";
        let parsed = parse_text(text);
        let tree = parsed.tree();
        let unpacking = tree
            .nodes()
            .iter()
            .find(|node| node.kind() == SyntaxKind::DoubleStarred)
            .unwrap();
        assert_eq!(
            &text[unpacking.start() as usize..unpacking.end() as usize],
            "** more"
        );
    }
}
