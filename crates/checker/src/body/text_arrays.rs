//! Text arrays, `Array[str, 1]` (decision 0045): the one array whose elements are not a copy
//! type. It stores every element's bytes end to end in one buffer, so reading an element
//! borrows it from the array, as reading an element of a `list[str]` does.

use super::Body;
use super::calls::Arguments;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Type, TypeKind};

/// What an array of `str` can do today, for messages.
const BUILT: &str = "an array of `str` has `len`, indexing, `for` loops and `copy()` today; the \
                     rest comes with roadmap item 3.13.3";

impl<'db> Body<'db> {
    /// `names[i]` on a text array: the element, a `str` borrowed from the array. A slice, and
    /// more than one index, are errors: the array has one dimension, and a part of one is not
    /// built yet.
    pub(super) fn text_array_index(
        &mut self,
        node: NodeId,
        sliced: bool,
        indices: &[NodeId],
    ) -> Type<'db> {
        if sliced {
            return self.lowerer.error(
                node,
                "a slice of an array of `str` is not built yet",
                "a slice",
                Some("read one element at a time, such as `names[i]`"),
            );
        }
        if indices.len() != 1 {
            return self.lowerer.error(
                node,
                "an array of `str` has one dimension, so it takes one index",
                "more than one index",
                Some("for example `names[i]`"),
            );
        }
        Type::new(self.db, TypeKind::Str)
    }

    /// Whether `ty` is a text array.
    pub(super) fn is_text_array(&self, ty: Type<'db>) -> bool {
        matches!(ty.kind(self.db), TypeKind::TextArray)
    }

    /// Reports `what`, such as "`+`" or "the method `sum`", used at `node` on a text array,
    /// which does not have it yet, and gives the unknown type.
    pub(super) fn text_array_unbuilt(&mut self, node: NodeId, what: &str) -> Type<'db> {
        let message = format!("{what} is not built for an array of `str` yet");
        self.lowerer
            .error(node, &message, "an array of `str`", Some(BUILT))
    }

    /// A method call on a text array: `copy()` gives a new array with its own bytes; no other
    /// method is built yet.
    pub(super) fn text_array_method(
        &mut self,
        node: NodeId,
        base: Type<'db>,
        name: &str,
        arguments: &Arguments,
    ) -> Type<'db> {
        if name == "copy" && arguments.positional.is_empty() && arguments.keywords.is_empty() {
            return base;
        }
        self.untyped(arguments);
        self.text_array_unbuilt(node, &format!("the method `{name}`"))
    }

    /// Reports a text array, or a value that holds one, shown by `print`, `str` or an
    /// f-string at `node`, which cannot write one yet.
    pub(super) fn shows_no_text_array(&mut self, node: NodeId, ty: Type<'db>) {
        if self.has_text_column(ty) {
            self.text_column_unbuilt(node, "showing it");
        } else if super::relations::holds_text_array(self.db, ty) {
            self.lowerer.error(
                node,
                "an array of `str` cannot be shown yet",
                "an array of `str`",
                Some("show its elements one at a time: `for name in names: print(name)`"),
            );
        }
    }

    /// Reports the assignment whose target is `target` when it writes an element of a text
    /// array, `names[i] = "x"`, and returns whether it was one. The elements lie end to end
    /// in one buffer, so one cannot change length in place.
    pub(super) fn text_array_element_assigned(&mut self, target: NodeId) -> bool {
        let base = (self.tree.kind(target) == SyntaxKind::Subscript)
            .then(|| self.tree.child(target, Role::Value))
            .flatten()
            .and_then(|value| self.typed.expression(value));
        if !base.is_some_and(|base| self.is_text_array(base)) {
            return false;
        }
        self.lowerer.error(
            target,
            "an element of an array of `str` cannot be assigned yet",
            "assigned here",
            Some(
                "the elements are stored end to end, so one cannot change length in place; \
                 build a new array from a list, `Array(words)`",
            ),
        );
        true
    }
}
