//! Arrays (language reference, "Arrays"): creating them, their methods and attributes, and
//! the list methods.
//!
//! - `Array.zeros(shape)`, `Array.ones(shape)` and `Array.fill(shape, value)` create an `Array`
//!   whose rank is the length of the `shape` tuple. Its element type is the one the context
//!   needs, else the `dtype` argument, else `f64` (for `fill`, the value's type).
//! - `Array.fromfile(path, dtype)` and `a.tofile(path)` are in the `array_files` module.
//! - `Vec3.zeros()`, `Vec3.ones()` and `Vec3.fill(value)` create a `Fixed` array an alias
//!   names, and `Mat3((1.0, 0.0), (0.0, 1.0))` builds one from nested tuples of its elements.
//! - Arithmetic on arrays and the matrix product are in the `elementwise` module.
//! - `a.sum(axis=k)`, `a.min(axis=k)` and `a.max(axis=k)` are in the `axes` module.
//! - `a.mean()`, `a.var()` and `a.std()` are in the `statistics` module.

use super::calls::Arguments;
use super::{Body, relations};
use crate::forms::is_copy;
use syntax::{Flags, NodeId, Role, SyntaxKind};
use types::{Float, Integer, Primitive, Type, TypeKind};

/// The methods version 0 defines on a `Fixed` array, for messages.
const FIXED_METHODS: &str =
    "`copy`, `sum`, `min`, `max`, `mean`, `var`, `std`, `dot`, `round` and `fill`";

/// The methods version 0 defines on an `Array`, for messages.
const ARRAY_METHODS: &str = concat!(
    "`copy`, `sum`, `min`, `max`, `mean`, `var`, `std`, `dot`, `round`, `fill`, `tofile` and, ",
    "for one dimension, `argsort`, `unique` and `nunique`"
);

impl<'db> Body<'db> {
    /// `Array.zeros(shape)`, `Array.ones(shape)`, `Array.fill(shape, value)`, each with an
    /// optional `dtype`. `expected` is the type the context needs.
    pub(super) fn array_function(
        &mut self,
        node: NodeId,
        name: &str,
        arguments: &Arguments,
        expected: Option<Type<'db>>,
    ) -> Type<'db> {
        match name {
            "fromfile" => return self.array_fromfile(node, arguments, expected),
            "loadtxt" => return self.array_loadtxt(node, arguments, expected),
            "savetxt" => return self.array_savetxt(node, arguments),
            "load" => return self.array_load(node, arguments, expected),
            "save" => return self.array_save(node, arguments),
            _ => {}
        }
        if !matches!(name, "zeros" | "ones" | "fill") {
            self.untyped(arguments);
            let message = format!("`Array` has no function `{name}`");
            return self.lowerer.error(
                node,
                &message,
                "not defined",
                Some("arrays are created with `Array.zeros`, `Array.ones`, `Array.fill`, `Array(values)`, `Array.fromfile`, `Array.loadtxt` or `Array.load`"),
            );
        }
        let mut positional = arguments.positional.iter().copied();
        let Some(shape) = positional.next() else {
            let message = format!("`Array.{name}` needs a shape, such as `(nx, ny)`");
            return self.lowerer.error(node, &message, "no shape", None);
        };
        let rank = self.shape(shape);
        let fill_value = if name == "fill" {
            positional.next()
        } else {
            None
        };
        let dtype = positional.next().or_else(|| {
            arguments
                .keywords
                .iter()
                .find(|&&(keyword, _)| self.tree.text(keyword, self.text) == "dtype")
                .map(|&(_, value)| value)
        });
        let context = match expected.map(|ty| ty.kind(self.db)) {
            Some(TypeKind::Array { element, .. }) => Some(*element),
            _ => None,
        };
        let dtype = dtype.map(|dtype| self.lowerer.lower(dtype));
        let element = match (fill_value, context.or(dtype)) {
            (Some(value), wanted) => {
                let found = self.expression(value, wanted);
                if let Some(wanted) = wanted {
                    self.expect(value, wanted, found, "the array's elements are");
                    wanted
                } else {
                    found
                }
            }
            (None, Some(wanted)) => wanted,
            (None, None) => Type::new(self.db, TypeKind::Primitive(Primitive::Float(Float::F64))),
        };
        if name == "fill" && fill_value.is_none() {
            return self.lowerer.error(
                node,
                "`Array.fill` needs a shape and a value",
                "no value",
                Some("for example `Array.fill((n,), 1.0)`"),
            );
        }
        if matches!(element.kind(self.db), TypeKind::Str) {
            let message = format!("`Array.{name}` cannot make an array of `str` yet");
            return self.lowerer.error(
                fill_value.unwrap_or(node),
                &message,
                "an array of `str`",
                Some("make one from a list of strings: `Array(words)`"),
            );
        }
        if !is_copy(self.db, element) {
            let message = format!(
                "an array's elements must be a copy type, found `{}`",
                self.show(element)
            );
            return self.lowerer.error(
                fill_value.unwrap_or(node),
                &message,
                "not a copy type",
                Some("a copy type is a number, `bool`, a `Fixed` array, an enum, or a struct declared `Copy`; use a `list` for other values"),
            );
        }
        match rank {
            Some(rank) => Type::new(self.db, TypeKind::Array { element, rank }),
            None => self.lowerer.unknown(),
        }
    }

    /// The rank a shape argument gives: the length of a tuple written out, or 1 for a single
    /// integer. Reports a shape that is neither, and an entry that is not an integer.
    fn shape(&mut self, shape: NodeId) -> Option<u32> {
        let entries: Vec<NodeId> = if self.tree.kind(shape) == SyntaxKind::Tuple {
            self.tree.children_with(shape, Role::Item).collect()
        } else {
            vec![shape]
        };
        let int = Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::I64)),
        );
        for &entry in &entries {
            let found = self.expression(entry, Some(int));
            let is_integer = matches!(
                found.kind(self.db),
                TypeKind::Primitive(Primitive::Integer(_))
            );
            if !is_integer && !relations::is_open(self.db, found) {
                let message = format!(
                    "a shape's lengths are integers, found `{}`",
                    self.show(found)
                );
                self.lowerer.error(
                    entry,
                    &message,
                    "not an integer",
                    Some("a shape is a tuple of integers written out, such as `(nx, ny)`"),
                );
                return None;
            }
        }
        let rank = u32::try_from(entries.len()).ok().filter(|&rank| rank >= 1);
        if rank.is_none() {
            self.lowerer.error(
                shape,
                "a shape needs at least one length",
                "empty shape",
                Some("a shape is a tuple of integers written out, such as `(nx, ny)`"),
            );
        }
        rank
    }

    /// `Array(values)`: a rank-1 array with the elements of a list.
    pub(super) fn array_from_list(
        &mut self,
        node: NodeId,
        arguments: &Arguments,
        expected: Option<Type<'db>>,
    ) -> Type<'db> {
        let wanted = match expected.map(|ty| ty.kind(self.db)) {
            Some(TypeKind::Array { element, rank: 1 }) => {
                Some(Type::new(self.db, TypeKind::List(*element)))
            }
            Some(TypeKind::TextArray) => {
                let text = Type::new(self.db, TypeKind::Str);
                Some(Type::new(self.db, TypeKind::List(text)))
            }
            Some(TypeKind::OptionalArray(element)) => {
                let item = Type::new(self.db, TypeKind::Optional(*element));
                Some(Type::new(self.db, TypeKind::List(item)))
            }
            _ => None,
        };
        let [values] = arguments.positional.as_slice() else {
            self.untyped(arguments);
            return self.lowerer.error(
                node,
                "`Array(...)` takes one list",
                "wrong arguments",
                Some("for example `Array(values)` with `values: list[f64]`"),
            );
        };
        let found = self.expression(*values, wanted);
        match found.kind(self.db) {
            TypeKind::List(element) if is_copy(self.db, *element) => Type::new(
                self.db,
                TypeKind::Array {
                    element: *element,
                    rank: 1,
                },
            ),
            // A list whose items may be `None` gives an array with missing values.
            TypeKind::List(item) if self.lowerer.optional_element(*item).is_some() => {
                let element = self.lowerer.optional_element(*item).unwrap_or(*item);
                Type::new(self.db, TypeKind::OptionalArray(element))
            }
            // A list of `str` gives a text array (decision 0045), which reads the list.
            TypeKind::List(element) if matches!(element.kind(self.db), TypeKind::Str) => {
                Type::new(self.db, TypeKind::TextArray)
            }
            TypeKind::List(element) => {
                let message = format!(
                    "an array's elements must be a copy type, found `{}`",
                    self.show(*element)
                );
                self.lowerer
                    .error(*values, &message, "not a copy type", None)
            }
            _ if relations::is_open(self.db, found) => self.lowerer.unknown(),
            _ => {
                let message = format!("`Array(...)` takes a list, found `{}`", self.show(found));
                self.lowerer.error(*values, &message, "not a list", None)
            }
        }
    }

    /// `Vec3.zeros()`, `Vec3.ones()`, `Vec3.fill(value)` on a `Fixed` type.
    pub(super) fn fixed_function(
        &mut self,
        node: NodeId,
        fixed: Type<'db>,
        name: &str,
        arguments: &Arguments,
    ) -> Type<'db> {
        let TypeKind::Fixed { element, .. } = fixed.kind(self.db) else {
            return self.untyped(arguments);
        };
        let element = *element;
        match (name, arguments.positional.as_slice()) {
            ("zeros" | "ones", []) if arguments.keywords.is_empty() => fixed,
            ("fill", [value]) => {
                let found = self.expression(*value, Some(element));
                self.expect(*value, element, found, "the array's elements are");
                fixed
            }
            _ => {
                self.untyped(arguments);
                let message = format!(
                    "`{}` has no function `{name}` taking these arguments",
                    self.show(fixed)
                );
                self.lowerer.error(
                    node,
                    &message,
                    "not defined",
                    Some("a `Fixed` array is created with `zeros()`, `ones()`, `fill(value)`, or by calling its type with its elements"),
                )
            }
        }
    }

    /// Checks the elements of `Mat3((1.0, 0.0), (0.0, 1.0))` against a `Fixed` array's
    /// shape: `items` are the values of the outermost dimension, `lengths` the remaining shape.
    /// Returns whether the count of every dimension matched; an element of the wrong type is
    /// reported where it is.
    pub(super) fn fixed_elements(
        &mut self,
        items: &[NodeId],
        element: Type<'db>,
        lengths: &[u64],
    ) -> bool {
        let Some((&length, inner)) = lengths.split_first() else {
            return true;
        };
        if u64::try_from(items.len()).ok() != Some(length) {
            for &item in items {
                self.expression(item, None);
            }
            return false;
        }
        let mut matched = true;
        for &item in items {
            if inner.is_empty() {
                let found = self.expression(item, Some(element));
                self.expect(item, element, found, "each element is");
            } else if self.tree.kind(item) == SyntaxKind::Tuple
                && self.tree.node(item).flags().contains(Flags::PARENTHESIZED)
            {
                let row: Vec<NodeId> = self.tree.children_with(item, Role::Item).collect();
                matched &= self.fixed_elements(&row, element, inner);
            } else {
                self.expression(item, None);
                matched = false;
            }
        }
        matched
    }

    /// A method call on an array or a list, `a.dot(b)` or `values.append(x)`, or `None` when
    /// `base` is neither.
    pub(super) fn container_method(
        &mut self,
        node: NodeId,
        base: Type<'db>,
        name: &str,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        let nothing = Type::new(self.db, TypeKind::None);
        let (element, rank_one) = match base.kind(self.db) {
            TypeKind::Array { element, rank } => (*element, *rank == 1),
            TypeKind::Fixed { element, shape } => (*element, shape.rank() == 1),
            TypeKind::DataFrame { .. } => {
                return Some(self.frame_method(node, base, name, arguments));
            }
            TypeKind::TextArray => {
                return Some(self.text_array_method(node, base, name, arguments));
            }
            TypeKind::OptionalArray(_) => {
                return Some(self.optional_array_method(node, base, name, arguments));
            }
            TypeKind::List(item) => {
                let item = *item;
                return Some(match (name, arguments.positional.as_slice()) {
                    ("append", [value]) => {
                        let found = self.expression(*value, Some(item));
                        self.consume(*value, found);
                        self.expect(*value, item, found, "the list holds");
                        nothing
                    }
                    ("pop", []) => item,
                    ("copy", []) => base,
                    _ => {
                        self.untyped(arguments);
                        let message = format!("`{}` has no method `{name}`", self.show(base));
                        self.lowerer.error(
                            node,
                            &message,
                            "not defined",
                            Some("a list has `append`, `pop` and `copy`, and works with `len`, indexing, slicing, `in` and loops"),
                        )
                    }
                });
            }
            _ => return None,
        };
        let is_array = matches!(base.kind(self.db), TypeKind::Array { .. });
        if name == "tofile" && is_array {
            return Some(self.array_tofile(node, arguments));
        }
        if name == "decode" && is_array {
            return Some(self.decode(node, base, arguments));
        }
        if matches!(name, "mean" | "var" | "std") {
            return Some(self.statistic(node, (base, element), name, arguments));
        }
        Some(match (name, arguments.positional.as_slice()) {
            ("copy" | "round", []) => base,
            ("sum" | "min" | "max", []) if arguments.keywords.is_empty() => element,
            ("sum" | "min" | "max", _) => {
                let axis = self.axis_argument(arguments);
                self.axis_reduction(node, (base, element), name, axis, arguments)
            }
            ("argsort" | "unique" | "nunique", [])
                if is_array && rank_one && arguments.keywords.is_empty() =>
            {
                self.compared(node, name, base, element)
            }
            ("dot", [other]) => {
                let found = self.expression(*other, Some(base));
                self.expect(*other, base, found, "the dot product needs");
                if !rank_one {
                    let message = format!("`dot` needs a vector, found `{}`", self.show(base));
                    return Some(self.lowerer.error(
                        node,
                        &message,
                        "not rank 1",
                        Some("for a matrix product, use `@`"),
                    ));
                }
                element
            }
            ("fill", [value]) => {
                let found = self.expression(*value, Some(element));
                self.expect(*value, element, found, "the array's elements are");
                nothing
            }
            _ => {
                self.untyped(arguments);
                let message = format!(
                    "`{}` has no method `{name}` taking these arguments",
                    self.show(base)
                );
                let methods = if is_array {
                    ARRAY_METHODS
                } else {
                    FIXED_METHODS
                };
                let hint = format!("version 0 defines {methods}");
                self.lowerer
                    .error(node, &message, "not defined", Some(&hint))
            }
        })
    }

    /// A method of the rank-1 array `base` of `element` that compares its elements:
    /// `a.argsort()`, the positions that put them in rising order, an `Array[i64, 1]`;
    /// `a.unique()`, the distinct elements in the order they first appear, an array like
    /// `base`; or `a.nunique()`, how many distinct elements there are, an `i64` (decision
    /// 0045). The comparison is a sort, and complex numbers have no order.
    fn compared(
        &mut self,
        node: NodeId,
        name: &str,
        base: Type<'db>,
        element: Type<'db>,
    ) -> Type<'db> {
        if matches!(
            element.kind(self.db),
            TypeKind::Primitive(Primitive::Complex(_))
        ) {
            let message = format!(
                "`{name}` needs elements with an order, found `{}`",
                self.show(base)
            );
            return self.lowerer.error(node, &message, "complex numbers", None);
        }
        let int = Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::I64)),
        );
        match name {
            "unique" => base,
            "nunique" => int,
            _ => Type::new(
                self.db,
                TypeKind::Array {
                    element: int,
                    rank: 1,
                },
            ),
        }
    }

    /// `a.shape` or `a.size` on an array, or `None` when `base` is not an array.
    pub(super) fn array_attribute(
        &mut self,
        node: NodeId,
        base: Type<'db>,
        name: &str,
    ) -> Option<Type<'db>> {
        let rank = match base.kind(self.db) {
            TypeKind::Array { rank, .. } => *rank as usize,
            TypeKind::Fixed { shape, .. } => shape.rank(),
            _ => return None,
        };
        let int = Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::I64)),
        );
        Some(match name {
            "shape" => Type::new(self.db, TypeKind::Tuple(vec![int; rank])),
            "size" => int,
            // `a.unchecked[i]` inside `with unsafe:` reads like `a[i]`.
            "unchecked" => base,
            _ => {
                let message = format!("`{}` has no attribute `{name}`", self.show(base));
                self.lowerer.error(
                    node,
                    &message,
                    "not defined",
                    Some("an array has `shape` and `size`"),
                )
            }
        })
    }
}
