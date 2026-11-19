//! Arrays with missing values, `Array[T | None, 1]` (decision 0045, part 3): one dimension of
//! numbers, `bool`s or text, any of which may be `None`. Reading an element gives a
//! `T | None`, which the program must check before it uses the value, so a missing value is
//! never read as a value by accident.

use super::Body;
use super::calls::Arguments;
use super::relations;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Integer, Primitive, Type, TypeKind};

/// What an array with missing values can do, for messages.
const BUILT: &str = "an array with missing values has `len`, `print`, `values[i]`, which gives a \
                     `T | None` and for numbers and `bool`s can be assigned, `for` loops, slices, \
                     masks and arrays of positions, the methods `isna`, `notna`, `dropna`, \
                     `fillna`, `copy`, `argsort`, `unique`, `nunique`, `count` and, for numbers, \
                     `sum`, `min`, `max`, `mean`, `var` and `std`, for `bool`s `sum`, `any` and \
                     `all`, for text `min` and `max`, the comparisons, and for numbers `+`, `-`, \
                     `*`, `/`, `//`, `%` and `**`; the rest comes with roadmap item 3.24";

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
    /// an `element | None`. A slice, `values[1:3]`, is a new array with missing values that
    /// holds a copy of the elements it takes, as a slice of a text array is. A mask and an
    /// array of positions are checked with the other choices of elements (the `selection`
    /// module). More than one index is an error.
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
            [_] if sliced => Type::new(self.db, TypeKind::OptionalArray(element)),
            [_] => self.optional_array_unbuilt(node, "this index"),
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
    /// true where it is not, as in pandas; `copy()` gives a new array with its own memory;
    /// `argsort()` gives the positions that put the elements in rising order with the
    /// `None`s last, an `Array[i64, 1]`; `unique()` gives the distinct elements, one `None`
    /// among them, and `nunique()` how many there are, a `None` not counted, as in pandas;
    /// and the reductions skip `None`. No other method is built.
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
        if plain && name == "unique" {
            return base;
        }
        if plain && matches!(name, "argsort" | "nunique") {
            let whole = TypeKind::Primitive(Primitive::Integer(Integer::I64));
            let int = Type::new(self.db, whole);
            if name == "nunique" {
                return int;
            }
            return Type::new(
                self.db,
                TypeKind::Array {
                    element: int,
                    rank: 1,
                },
            );
        }
        if let &TypeKind::OptionalArray(element) = base.kind(self.db) {
            if let Some(narrowed) = self.optional_array_narrowed(node, element, name, arguments) {
                return narrowed;
            }
            // `var` and `std` take `ddof=`, as they do on any array.
            let spread = matches!(name, "var" | "std")
                && arguments.positional.is_empty()
                && arguments
                    .keywords
                    .iter()
                    .all(|&(keyword, _)| self.tree.text(keyword, self.text) == "ddof");
            if (plain || spread)
                && let Some(reduced) = self.optional_array_reduced(node, element, name, arguments)
            {
                return reduced;
            }
        }
        self.untyped(arguments);
        self.optional_array_unbuilt(node, &format!("the method `{name}`"))
    }

    /// A reduction of an array with missing values whose elements are `element`s, the method
    /// `name` called at `node` with `arguments`, which are empty but for the `ddof=` of `var`
    /// and `std`. Each skips the elements that are `None`, as pandas does: `count()` is how
    /// many elements are there, an `i64`, and `sum()`, `min()`, `max()`, `mean()`, `var()`
    /// and `std()` of numbers give what the same method gives the array of the elements that
    /// are there, `values.dropna()`. A mask of `bool`s has `sum()`, how many elements are
    /// `True`, and `any()` and `all()`, which with no element there are `False` and `True`.
    /// Text has `min()` and `max()`, by Python's `<` on `str`.
    /// `None`, with nothing reported, for any other method or kind of element.
    fn optional_array_reduced(
        &mut self,
        node: NodeId,
        element: Type<'db>,
        name: &str,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        if name == "count" {
            let whole = Primitive::Integer(Integer::I64);
            return Some(Type::new(self.db, TypeKind::Primitive(whole)));
        }
        let built = match element.kind(self.db) {
            TypeKind::Primitive(Primitive::Integer(_) | Primitive::Float(_)) => {
                matches!(name, "sum" | "min" | "max" | "mean" | "var" | "std")
            }
            TypeKind::Primitive(Primitive::Bool) => matches!(name, "sum" | "any" | "all"),
            TypeKind::Str => matches!(name, "min" | "max"),
            _ => false,
        };
        if !built {
            return None;
        }
        let there = if matches!(element.kind(self.db), TypeKind::Str) {
            Type::new(self.db, TypeKind::TextArray)
        } else {
            Type::new(self.db, TypeKind::Array { element, rank: 1 })
        };
        self.container_method(node, there, name, arguments)
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
    /// array, or into an array with missing values in a way that is not built, and returns
    /// whether it was one.
    pub(super) fn array_element_unassignable(&mut self, target: NodeId) -> bool {
        self.text_array_element_assigned(target) || self.optional_array_element_assigned(target)
    }

    /// Reports the assignment whose target is `target` when it writes into an array with
    /// missing values in a way that is not built, and returns whether it did. Numbers and
    /// `bool`s are built: one element, `values[i] = 2.0` or `values[i] = None`, checked as
    /// any assignment to a `T | None` is, and the elements a slice, a mask, positions or a
    /// list chooses, `values[mask] = None`, which all take the one value. Text is not,
    /// since it is stored end to end.
    fn optional_array_element_assigned(&mut self, target: NodeId) -> bool {
        let base = (self.tree.kind(target) == SyntaxKind::Subscript)
            .then(|| self.tree.child(target, Role::Value))
            .flatten()
            .and_then(|value| self.typed.expression(value));
        let Some(TypeKind::OptionalArray(element)) = base.map(|base| base.kind(self.db)) else {
            return false;
        };
        // The target names one element when it is a `T | None`, and several when it is an
        // array; a target already reported has an open type.
        let one = self.typed.expression(target).is_none_or(|ty| {
            matches!(ty.kind(self.db), TypeKind::Optional(_)) || relations::is_open(self.db, ty)
        });
        if !matches!(element.kind(self.db), TypeKind::Str) {
            return false;
        }
        let what = if one {
            "assigning an element of text"
        } else {
            "assigning elements of text"
        };
        self.optional_array_unbuilt(target, what);
        true
    }

    /// Reports a value that holds an array with missing values, or a frame with a column
    /// that may hold `None`, inside another value, such as a tuple or a list, shown by
    /// `print` or an f-string at `node`, which cannot write one yet. An array with missing
    /// values alone is shown, and so is such a frame alone.
    pub(super) fn shows_no_optional_array(&mut self, node: NodeId, ty: Type<'db>) {
        let alone = self.is_optional_array(ty) || self.has_missing_column(ty);
        let holds = relations::holds_optional_array(self.db, ty)
            || relations::holds_missing_column(self.db, ty);
        if holds && !alone {
            self.lowerer.error(
                node,
                "a value that holds an array with missing values cannot be shown yet",
                "holds an array that may hold `None`",
                Some("show the array or the frame alone: `print(values)`"),
            );
        }
    }
}
