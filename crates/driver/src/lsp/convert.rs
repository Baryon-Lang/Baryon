//! Translating between the compiler's terms and the protocol's: file paths and `file:` URIs,
//! byte offsets and line-and-character positions, and [`Diagnostic`]s and the protocol's
//! diagnostic objects.
//!
//! The protocol counts a position's character in UTF-16 code units, the default encoding every
//! client supports. The compiler counts bytes. So `ö` is one character to the editor, two bytes
//! to the compiler, and `𝜓` is two characters to the editor and four bytes to the compiler.

use diagnostics::{Diagnostic, Severity};
use serde_json::{Value, json};
use source::{SourceFile, Sources, Span};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// What the editor shows as the origin of every diagnostic.
const SOURCE: &str = "npl";

/// The bytes a URI path keeps as they are; every other byte is percent-encoded. These are the
/// unreserved characters of RFC 3986, plus `/` between path segments.
fn kept(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/')
}

/// The `file:` URI of an absolute path, such as `file:///work/sim/main.npl`.
pub(super) fn uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for &byte in path.to_string_lossy().as_bytes() {
        if kept(byte) {
            uri.push(char::from(byte));
        } else {
            // Writing to a `String` cannot fail.
            let _ = write!(uri, "%{byte:02X}");
        }
    }
    uri
}

/// The path a `file:` URI names, or `None` for any other scheme, such as an editor's
/// `untitled:` buffer, or for a URI whose escapes are not UTF-8.
pub(super) fn path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let mut bytes = Vec::with_capacity(rest.len());
    let mut input = rest.bytes();
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let high = char::from(input.next()?).to_digit(16)?;
            let low = char::from(input.next()?).to_digit(16)?;
            bytes.push(u8::try_from(high * 16 + low).ok()?);
        } else {
            bytes.push(byte);
        }
    }
    String::from_utf8(bytes).ok().map(PathBuf::from)
}

/// The protocol position of byte `offset` in `file`: a 0-based line and a 0-based character
/// counted in UTF-16 code units. An offset past the end clamps to the end, and one inside a
/// character moves back to its start.
fn position(file: &SourceFile, offset: u32) -> Value {
    let text = file.text();
    let mut index = (offset as usize).min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    let line_start = text[..index].rfind('\n').map_or(0, |newline| newline + 1);
    let character = text[line_start..index].encode_utf16().count();
    // Moving back to a character's start never crosses a line, so the line is the offset's.
    let line = file.position(offset).line - 1;
    json!({"line": line, "character": character})
}

/// The protocol range that `span` covers.
fn range(sources: &dyn Sources, span: Span) -> Value {
    let file = sources.file(span.file());
    json!({"start": position(file, span.start()), "end": position(file, span.end())})
}

/// The protocol's diagnostic object for `diagnostic`, whose primary span must be present.
///
/// The message is the diagnostic's summary, then the primary label and each note on lines of
/// their own, so the editor's hover shows what `npl check` would print. Secondary labels become
/// related information, which editors show as links to each location. `uri_of` gives the URI
/// of each file a secondary label points into.
pub(super) fn diagnostic(
    diagnostic: &Diagnostic,
    primary: Span,
    sources: &dyn Sources,
    uri_of: impl Fn(Span) -> String,
) -> Value {
    let mut message = diagnostic.message().to_owned();
    if let Some(label) = diagnostic
        .primary()
        .filter(|label| !label.message().is_empty())
    {
        message.push('\n');
        message.push_str(label.message());
    }
    for note in diagnostic.notes() {
        message.push_str("\nnote: ");
        message.push_str(note);
    }
    let related: Vec<Value> = diagnostic
        .secondary()
        .iter()
        .map(|label| {
            json!({
                "location": {"uri": uri_of(label.span()), "range": range(sources, label.span())},
                "message": label.message(),
            })
        })
        .collect();
    let mut object = json!({
        "range": range(sources, primary),
        "severity": severity(diagnostic.severity()),
        "source": SOURCE,
        "message": message,
    });
    if !related.is_empty() {
        object["relatedInformation"] = Value::Array(related);
    }
    object
}

/// The protocol's number for a severity: 1 is an error, 2 a warning, 3 information.
fn severity(severity: Severity) -> u8 {
    match severity {
        Severity::Error => 1,
        Severity::Warning => 2,
        Severity::Note => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use source::SourceMap;

    #[test]
    fn paths_and_uris_round_trip() {
        let path = Path::new("/work/my sim/ψ.npl");
        let uri = uri(path);
        assert_eq!(uri, "file:///work/my%20sim/%CF%88.npl");
        assert_eq!(super::path(&uri).as_deref(), Some(path));
    }

    #[test]
    fn uris_with_other_schemes_or_bad_escapes_name_no_path() {
        assert_eq!(path("untitled:Untitled-1"), None);
        assert_eq!(path("file:///work/%ZZ.npl"), None);
        assert_eq!(path("file:///work/%FF.npl"), None);
        assert_eq!(
            path("file://localhost/work/main.npl").as_deref(),
            Some(Path::new("/work/main.npl"))
        );
    }

    #[test]
    fn characters_count_utf16_code_units() {
        let file = SourceFile::new("main.npl", "x = 1\nψ = 𝜓 + y\n").unwrap();
        let at = |offset| position(&file, offset);
        assert_eq!(at(4), json!({"line": 0, "character": 4}));
        assert_eq!(at(6), json!({"line": 1, "character": 0}));
        // `ψ` is 2 bytes and 1 code unit; `𝜓` is 4 bytes and 2 code units.
        assert_eq!(at(15), json!({"line": 1, "character": 6}));
        // An offset inside `𝜓` moves back to its start.
        assert_eq!(at(13), json!({"line": 1, "character": 4}));
        assert_eq!(at(1000), json!({"line": 2, "character": 0}));
    }

    #[test]
    fn a_diagnostic_carries_its_labels_and_notes() {
        let mut sources = SourceMap::new();
        let main = sources.add("main.npl", "y = x + 1\n").unwrap();
        let other = sources.add("other.npl", "x = 2\n").unwrap();
        let problem = Diagnostic::error("undefined name `x`")
            .with_primary(Span::new(main, 4, 5), "not defined in this scope")
            .with_secondary(Span::new(other, 0, 1), "a different `x`")
            .with_note("names must be assigned before they are used");
        let object = diagnostic(&problem, Span::new(main, 4, 5), &sources, |span| {
            format!("file:///{}", sources.get(span.file()).name())
        });
        assert_eq!(
            object,
            json!({
                "range": {
                    "start": {"line": 0, "character": 4},
                    "end": {"line": 0, "character": 5},
                },
                "severity": 1,
                "source": "npl",
                "message": "undefined name `x`\nnot defined in this scope\n\
                            note: names must be assigned before they are used",
                "relatedInformation": [{
                    "location": {
                        "uri": "file:///other.npl",
                        "range": {
                            "start": {"line": 0, "character": 0},
                            "end": {"line": 0, "character": 1},
                        },
                    },
                    "message": "a different `x`",
                }],
            })
        );
    }
}
