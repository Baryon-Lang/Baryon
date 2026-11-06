//! Arrays (language reference, "Arrays"): creating them, their methods and attributes,
//! whole-array arithmetic and the matrix product, and the list methods.
//!
//! - `Array.zeros(shape)`, `Array.ones(shape)` and `Array.fill(shape, value)` create an `Array`
//!   whose rank is the length of the `shape` tuple. Its element type is the one the context
//!   needs, else the `dtype` argument, else `f64` (for `fill`, the value's type).
//! - `Vec3.zeros()`, `Vec3.ones()` and `Vec3.fill(value)` create a `Fixed` array an alias
//!   names, and `Mat3((1.0, 0.0), (0.0, 1.0))` builds one from nested tuples of its elements.
//! - Arithmetic works element by element on two arrays of the same rank (`Array`) or shape
//!   (`Fixed`), or on an array and a number; the number mixing rules apply to the elements.

use super::calls::Arguments;
use super::{Body, relations};
use crate::forms::is_copy;
use syntax::{Flags, NodeId, Operator, Role, SyntaxKind};
use types::{Float, Integer, Primitive, Shape, Type, TypeKind};

/// The methods version 0 defines on arrays, for messages.
const ARRAY_METHODS: &str = "`copy`, `sum`, `min`, `max`, `dot`, `round` and `fill`";

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
        if !matches!(name, "zeros" | "ones" | "fill") {
            self.untyped(arguments);
            let message = format!("`Array` has no function `{name}`");
            return self.lowerer.error(
                node,
                &message,
                "not defined",
                Some("arrays are created with `Array.zeros`, `Array.ones`, `Array.fill` or `Array(values)`"),
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
            TypeKind::List(item) => {
                let item = *item;
                return Some(match (name, arguments.positional.as_slice()) {
                    ("append", [value]) => {
                        let found = self.expression(*value, Some(item));
                        self.expect(*value, item, found, "the list holds");
                        nothing
                    }
                    ("pop", []) => item,
                    _ => {
                        self.untyped(arguments);
                        let message = format!("`{}` has no method `{name}`", self.show(base));
                        self.lowerer.error(
                            node,
                            &message,
                            "not defined",
                            Some("a list has `append` and `pop`, and works with `len`, indexing, slicing, `in` and loops"),
                        )
                    }
                });
            }
            _ => return None,
        };
        Some(match (name, arguments.positional.as_slice()) {
            ("copy" | "round", []) => base,
            ("sum" | "min" | "max", []) => element,
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
                let hint = format!("version 0 defines {ARRAY_METHODS}");
                self.lowerer
                    .error(node, &message, "not defined", Some(&hint))
            }
        })
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

    /// `left op right` where at least one side is an array, or `None` when neither is.
    pub(super) fn array_arithmetic(
        &mut self,
        node: NodeId,
        operator: Operator,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Option<Type<'db>> {
        use TypeKind::{Array, Fixed};
        let symbol = operator.symbol();
        if operator == Operator::MatrixMultiply {
            return Some(self.matrix_product(node, left, right));
        }
        if !matches!(
            operator,
            Operator::Add
                | Operator::Subtract
                | Operator::Multiply
                | Operator::Divide
                | Operator::FloorDivide
                | Operator::Modulo
                | Operator::Power
        ) {
            return None;
        }
        let scalar = |ty: Type<'db>| matches!(ty.kind(self.db), TypeKind::Primitive(_));
        let result = match (left.kind(self.db), right.kind(self.db)) {
            (
                Array {
                    element: a,
                    rank: m,
                },
                Array {
                    element: b,
                    rank: n,
                },
            ) => {
                if m != n {
                    return Some(self.shape_error(node, symbol, left, right));
                }
                let (a, b, rank) = (*a, *b, *m);
                let element = self.combine(node, operator, a, b);
                Array { element, rank }
            }
            (
                Fixed {
                    element: a,
                    shape: s,
                },
                Fixed {
                    element: b,
                    shape: t,
                },
            ) => {
                if s != t {
                    return Some(self.shape_error(node, symbol, left, right));
                }
                let (a, b, shape) = (*a, *b, s.clone());
                let element = self.combine(node, operator, a, b);
                Fixed { element, shape }
            }
            (Array { element, rank }, _) if scalar(right) => {
                let (element, rank) = (*element, *rank);
                let element = self.combine(node, operator, element, right);
                Array { element, rank }
            }
            (_, Array { element, rank }) if scalar(left) => {
                let (element, rank) = (*element, *rank);
                let element = self.combine(node, operator, left, element);
                Array { element, rank }
            }
            (Fixed { element, shape }, _) if scalar(right) => {
                let (element, shape) = (*element, shape.clone());
                let element = self.combine(node, operator, element, right);
                Fixed { element, shape }
            }
            (_, Fixed { element, shape }) if scalar(left) => {
                let (element, shape) = (*element, shape.clone());
                let element = self.combine(node, operator, left, element);
                Fixed { element, shape }
            }
            (Array { .. } | Fixed { .. }, _) | (_, Array { .. } | Fixed { .. }) => {
                return Some(self.shape_error(node, symbol, left, right));
            }
            _ => return None,
        };
        Some(Type::new(self.db, result))
    }

    /// `a @ b`: the matrix product of two matrices, or of a matrix and a vector.
    fn matrix_product(&mut self, node: NodeId, left: Type<'db>, right: Type<'db>) -> Type<'db> {
        use TypeKind::{Array, Fixed};
        let product = match (left.kind(self.db), right.kind(self.db)) {
            (
                Array {
                    element: a,
                    rank: 2,
                },
                Array { element: b, rank },
            ) if *rank <= 2 => {
                let (a, b, rank) = (*a, *b, *rank);
                let element = self.combine(node, Operator::Multiply, a, b);
                Some(Array { element, rank })
            }
            (
                Fixed {
                    element: a,
                    shape: s,
                },
                Fixed {
                    element: b,
                    shape: t,
                },
            ) if s.rank() == 2 && t.rank() <= 2 && s.lengths()[1] == t.lengths()[0] => {
                let mut lengths = vec![s.lengths()[0]];
                lengths.extend_from_slice(&t.lengths()[1..]);
                let (a, b) = (*a, *b);
                let element = self.combine(node, Operator::Multiply, a, b);
                Shape::new(lengths).map(|shape| Fixed { element, shape })
            }
            _ => None,
        };
        match product {
            Some(kind) => Type::new(self.db, kind),
            None => self.shape_error(node, "@", left, right),
        }
    }

    /// Reports two operands whose ranks or shapes do not match.
    fn shape_error(
        &mut self,
        node: NodeId,
        symbol: &str,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Type<'db> {
        let message = format!(
            "`{symbol}` cannot combine `{}` and `{}`",
            self.show(left),
            self.show(right)
        );
        let hint = if symbol == "@" {
            "`@` multiplies a matrix by a matrix or a vector whose lengths match"
        } else {
            "both arrays need the same rank, or the same shape for `Fixed` arrays, or one side a single number"
        };
        self.lowerer
            .error(node, &message, "shapes do not match", Some(hint))
    }
}
