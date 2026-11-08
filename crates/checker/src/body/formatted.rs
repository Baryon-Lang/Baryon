//! f-strings (decision 0022): the values interpolated into one, and each format specification
//! checked against the type of the value it formats, so a specification Python would reject at
//! run time is an error at compile time.

use super::Body;
use syntax::{NodeId, Role, SyntaxKind};
use types::{FormatClass, FormatSpec, Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// Types the values interpolated into an f-string.
    pub(super) fn interpolations(&mut self, node: NodeId) {
        let parts: Vec<NodeId> = self.tree.children(node).collect();
        for part in parts {
            match self.tree.kind(part) {
                SyntaxKind::Interpolation => {
                    let ty = self
                        .tree
                        .child(part, Role::Value)
                        .map(|value| self.expression(value, None));
                    if let Some(format) = self.tree.child(part, Role::Format) {
                        self.interpolations(format);
                        if let Some(ty) = ty {
                            self.format_spec(format, ty);
                        }
                    }
                }
                SyntaxKind::FormatSpecification => self.interpolations(part),
                _ => {}
            }
        }
    }

    /// Checks the format specification `format` against `ty`, the type of the value it formats,
    /// with the rules Python applies when it formats one (decision 0022): `{x:.2f}` of a `str` is
    /// an error. A specification with a value inside, `{x:{width}}`, is checked when it is
    /// formatted, and so is a value of a type a specification does not apply to yet.
    fn format_spec(&mut self, format: NodeId, ty: Type<'db>) {
        let mut text = String::new();
        for part in self.tree.children(format) {
            if self.tree.kind(part) != SyntaxKind::FormattedText {
                return;
            }
            text.push_str(self.tree.text(part, self.text));
        }
        let class = match ty.kind(self.db) {
            TypeKind::Primitive(Primitive::Integer(_)) => FormatClass::Integer,
            TypeKind::Primitive(Primitive::Float(_)) => FormatClass::Float,
            TypeKind::Primitive(Primitive::Bool) => FormatClass::Bool,
            TypeKind::Str => FormatClass::Text,
            _ => return,
        };
        let checked = FormatSpec::parse(&text).and_then(|spec| spec.check(class));
        if let Err(error) = checked {
            let message = format!("{error}");
            let label = format!("formats a value of type `{}`", ty.display(self.db));
            self.lowerer.error(
                format,
                &message,
                &label,
                Some("an f-string's format specification follows Python's format mini-language"),
            );
        }
    }
}
