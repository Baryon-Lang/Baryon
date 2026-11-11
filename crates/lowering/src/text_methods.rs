//! `str`'s methods and its indexing and slicing (roadmap 1.13.12). Each is one call of
//! [`Builtin::Text`] with the text borrowed where it is; the backend and the runtime do the work.
//!
//! `text[i]` and `text[i:j:k]` count characters, as Python's do, and follow the rules of a list:
//! a negative literal counts from the end, an index or a bound outside the text panics, and a
//! slice is a new `str`. The text's length comes from [`Builtin::LengthStr`], then the index or
//! range is checked against it exactly as an array's is, then [`TextMethod::Take`] copies the
//! characters out.
//!
//! `int(text)` and `float(text)` (decision 0037) are [`TextMethod::ParseInt`] and
//! [`TextMethod::ParseFloat`] calls that can fail: the checker records the library's
//! `ValueError` as the call's error, and the call gets an error edge like any fallible call.

use crate::builder::Builder;
use mir::{
    Argument, Builtin, Callee, Constant, Operand, Place, SliceIndex, TerminatorKind, TextMethod,
};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Primitive, Type, TypeKind};

impl<'db> Builder<'db> {
    /// Whether `node` indexes or slices a `str`, which makes a new `str` rather than naming a
    /// place.
    pub(crate) fn is_text_subscript(&self, node: NodeId) -> bool {
        self.tree.kind(node) == SyntaxKind::Subscript
            && self
                .tree
                .child(node, Role::Value)
                .is_some_and(|value| matches!(self.ty(value).kind(self.db), TypeKind::Str))
    }

    /// `text[i]` or `text[i:j:k]` into `place`: a new `str` of the characters taken, after the
    /// checks that keep the index or range inside the text.
    ///
    /// Cost: one pass to count the characters, and one to copy the ones taken.
    pub(crate) fn text_subscript(&mut self, place: Place, node: NodeId) {
        let (Some(text), Some(index)) = (
            self.tree.child(node, Role::Value),
            self.tree.child(node, Role::Index),
        ) else {
            return;
        };
        let source = self.borrow(text);
        let int = self.int_type();
        let length = self.temporary(int, node);
        let borrowed = vec![Argument::Borrow(source.clone())];
        self.text_call(Place::from(length), Builtin::LengthStr, borrowed, node);
        let length = Operand::Copy(Place::from(length));
        let copy = |local| Operand::Copy(Place::from(local));
        let (start, count, step) = if self.tree.kind(index) == SyntaxKind::Slice {
            let SliceIndex::Range {
                start,
                length,
                step,
            } = self.range_within(&length, 0, index)
            else {
                return;
            };
            (copy(start), copy(length), copy(step))
        } else if self.tree.kind(index) == SyntaxKind::Tuple {
            return self.unsupported(index, "more than one index into a `str`");
        } else {
            let one = Operand::Constant(Constant::integer(int, 1));
            let at = self.index_within(Some(length), 0, index, false);
            (copy(at), one.clone(), one)
        };
        let arguments = vec![
            Argument::Borrow(source),
            Argument::Owned(start),
            Argument::Owned(count),
            Argument::Owned(step),
        ];
        self.text_call(place, Builtin::Text(TextMethod::Take), arguments, node);
    }

    /// `text.method(arguments…)` into `place`, `arguments` starting with the text: `split`,
    /// `strip`, `lstrip`, `rstrip`, `join`, `startswith`, `endswith`, `find`, `replace`,
    /// `lower` or `upper`, each argument borrowed, or `copy`, which builds a new `str`. `split`
    /// and the strips without an argument work on whitespace.
    pub(crate) fn text_method(
        &mut self,
        place: Place,
        method: &str,
        arguments: &[NodeId],
        node: NodeId,
    ) {
        let text_method = match (method, arguments.len()) {
            ("copy", 1) => return self.joined(place, arguments, node),
            ("split", 1) => TextMethod::SplitWhitespace,
            ("split", _) => TextMethod::Split,
            ("strip", _) => TextMethod::Strip,
            ("lstrip", _) => TextMethod::StripStart,
            ("rstrip", _) => TextMethod::StripEnd,
            ("join", _) => TextMethod::Join,
            ("startswith", _) => TextMethod::StartsWith,
            ("endswith", _) => TextMethod::EndsWith,
            ("find", _) => TextMethod::Find,
            ("replace", _) => TextMethod::Replace,
            ("lower", _) => TextMethod::Lower,
            ("upper", _) => TextMethod::Upper,
            _ => return self.unsupported(node, &format!("the `str` method `{method}`")),
        };
        let arguments = arguments
            .iter()
            .map(|&argument| Argument::Borrow(self.borrow(argument)))
            .collect();
        self.text_call(place, Builtin::Text(text_method), arguments, node);
    }

    /// `int(text)` or `float(text)` into `place`, of the number type `to`: a call that reads the
    /// borrowed text, and fails with the `ValueError` the checker recorded for `node`, which goes
    /// where any error of a call goes.
    pub(crate) fn parse_number(&mut self, place: Place, to: Type<'db>, text: NodeId, node: NodeId) {
        let method = if matches!(to.kind(self.db), TypeKind::Primitive(Primitive::Float(_))) {
            TextMethod::ParseFloat
        } else {
            TextMethod::ParseInt
        };
        let text = Argument::Borrow(self.borrow(text));
        let errors = self.typed.errors(node);
        let edge = self.error_edge(errors, node);
        let next = self.new_block();
        self.terminate(
            TerminatorKind::Call {
                callee: Callee::Builtin(Builtin::Text(method)),
                arguments: vec![text],
                destination: place,
                target: next,
                error: edge.clone(),
            },
            node,
        );
        if let Some(edge) = edge {
            self.dispatch(&edge, errors, node);
        }
        self.start(next);
    }
}
