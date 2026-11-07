//! Plain-text rendering of diagnostics, in the style of rustc:
//!
//! ```text
//! error: undefined name `x`
//!  --> main.byn:1:5
//!   |
//! 1 | y = x + 1
//!   |     ^ not defined in this scope
//!   = note: names must be assigned before they are used
//! ```

use crate::{Diagnostic, Label};
use source::Sources;
use std::fmt;

/// Columns a tab advances in rendered source lines.
const TAB_WIDTH: usize = 4;

/// Returns a value that formats `diagnostic` as text, reading source lines from `sources`: a
/// [`source::SourceMap`] or any other [`Sources`], such as the compiler's database.
///
/// Nothing is formatted until the value is displayed, so writing it straight to a terminal or
/// a log allocates no intermediate string.
///
/// ```
/// use diagnostics::{Diagnostic, render};
/// use source::{SourceMap, Span};
///
/// let mut sources = SourceMap::new();
/// let file = sources.add("main.byn", "y = x + 1\n")?;
/// let diagnostic = Diagnostic::error("undefined name `x`")
///     .with_primary(Span::new(file, 4, 5), "not defined in this scope");
///
/// assert_eq!(
///     render(&diagnostic, &sources).to_string(),
///     "error: undefined name `x`\n --> main.byn:1:5\n  |\n1 | y = x + 1\n  |     ^ not defined in this scope",
/// );
/// # Ok::<(), source::SourceError>(())
/// ```
#[must_use]
pub fn render<'a>(diagnostic: &'a Diagnostic, sources: &'a dyn Sources) -> Rendered<'a> {
    Rendered {
        diagnostic,
        sources,
    }
}

/// A diagnostic ready to display. Created by [`render()`].
#[derive(Clone, Copy, Debug)]
pub struct Rendered<'a> {
    /// The diagnostic to format.
    diagnostic: &'a Diagnostic,
    /// Where the diagnostic's spans point, for reading file names and source lines.
    sources: &'a dyn Sources,
}

impl fmt::Display for Rendered<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let diagnostic = self.diagnostic;
        let labels: Vec<(&Label, char)> = diagnostic
            .primary()
            .map(|label| (label, '^'))
            .into_iter()
            .chain(diagnostic.secondary().iter().map(|label| (label, '-')))
            .collect();
        let gutter = labels
            .iter()
            .map(|(label, _)| self.line_of(label).to_string().len())
            .max()
            .unwrap_or(0);

        write!(
            formatter,
            "{}: {}",
            diagnostic.severity(),
            diagnostic.message()
        )?;
        for (index, &(label, marker)) in labels.iter().enumerate() {
            let arrow = if index == 0 { "-->" } else { ":::" };
            self.write_excerpt(formatter, label, marker, arrow, gutter)?;
        }
        for note in diagnostic.notes() {
            write!(formatter, "\n{:gutter$} = note: {note}", "")?;
        }
        Ok(())
    }
}

impl Rendered<'_> {
    /// The 1-based line where a label starts, used to size the line-number gutter.
    fn line_of(&self, label: &Label) -> u32 {
        let span = label.span();
        self.sources.file(span.file()).position(span.start()).line
    }

    /// Writes one label: its location, the source line, and markers under the labelled text.
    ///
    /// A span that runs past the end of its first line is marked to the end of that line.
    fn write_excerpt(
        &self,
        formatter: &mut fmt::Formatter<'_>,
        label: &Label,
        marker: char,
        arrow: &str,
        gutter: usize,
    ) -> fmt::Result {
        let span = label.span();
        let file = self.sources.file(span.file());
        let start = file.position(span.start());
        let end = file.position(span.end());
        let line_text = file.line(start.line).unwrap_or_default();
        let characters: Vec<char> = line_text.chars().collect();

        let first = start.column as usize - 1;
        let last = if end.line == start.line {
            end.column as usize - 1
        } else {
            characters.len()
        };
        let indent = display_width(characters.iter().take(first));
        let width = display_width(characters.iter().take(last).skip(first)).max(1);
        let marks = marker.to_string().repeat(width);
        let message = label.message();

        write!(
            formatter,
            "\n{:gutter$}{arrow} {}:{}:{}",
            "",
            file.name(),
            start.line,
            start.column
        )?;
        write!(formatter, "\n{:gutter$} |", "")?;
        write!(
            formatter,
            "\n{:>gutter$} | {}",
            start.line,
            line_text.replace('\t', &" ".repeat(TAB_WIDTH))
        )?;
        write!(formatter, "\n{:gutter$} | {:indent$}{marks}", "", "")?;
        if !message.is_empty() {
            write!(formatter, " {message}")?;
        }
        Ok(())
    }
}

/// Width of `characters` on screen, with tabs expanded to [`TAB_WIDTH`] columns.
///
/// Every other character counts as one column. Wide characters, such as CJK text and most emoji,
/// take two columns in a terminal, so markers under or after them fall short; roadmap idea I.12
/// tracks measuring true display width.
fn display_width<'a>(characters: impl Iterator<Item = &'a char>) -> usize {
    characters
        .map(|&character| if character == '\t' { TAB_WIDTH } else { 1 })
        .sum()
}

#[cfg(test)]
mod tests {
    use crate::{Diagnostic, render};
    use source::{SourceMap, Span};

    fn rendered(text: &str, build: impl Fn(source::FileId) -> Diagnostic) -> String {
        let mut sources = SourceMap::new();
        let file = sources.add("main.byn", text).unwrap();
        render(&build(file), &sources).to_string()
    }

    #[test]
    fn renders_message_without_location() {
        let output = rendered("", |_| Diagnostic::error("no input files"));
        assert_eq!(output, "error: no input files");
    }

    #[test]
    fn renders_primary_label_and_note() {
        let output = rendered("y = x + 1\n", |file| {
            Diagnostic::error("undefined name `x`")
                .with_primary(Span::new(file, 4, 5), "not defined in this scope")
                .with_note("names must be assigned before they are used")
        });
        let expected = "\
error: undefined name `x`
 --> main.byn:1:5
  |
1 | y = x + 1
  |     ^ not defined in this scope
  = note: names must be assigned before they are used";
        assert_eq!(output, expected);
    }

    #[test]
    fn renders_secondary_labels_with_dashes() {
        let text = "a = load()\nb = a\nc = a\n";
        let output = rendered(text, |file| {
            Diagnostic::error("value `a` used after move")
                .with_primary(Span::new(file, 21, 22), "used here after the move")
                .with_secondary(Span::new(file, 15, 16), "moved here")
        });
        let expected = "\
error: value `a` used after move
 --> main.byn:3:5
  |
3 | c = a
  |     ^ used here after the move
 ::: main.byn:2:5
  |
2 | b = a
  |     - moved here";
        assert_eq!(output, expected);
    }

    #[test]
    fn widens_gutter_for_large_line_numbers() {
        let text = format!("{}x = 1\n", "\n".repeat(99));
        let output = rendered(&text, |file| {
            Diagnostic::warning("unused variable `x`").with_primary(Span::new(file, 99, 100), "")
        });
        let expected = "\
warning: unused variable `x`
   --> main.byn:100:1
    |
100 | x = 1
    | ^";
        assert_eq!(output, expected);
    }

    #[test]
    fn underlines_multiline_spans_to_end_of_first_line() {
        let output = rendered("total = f(\n    1)\n", |file| {
            Diagnostic::error("bad call").with_primary(Span::new(file, 8, 16), "")
        });
        assert!(
            output.ends_with("1 | total = f(\n  |         ^^"),
            "{output}"
        );
    }

    #[test]
    fn expands_tabs_so_markers_line_up() {
        let output = rendered("\tx = 1\n", |file| {
            Diagnostic::error("tab indentation").with_primary(Span::new(file, 1, 2), "")
        });
        assert!(output.ends_with("1 |     x = 1\n  |     ^"), "{output}");
    }
}
