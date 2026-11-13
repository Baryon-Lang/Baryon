//! Text arrays, `Array[str, 1]` (decision 0045): the one array whose elements are not a copy
//! type. It stores every element's bytes end to end in one buffer, so reading an element
//! borrows it from the array, as reading an element of a `list[str]` does.

use super::Body;
use super::calls::Arguments;
use syntax::{NodeId, Operator, Role, SyntaxKind};
use types::{Integer, Primitive, Type, TypeKind};

/// What an array of `str` can do today, for messages.
const BUILT: &str = "an array of `str` has `len`, indexing, slices, masks, comparisons, \
                     `in`, `for` loops, `copy()`, `unique()` and `nunique()` today; the rest \
                     comes with roadmap item 3.13.3";

impl<'db> Body<'db> {
    /// `names[i]` on a text array: the element, a `str` borrowed from the array. A slice,
    /// `names[1:3]`, is a new text array holding a copy of the elements it takes. More than
    /// one index is an error: the array has one dimension.
    pub(super) fn text_array_index(
        &mut self,
        node: NodeId,
        sliced: bool,
        indices: &[NodeId],
    ) -> Type<'db> {
        if indices.len() != 1 {
            return self.lowerer.error(
                node,
                "an array of `str` has one dimension, so it takes one index",
                "more than one index",
                Some("for example `names[i]`"),
            );
        }
        let kind = if sliced {
            TypeKind::TextArray
        } else {
            TypeKind::Str
        };
        Type::new(self.db, kind)
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

    /// A method call on a text array: `copy()` gives a new array with its own bytes,
    /// `unique()` the distinct elements in the order they first appear, and `nunique()` how
    /// many there are, an `i64` (decision 0045); no other method is built yet.
    pub(super) fn text_array_method(
        &mut self,
        node: NodeId,
        base: Type<'db>,
        name: &str,
        arguments: &Arguments,
    ) -> Type<'db> {
        if arguments.positional.is_empty() && arguments.keywords.is_empty() {
            match name {
                "copy" | "unique" => return base,
                "nunique" => {
                    let int = Primitive::Integer(Integer::I64);
                    return Type::new(self.db, TypeKind::Primitive(int));
                }
                _ => {}
            }
        }
        self.untyped(arguments);
        self.text_array_unbuilt(node, &format!("the method `{name}`"))
    }

    /// Reports a text array, or a value that holds one, shown by `print`, `str` or an
    /// f-string at `node`, which cannot write one yet.
    pub(super) fn shows_no_text_array(&mut self, node: NodeId, ty: Type<'db>) {
        if super::relations::holds_text_array(self.db, ty) {
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

    /// One comparison, `left operator right` at `node`, with a text array on either side.
    /// `==`, `!=`, `<`, `<=`, `>` and `>=` compare each element with the other side's, a
    /// text array or one `str`, and give an `Array[bool, 1]`; `text in names` and `text not
    /// in names` give one `bool`. Any other pairing is an error.
    pub(super) fn text_array_comparison(
        &mut self,
        node: NodeId,
        operator: Operator,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Type<'db> {
        let boolean = Type::new(self.db, TypeKind::Primitive(Primitive::Bool));
        let text = |ty: Type<'db>| matches!(ty.kind(self.db), TypeKind::Str);
        let unknown = |ty: Type<'db>| matches!(ty.kind(self.db), TypeKind::Unknown);
        if unknown(left) || unknown(right) {
            return self.lowerer.unknown();
        }
        let fits = match operator {
            Operator::In | Operator::NotIn => {
                if text(left) && self.is_text_array(right) {
                    return boolean;
                }
                false
            }
            // `is` with anything but `None` is reported where `is` is checked.
            Operator::Is | Operator::IsNot => return boolean,
            _ => {
                (self.is_text_array(left) || text(left))
                    && (self.is_text_array(right) || text(right))
            }
        };
        if fits {
            return Type::new(
                self.db,
                TypeKind::Array {
                    element: boolean,
                    rank: 1,
                },
            );
        }
        let message = format!(
            "`{}` cannot compare `{}` and `{}`",
            operator.symbol(),
            self.show(left),
            self.show(right)
        );
        self.lowerer.error(
            node,
            &message,
            "an array of `str`",
            Some(
                "an array of `str` is compared with another or with one `str`, such as \
                 `names == \"alpha\"`, and searched with `\"alpha\" in names`",
            ),
        )
    }
}
