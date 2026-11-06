//! Syntax errors as this language's diagnostics.
//!
//! Ruff words its errors for Python programmers using Ruff. This module rewrites the common ones
//! for someone writing this language: lower-case like every other compiler message, pointing at
//! the problem, with a note on how to fix it where one helps. Errors with no rewrite keep Ruff's
//! wording, lower-cased.

use diagnostics::Diagnostic;
use ruff_python_parser::{LexicalErrorType, ParseError, ParseErrorType, UnsupportedSyntaxError};
use ruff_text_size::{Ranged, TextRange};
use source::{FileId, Span};

/// Converts Ruff's parse errors and newer-than-3.12 syntax errors into diagnostics.
pub(crate) fn diagnostics(
    file: FileId,
    errors: &[ParseError],
    unsupported: &[UnsupportedSyntaxError],
) -> Vec<Diagnostic> {
    errors
        .iter()
        .map(|error| parse_error(file, error))
        .chain(
            unsupported
                .iter()
                .map(|error| unsupported_syntax(file, error)),
        )
        .collect()
}

/// One parse error as a diagnostic.
fn parse_error(file: FileId, error: &ParseError) -> Diagnostic {
    let span = span(file, error.location);
    match &error.error {
        ParseErrorType::ExpectedToken { expected, found } => {
            Diagnostic::error(format!("expected {expected}, found {found}"))
                .with_primary(span, format!("expected {expected}"))
        }
        ParseErrorType::ExpectedExpression => Diagnostic::error("expected an expression")
            .with_primary(span, "an expression should be here"),
        ParseErrorType::UnexpectedIndentation => Diagnostic::error("unexpected indentation")
            .with_primary(span, "this line is indented more than the one before it")
            .with_note("indent only the lines of a block, after a line that ends in `:`"),
        ParseErrorType::Lexical(LexicalErrorType::IndentationError) => {
            Diagnostic::error("this line's indentation matches no enclosing block")
                .with_primary(span, "unindented to here")
                .with_note("each block is indented 4 spaces deeper than the line that opens it")
        }
        ParseErrorType::Lexical(LexicalErrorType::UnclosedStringError) => {
            Diagnostic::error("this string is never closed")
                .with_primary(span, "the string starts here")
                .with_note("close it with the same quote it opens with")
        }
        ParseErrorType::SimpleStatementsOnSameLine
        | ParseErrorType::SimpleAndCompoundStatementOnSameLine => {
            Diagnostic::error("two statements on one line")
                .with_primary(span, "a new statement starts here")
                .with_note("put each statement on its own line")
        }
        other => Diagnostic::error(lower_first(&other.to_string())).with_primary(span, ""),
    }
}

/// Syntax that is valid in a newer Python than 3.12 as a diagnostic.
fn unsupported_syntax(file: FileId, error: &UnsupportedSyntaxError) -> Diagnostic {
    Diagnostic::error(lower_first(&error.to_string()))
        .with_primary(span(file, error.range()), "")
        .with_note("source files use Python 3.12 syntax, so every Python tool can read them")
}

/// A Ruff range as a span in `file`.
fn span(file: FileId, range: TextRange) -> Span {
    Span::new(file, range.start().into(), range.end().into())
}

/// `message` with its first letter in lower case, the style of every compiler message.
fn lower_first(message: &str) -> String {
    let mut characters = message.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(characters).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_first_lowers_only_the_first_letter() {
        assert_eq!(lower_first("Expected an `Array`"), "expected an `Array`");
        assert_eq!(lower_first(""), "");
    }
}
