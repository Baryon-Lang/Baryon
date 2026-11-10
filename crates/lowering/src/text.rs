//! `str` values (decision 0022). Building one: an f-string, a `copy()` of a `str` and `a + b`
//! all lower to one [`Format`](Rvalue::Format) rvalue, whose pieces are literal text and values
//! read in place, each with its format specification. Reading one: comparisons, `in` and `len`
//! call the runtime through [`Builtin::CompareStr`], [`Builtin::ContainsStr`] and
//! [`Builtin::LengthStr`], with each string borrowed where it is (roadmap 1.13.2).

use crate::builder::Builder;
use crate::escapes::{self, string};
use mir::{
    Argument, BinaryOperator, Builtin, Callee, Operand, Piece, Place, Rvalue, TerminatorKind,
    UnaryOperator,
};
use syntax::{Flags, NodeId, Operator, Role, SyntaxKind};
use types::FormatSpec;

impl<'db> Builder<'db> {
    /// Writes the f-string `node` into `place`: its literal text and each interpolated value with
    /// its specification, which the checker has checked against the value's type.
    pub(crate) fn formatted(&mut self, place: Place, node: NodeId) {
        let mut pieces = Vec::new();
        let raw = self
            .text_of(node)
            .chars()
            .take_while(|&letter| letter != '"' && letter != '\'')
            .any(|letter| letter.eq_ignore_ascii_case(&'r'));
        let items: Vec<NodeId> = self.tree.children(node).collect();
        for item in items {
            match self.tree.kind(item) {
                SyntaxKind::StringLiteral => match string(self.text_of(item)) {
                    Some(text) => pieces.push(Piece::Text(text)),
                    None => {
                        return self
                            .unsupported(item, "strings with `\\N{…}` escapes or a `b` prefix");
                    }
                },
                SyntaxKind::FormattedText => match escapes::formatted(self.text_of(item), raw) {
                    Some(text) => pieces.push(Piece::Text(text)),
                    None => return self.unsupported(item, "f-string text with `\\N{…}` escapes"),
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

    /// One link of a comparison chain between two `str`s, `left op right`, into the `bool` at
    /// `place`. `==`, `!=`, `<`, `<=`, `>` and `>=` compare the three-way order
    /// [`Builtin::CompareStr`] gives with 0; `in` and `not in` ask [`Builtin::ContainsStr`]
    /// whether `left` occurs in `right`. `previous` is the left string when the link before
    /// borrowed it, so a chain reads each operand once. Returns the right string, for the next
    /// link.
    pub(crate) fn text_comparison(
        &mut self,
        place: Place,
        operator: Operator,
        (left, right): (NodeId, NodeId),
        previous: Option<Operand<'db>>,
        node: NodeId,
    ) -> Option<Operand<'db>> {
        let left = match previous {
            Some(Operand::Copy(place)) => place,
            _ => self.borrow(left),
        };
        let right = self.borrow(right);
        let returned = Some(Operand::Copy(right.clone()));
        if matches!(operator, Operator::In | Operator::NotIn) {
            let arguments = vec![Argument::Borrow(right), Argument::Borrow(left)];
            if operator == Operator::In {
                self.text_call(place, Builtin::ContainsStr, arguments, node);
            } else {
                let found = self.temporary(self.boolean_type(), node);
                self.text_call(Place::from(found), Builtin::ContainsStr, arguments, node);
                let found = Operand::Copy(Place::from(found));
                self.assign(place, Rvalue::Unary(UnaryOperator::Not, found), node);
            }
            return returned;
        }
        let compared = match operator {
            Operator::Equal => BinaryOperator::Equal,
            Operator::NotEqual => BinaryOperator::NotEqual,
            Operator::Less => BinaryOperator::Less,
            Operator::LessEqual => BinaryOperator::LessEqual,
            Operator::Greater => BinaryOperator::Greater,
            Operator::GreaterEqual => BinaryOperator::GreaterEqual,
            _ => {
                self.unsupported(node, &format!("`{}` on strings", operator.symbol()));
                return None;
            }
        };
        let int = self.int_type();
        let order = self.temporary(int, node);
        let arguments = vec![Argument::Borrow(left), Argument::Borrow(right)];
        self.text_call(Place::from(order), Builtin::CompareStr, arguments, node);
        let zero = Operand::Constant(self.number_constant(int, 0));
        let order = Operand::Copy(Place::from(order));
        self.assign(place, Rvalue::Binary(compared, order, zero), node);
        returned
    }

    /// `len(text)` of a `str` into `place`: its number of characters, through
    /// [`Builtin::LengthStr`].
    pub(crate) fn text_length(&mut self, place: Place, text: NodeId, node: NodeId) {
        let text = self.borrow(text);
        self.text_call(
            place,
            Builtin::LengthStr,
            vec![Argument::Borrow(text)],
            node,
        );
    }

    /// `place = builtin(arguments…)`, for a built-in over `str`s, which cannot fail.
    fn text_call(
        &mut self,
        place: Place,
        builtin: Builtin,
        arguments: Vec<Argument<'db>>,
        node: NodeId,
    ) {
        let next = self.new_block();
        self.terminate(
            TerminatorKind::Call {
                callee: Callee::Builtin(builtin),
                arguments,
                destination: place,
                target: next,
                error: None,
            },
            node,
        );
        self.start(next);
    }
}
