//! Building `str` values (decision 0022): an f-string, a `copy()` of a `str` and `a + b` all
//! lower to one [`Format`](Rvalue::Format) rvalue, whose pieces are literal text and values
//! read in place, each with its format specification.

use crate::builder::Builder;
use crate::constants::string;
use mir::{Piece, Place, Rvalue};
use syntax::{Flags, NodeId, Role, SyntaxKind};
use types::FormatSpec;

impl Builder<'_> {
    /// Writes the f-string `node` into `place`: its literal text and each interpolated value with
    /// its specification, which the checker has checked against the value's type.
    pub(crate) fn formatted(&mut self, place: Place, node: NodeId) {
        let mut pieces = Vec::new();
        let items: Vec<NodeId> = self.tree.children(node).collect();
        for item in items {
            match self.tree.kind(item) {
                SyntaxKind::StringLiteral => match string(self.text_of(item)) {
                    Some(text) => pieces.push(Piece::Text(text)),
                    None => {
                        return self.unsupported(item, "strings with escapes or prefixes");
                    }
                },
                SyntaxKind::FormattedText => match self.formatted_text(item) {
                    Some(text) => pieces.push(Piece::Text(text)),
                    None => return self.unsupported(item, "f-string text with escapes"),
                },
                SyntaxKind::Interpolation => {
                    let flags = self.tree.node(item).flags();
                    if !flags.is_empty() && !flags.contains(Flags::CONVERT_STR) {
                        return self.unsupported(item, "`=` and `!r` in an f-string");
                    }
                    let Some(value) = self.tree.child(item, Role::Value) else {
                        return;
                    };
                    let Some(spec) = self.format_spec(item) else {
                        return;
                    };
                    let value = self.borrow(value);
                    pieces.push(Piece::Value(value, spec));
                }
                _ => return self.unsupported(item, "this part of an f-string"),
            }
        }
        self.assign(place, Rvalue::Format(pieces), node);
    }

    /// Writes a new `str` holding the text of each of `values` in order into `place`: one value
    /// for `copy()`, two for `a + b`.
    pub(crate) fn joined(&mut self, place: Place, values: &[NodeId], node: NodeId) {
        let pieces = values
            .iter()
            .map(|&value| Piece::Value(self.borrow(value), FormatSpec::default()))
            .collect();
        self.assign(place, Rvalue::Format(pieces), node);
    }

    /// The specification of the interpolation `node`, the default when it has none, or `None`
    /// after reporting one this slice does not lower: one with a value inside, `{x:{width}}`.
    fn format_spec(&mut self, node: NodeId) -> Option<FormatSpec> {
        let Some(format) = self.tree.child(node, Role::Format) else {
            return Some(FormatSpec::default());
        };
        let mut text = String::new();
        let parts: Vec<NodeId> = self.tree.children(format).collect();
        for part in parts {
            if self.tree.kind(part) != SyntaxKind::FormattedText {
                self.unsupported(part, "a value inside a format specification");
                return None;
            }
            text.push_str(self.text_of(part));
        }
        // The checker reported a specification that does not parse.
        FormatSpec::parse(&text).ok()
    }

    /// The text of the f-string text `node`, with `{{` and `}}` read as one brace, or `None` for
    /// text with a backslash escape, which this slice does not read yet.
    fn formatted_text(&self, node: NodeId) -> Option<String> {
        let written = self.text_of(node);
        (!written.contains('\\')).then(|| written.replace("{{", "{").replace("}}", "}"))
    }
}
