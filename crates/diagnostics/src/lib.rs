//! Compiler messages: what went wrong, where, and what else the reader should know.
//!
//! Every stage reports problems as [`Diagnostic`] values and returns them; no stage prints.
//! The application decides where diagnostics go. [`render()`] produces the plain-text form used
//! on the command line.
//!
//! ```
//! use diagnostics::{Diagnostic, Severity, render};
//! use source::{SourceMap, Span};
//!
//! let mut sources = SourceMap::new();
//! let file = sources.add("main.byn", "n: u8 = 300\n")?;
//!
//! // A stage returns its problems as values...
//! let problems = vec![
//!     Diagnostic::error("`300` does not fit in `u8`")
//!         .with_primary(Span::new(file, 8, 11), "the largest `u8` is 255"),
//! ];
//!
//! // ...and the application counts them and decides where they go.
//! let errors = problems.iter().filter(|problem| problem.severity() == Severity::Error).count();
//! assert_eq!(errors, 1);
//! assert_eq!(
//!     render(&problems[0], &sources).to_string(),
//!     "error: `300` does not fit in `u8`\n --> main.byn:1:9\n  |\n1 | n: u8 = 300\n  |         ^^^ the largest `u8` is 255",
//! );
//! # Ok::<(), source::SourceError>(())
//! ```

mod render;

pub use render::{Rendered, render};

use source::Span;
use std::fmt;

/// How serious a diagnostic is.
///
/// Severities are ordered from least to most serious, and display as the word that starts a
/// rendered diagnostic:
///
/// ```
/// use diagnostics::Severity;
///
/// assert!(Severity::Note < Severity::Warning && Severity::Warning < Severity::Error);
/// assert_eq!(Severity::Warning.to_string(), "warning");
/// ```
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
///
/// A label is made by [`Diagnostic::with_primary`] or [`Diagnostic::with_secondary`], and read
/// back from the diagnostic:
///
/// ```
/// use diagnostics::Diagnostic;
/// use source::{FileId, Span};
///
/// let span = Span::new(FileId::from_index(0), 4, 5);
/// let diagnostic = Diagnostic::error("undefined name `x`").with_primary(span, "not defined");
/// let label = diagnostic.primary().expect("a primary label was set");
/// assert_eq!((label.span(), label.message()), (span, "not defined"));
/// ```
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
    ///
    /// A diagnostic without a location suits a problem with the whole package, such as a file
    /// whose name is not a valid module name.
    ///
    /// ```
    /// use diagnostics::{Diagnostic, Severity};
    ///
    /// let note = Diagnostic::new(Severity::Note, "checked 3 files");
    /// assert_eq!(note.severity(), Severity::Note);
    /// assert!(note.primary().is_none());
    /// ```
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

    /// Creates an error, which stops compilation.
    ///
    /// ```
    /// use diagnostics::{Diagnostic, Severity};
    ///
    /// let error = Diagnostic::error("`main` takes no parameters");
    /// assert_eq!(error.severity(), Severity::Error);
    /// assert_eq!(error.message(), "`main` takes no parameters");
    /// ```
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(Severity::Error, message)
    }

    /// Creates a warning, which reports a likely mistake without stopping compilation.
    ///
    /// ```
    /// use diagnostics::{Diagnostic, Severity};
    ///
    /// let warning = Diagnostic::warning("reordering the fields of `Sample` would save 8 bytes");
    /// assert_eq!(warning.severity(), Severity::Warning);
    /// ```
    #[must_use]
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, message)
    }

    /// Sets the main location of the problem, replacing any earlier one. It is marked with `^`
    /// when rendered.
    ///
    /// ```
    /// use diagnostics::Diagnostic;
    /// use source::{FileId, Span};
    ///
    /// let file = FileId::from_index(0);
    /// let diagnostic = Diagnostic::error("type mismatch")
    ///     .with_primary(Span::new(file, 0, 1), "first guess")
    ///     .with_primary(Span::new(file, 4, 9), "expected `f64`");
    /// assert_eq!(diagnostic.primary().map(|label| label.message()), Some("expected `f64`"));
    /// ```
    #[must_use]
    pub fn with_primary(mut self, span: Span, message: impl Into<String>) -> Self {
        self.primary = Some(Label {
            span,
            message: message.into(),
        });
        self
    }

    /// Adds a related location, such as where a moved value was first used. When rendered, each
    /// is shown after the primary location, under a `:::` line naming it, and marked with `-`.
    ///
    /// ```
    /// use diagnostics::{Diagnostic, render};
    /// use source::{SourceMap, Span};
    ///
    /// let mut sources = SourceMap::new();
    /// let file = sources.add("main.byn", "b = a\nc = a\n")?;
    /// let diagnostic = Diagnostic::error("`a` was moved")
    ///     .with_primary(Span::new(file, 10, 11), "used here after the move")
    ///     .with_secondary(Span::new(file, 4, 5), "moved here");
    ///
    /// assert_eq!(diagnostic.secondary().len(), 1);
    /// assert_eq!(
    ///     render(&diagnostic, &sources).to_string(),
    ///     "error: `a` was moved\n --> main.byn:2:5\n  |\n2 | c = a\n  |     ^ used here after the move\n \
    ///      ::: main.byn:1:5\n  |\n1 | b = a\n  |     - moved here",
    /// );
    /// # Ok::<(), source::SourceError>(())
    /// ```
    #[must_use]
    pub fn with_secondary(mut self, span: Span, message: impl Into<String>) -> Self {
        self.secondary.push(Label {
            span,
            message: message.into(),
        });
        self
    }

    /// Adds a note shown after the source excerpts. Use it for what the reader should do next.
    ///
    /// ```
    /// use diagnostics::Diagnostic;
    ///
    /// let diagnostic = Diagnostic::error("integer division by zero")
    ///     .with_note("divide floats to get an infinity instead");
    /// assert_eq!(diagnostic.notes(), ["divide floats to get an infinity instead"]);
    /// ```
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
