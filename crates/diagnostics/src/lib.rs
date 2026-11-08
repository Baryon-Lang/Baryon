//! Compiler messages: what went wrong, where, and what else the reader should know.
//!
//! Every stage reports problems as [`Diagnostic`] values and returns them; no stage prints.
//! The application decides where diagnostics go. [`render()`] produces the plain-text form used
//! on the command line.

mod render;

pub use render::{Rendered, render};

use source::Span;
use std::fmt;

/// How serious a diagnostic is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Extra context attached to the compilation, never a problem by itself.
    Note,
    /// A likely mistake that does not stop compilation.
    Warning,
    /// A problem that stops compilation.
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Note => "note",
            Self::Warning => "warning",
            Self::Error => "error",
        })
    }
}

/// A span of source text with a short message explaining its part in the diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    /// The text the label points at.
    span: Span,
    /// What the label says about that text. May be empty.
    message: String,
}

impl Label {
    /// The text this label points at.
    #[must_use]
    pub fn span(&self) -> Span {
        self.span
    }

    /// What the label says about that text. May be empty.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// One message from the compiler.
///
/// Built with a constructor and `with_*` methods:
///
/// ```
/// # let mut sources = source::SourceMap::new();
/// # let file = sources.add("main.byn", "y = x + 1").unwrap();
/// use diagnostics::Diagnostic;
/// use source::Span;
///
/// let diagnostic = Diagnostic::error("undefined name `x`")
///     .with_primary(Span::new(file, 4, 5), "not defined in this scope")
///     .with_note("names must be assigned before they are used");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// How serious the diagnostic is.
    severity: Severity,
    /// The one-line summary shown first.
    message: String,
    /// The main location of the problem, marked with `^` when rendered.
    primary: Option<Label>,
    /// Related locations, marked with `-` when rendered.
    secondary: Vec<Label>,
    /// Extra explanation shown after the source excerpts.
    notes: Vec<String>,
}

impl Diagnostic {
    /// Creates a diagnostic with no location.
    #[must_use]
    pub fn new(severity: Severity, message: impl Into<String>) -> Self {
        Self {
            severity,
            message: message.into(),
            primary: None,
            secondary: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// Creates an error.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(Severity::Error, message)
    }

    /// Creates a warning.
    #[must_use]
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, message)
    }

    /// Sets the main location of the problem, replacing any earlier one.
    #[must_use]
    pub fn with_primary(mut self, span: Span, message: impl Into<String>) -> Self {
        self.primary = Some(Label {
            span,
            message: message.into(),
        });
        self
    }

    /// Adds a related location, such as where a moved value was first used.
    #[must_use]
    pub fn with_secondary(mut self, span: Span, message: impl Into<String>) -> Self {
        self.secondary.push(Label {
            span,
            message: message.into(),
        });
        self
    }

    /// Adds a note shown after the source excerpts.
    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// How serious the diagnostic is.
    #[must_use]
    pub fn severity(&self) -> Severity {
        self.severity
    }

    /// The one-line summary.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The main location, if the diagnostic has one.
    #[must_use]
    pub fn primary(&self) -> Option<&Label> {
        self.primary.as_ref()
    }

    /// Related locations, in the order they were added.
    #[must_use]
    pub fn secondary(&self) -> &[Label] {
        &self.secondary
    }

    /// Notes, in the order they were added.
    #[must_use]
    pub fn notes(&self) -> &[String] {
        &self.notes
    }
}
