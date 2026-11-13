//! Arrays with missing values, `Array[T | None, 1]` (decision 0045, part 3): one dimension of
//! numbers or `bool`s, any of which may be `None`. Reading an element gives a `T | None`,
//! which the program must check before it uses the number, so a missing value is never read
//! as a number by accident.

use super::Body;
use super::calls::Arguments;
use super::relations;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Primitive, Type, TypeKind};

/// What an array with missing values can do, for messages.
const BUILT: &str = "an array with missing values has `len`, `values[i]`, which gives a \
                     `T | None`, and the methods `isna`, `notna`, `dropna`, `fillna` and `copy`; \
                     the rest comes with roadmap item 3.13.6";

/// What a frame with a column that may hold `None` can do, for messages.
const BUILT_FRAME: &str = "a frame with a column that may hold `None` is built from its \
                           columns, counted with `len`, copied with `copy`, narrowed by \
                           `dropna` and `fillna`, and its columns are read, replaced and \
                           chosen by name; the rest comes with roadmap item 3.13.6";

impl<'db> Body<'db> {
    /// Whether `ty` is a frame with a column that may hold `None`.
    pub(super) fn has_missing_column(&self, ty: Type<'db>) -> bool {
        match ty.kind(self.db) {
            TypeKind::DataFrame { columns, .. } => columns
                .iter()
                .any(|column| matches!(column.element.kind(self.db), TypeKind::Optional(_))),
            _ => false,
        }
    }

    /// Reports `what`, such as "the method `head`" or "choosing rows", used at `node` on a
    /// frame with a column that may hold `None`, which does not have it yet, and gives the
    /// unknown type.
    pub(super) fn missing_column_unbuilt(&mut self, node: NodeId, what: &str) -> Type<'db> {
        let message =
            format!("{what} is not built for a frame with a column that may hold `None` yet");
        self.lowerer.error(
            node,
            &message,
            "a column may hold `None`",
            Some(BUILT_FRAME),
        )
    }

    /// Whether `ty` is an array with missing values.
    pub(super) fn is_optional_array(&self, ty: Type<'db>) -> bool {
        matches!(ty.kind(self.db), TypeKind::OptionalArray(_))
    }

    /// Reports `what`, such as "`+`" or "the method `sum`", used at `node` on an array with
    /// missing values, which does not have it yet, and gives the unknown type.
    pub(super) fn optional_array_unbuilt(&mut self, node: NodeId, what: &str) -> Type<'db> {
        let message = format!("{what} is not built for an array with missing values yet");
        self.lowerer
            .error(node, &message, "an array that may hold `None`", Some(BUILT))
    }

    /// `values[i]` on an array with missing values whose elements are `element`s: the element,
    /// an `element | None`. A slice, a mask and more than one index are not built.
    pub(super) fn optional_array_index(
        &mut self,
        node: NodeId,
        element: Type<'db>,
        sliced: bool,
        indices: &[NodeId],
    ) -> Type<'db> {
        let whole = |body: &Self, index: &NodeId| {
            body.typed.expression(*index).is_some_and(|ty| {
                matches!(ty.kind(body.db), TypeKind::Primitive(Primitive::Integer(_)))
                    || relations::is_open(body.db, ty)
            })
        };
        match indices {
            [index] if !sliced && whole(self, index) => {
                Type::new(self.db, TypeKind::Optional(element))
            }
            [_] => self.optional_array_unbuilt(node, "a slice, a mask or a list of positions"),
            _ => self.lowerer.error(
                node,
                "an array with missing values has one dimension, so it takes one index",
                "more than one index",
                Some("for example `values[i]`"),
            ),
        }
    }

    /// A method call on the array with missing values of type `base`: `isna()` gives an
    /// `Array[bool, 1]` that is true where an element is `None`, and `notna()` one that is
    /// true where it is not, as in pandas; `copy()` gives a new array with its own memory. No
    /// other method is built.
    pub(super) fn optional_array_method(
        &mut self,
        node: NodeId,
        base: Type<'db>,
        name: &str,
        arguments: &Arguments,
    ) -> Type<'db> {
        let plain = arguments.positional.is_empty() && arguments.keywords.is_empty();
        if plain && name == "copy" {
            return base;
        }
        if plain && matches!(name, "isna" | "notna") {
            let element = Type::new(self.db, TypeKind::Primitive(Primitive::Bool));
            return Type::new(self.db, TypeKind::Array { element, rank: 1 });
        }
        if let &TypeKind::OptionalArray(element) = base.kind(self.db)
            && let Some(narrowed) = self.optional_array_narrowed(node, element, name, arguments)
        {
            return narrowed;
        }
        self.untyped(arguments);
        self.optional_array_unbuilt(node, &format!("the method `{name}`"))
    }

    /// Reports the call of the built-in function `name` when one of its arguments, the nodes
    /// `all` of the types `types`, is a text array or an array with missing values that the
    /// built-in does not take yet, and gives the unknown type; `None` when it takes them all.
    /// `Array(...)` never takes either.
    pub(super) fn array_builtin_unbuilt(
        &mut self,
        name: &str,
        all: &[NodeId],
        types: &[Type<'db>],
    ) -> Option<Type<'db>> {
        let what = format!("`{name}`");
        let text = ["len", "enumerate", "zip", "reversed", "print", "str"];
        if !text.contains(&name)
            && let Some(index) = types.iter().position(|&ty| self.is_text_array(ty))
        {
            return Some(self.text_array_unbuilt(all[index], &what));
        }
        if !matches!(name, "len" | "print")
            && let Some(index) = types.iter().position(|&ty| self.is_optional_array(ty))
        {
            return Some(self.optional_array_unbuilt(all[index], &what));
        }
        None
    }

    /// Reports the assignment whose target is `target` when it writes an element of a text
    /// array or of an array with missing values, neither of which is built, and returns
    /// whether it was one.
    pub(super) fn array_element_unassignable(&mut self, target: NodeId) -> bool {
        self.text_array_element_assigned(target) || self.optional_array_element_assigned(target)
    }

    /// Reports the assignment whose target is `target` when it writes an element of an array
    /// with missing values, `values[i] = 2.0`, which is not built yet, and returns whether it
    /// was one.
    fn optional_array_element_assigned(&mut self, target: NodeId) -> bool {
        let base = (self.tree.kind(target) == SyntaxKind::Subscript)
            .then(|| self.tree.child(target, Role::Value))
            .flatten()
            .and_then(|value| self.typed.expression(value));
        if !base.is_some_and(|base| self.is_optional_array(base)) {
            return false;
        }
        self.optional_array_unbuilt(target, "assigning an element");
        true
    }

    /// Reports a value that is or holds an array with missing values, or a frame with a
    /// column that may hold `None`, shown by `print` or an f-string at `node`, which cannot
    /// write one yet.
    pub(super) fn shows_no_optional_array(&mut self, node: NodeId, ty: Type<'db>) {
        if relations::holds_optional_array(self.db, ty) {
            self.lowerer.error(
                node,
                "an array with missing values cannot be shown yet",
                "holds an array that may hold `None`",
                Some("show its elements: `for at in range(len(values)): print(values[at])`"),
            );
        }
        if self.has_missing_column(ty) {
            self.missing_column_unbuilt(node, "showing it");
        }
    }
}
