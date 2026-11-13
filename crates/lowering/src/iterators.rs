//! `for` loops over Python's iterator built-ins: `enumerate`, `zip` and `reversed`, alone or
//! inside each other, as in `for i, (x, y) in enumerate(zip(xs, reversed(ys)))`. The `loops`
//! module sends a `for` statement here, and the `comprehensions` module each clause of a
//! comprehension, when the iterator is a call of one of them.
//!
//! Each call becomes a [`Walk`]: what to read on each pass. There is no iterator object at run
//! time. The loop keeps one counter from zero, runs as many passes as its shortest sequence has
//! elements, and each pass reads every sequence at the counter: `zip` stops at the shortest
//! sequence, as in Python. `reversed` reads at `length - 1 - counter` instead, so it costs
//! nothing beyond the subtraction and never copies its sequence. `enumerate`'s count is
//! `start + counter`.
//!
//! The sequences are what a plain `for` loops over (the `collections` module): rank-1 arrays,
//! lists, tuples whose items have one type and `str`s, plus `range` with a constant step. Each
//! is computed once, before the loop, in the order written, and borrowed for the loop. An
//! element that is not copied, such as a `str` in a list, is borrowed in place as in a plain
//! loop, so the same rule holds: its variable may be used only inside the loops that bind it.
//! A `str` is read forward one character at a time, so `reversed` of a `str` is not lowered yet.

use crate::builder::Builder;
use crate::collections::Elements;
use crate::frame_rows::Rows;
use crate::loops::Pass;
use database::Definition;
use mir::{Argument, BinaryOperator, Builtin, Constant, Local, Operand, Place, Projection, Rvalue};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Type, TypeKind};

/// What one sequence of a loop reads on each pass.
enum Walk<'db> {
    /// The elements of an array, a list, a tuple or a `str`, borrowed at `collection`.
    Elements {
        /// The borrowed collection.
        collection: Place,
        /// How its elements are reached.
        elements: Elements,
        /// The type of one element.
        element: Type<'db>,
        /// The local holding the number of elements.
        length: Local,
        /// Whether the walk runs from the last element to the first.
        reversed: bool,
    },
    /// The numbers of `range(first, …, step)`, `length` of them, of type `ty`.
    Range {
        /// The first number.
        first: Local,
        /// The constant step between numbers.
        step: i128,
        /// The local holding how many numbers the range has.
        length: Local,
        /// Whether the walk runs from the last number to the first.
        reversed: bool,
        /// The integer type of the numbers.
        ty: Type<'db>,
    },
    /// `enumerate`'s count, `start + counter`, which never ends on its own.
    Count {
        /// The local holding the count of the first pass.
        start: Local,
    },
    /// `enumerate` or `zip`: one tuple whose items are the parts' values.
    Group(Vec<Walk<'db>>),
    /// The rows of `df.itertuples()`, walked from the last to the first when the flag is set.
    Rows(Rows<'db>, bool),
}

impl<'db> Builder<'db> {
    /// Whether `iterator` is a call of `enumerate`, `zip` or `reversed`, which
    /// [`Self::iterator_loop`] lowers.
    pub(crate) fn is_iterator_call(&self, iterator: NodeId) -> bool {
        matches!(
            self.builtin_called(iterator),
            Some("enumerate" | "zip" | "reversed")
        ) || self.itertuples_frame(iterator).is_some()
    }

    /// `for target in iterator: body` where `iterator` calls `enumerate`, `zip` or `reversed`
    /// (the module documentation). `statement` is the `for` statement or the comprehension
    /// clause, and `pass` runs its body.
    ///
    /// Cost: one pass per element of the shortest sequence, each a few integer operations per
    /// sequence beyond reading its element.
    pub(crate) fn iterator_loop(
        &mut self,
        statement: NodeId,
        target: NodeId,
        iterator: NodeId,
        pass: Pass<'_, 'db>,
    ) {
        let Some(walk) = self.walk(iterator, false) else {
            return;
        };
        let ty = self.walk_type(&walk);
        let int = self.int_type();
        let mut lengths = Vec::new();
        Self::walk_lengths(&walk, &mut lengths);
        let limit = self.temporary(int, iterator);
        let Some((&first, rest)) = lengths.split_first() else {
            return;
        };
        self.assign(
            Place::from(limit),
            Rvalue::Use(Operand::Copy(Place::from(first))),
            iterator,
        );
        for &length in rest {
            self.lower_limit(limit, length, iterator);
        }
        let bind = &mut |builder: &mut Self, counter| {
            let place = builder.walk_element(&walk, counter, target);
            builder.bind_element(target, place, ty);
        };
        self.counted_loop(statement, (iterator, limit), bind, pass);
    }

    /// The name of the built-in that `node` calls, or `None` when it calls none.
    fn builtin_called(&self, node: NodeId) -> Option<&'static str> {
        if self.tree.kind(node) != SyntaxKind::Call {
            return None;
        }
        match self.definition(self.tree.child(node, Role::Function)?)? {
            Definition::Builtin(name) => Some(name),
            _ => None,
        }
    }

    /// The positional arguments of the call `node`, in order, and its keyword arguments, each
    /// name with its value.
    pub(crate) fn call_arguments(&self, node: NodeId) -> (Vec<NodeId>, Vec<(&'db str, NodeId)>) {
        let (mut positional, mut keywords) = (Vec::new(), Vec::new());
        let items = self
            .tree
            .child(node, Role::Arguments)
            .into_iter()
            .flat_map(|list| self.tree.children_with(list, Role::Item));
        for item in items {
            if self.tree.kind(item) != SyntaxKind::Keyword {
                positional.push(item);
            } else if let (Some(name), Some(value)) = (
                self.tree.child(item, Role::Name),
                self.tree.child(item, Role::Value),
            ) {
                keywords.push((self.text_of(name), value));
            }
        }
        (positional, keywords)
    }

    /// The walk of the sequence `node`, read backward when `reversed`, after computing and
    /// borrowing what it reads. `None` after an error.
    fn walk(&mut self, node: NodeId, reversed: bool) -> Option<Walk<'db>> {
        if let Some(rows) = self.rows(node) {
            return Some(Walk::Rows(rows, reversed));
        }
        let builtin = self.builtin_called(node);
        let (positional, keywords) = self.call_arguments(node);
        match builtin {
            Some("enumerate") => {
                let (&sequence, rest) = positional.split_first()?;
                let walk = self.walk(sequence, false)?;
                let int = self.int_type();
                let start = self.temporary(int, node);
                let given = rest
                    .first()
                    .copied()
                    .or_else(|| keywords.first().map(|&(_, value)| value));
                match given {
                    Some(value) => self.write(Place::from(start), int, value),
                    None => self.assign(
                        Place::from(start),
                        Rvalue::Use(Operand::Constant(Constant::integer(int, 0))),
                        node,
                    ),
                }
                Some(Walk::Group(vec![Walk::Count { start }, walk]))
            }
            Some("zip") if keywords.is_empty() && !positional.is_empty() => positional
                .into_iter()
                .map(|sequence| self.walk(sequence, false))
                .collect::<Option<Vec<_>>>()
                .map(Walk::Group),
            Some("reversed") => self.walk(*positional.first()?, !reversed),
            Some("range") => self.range_walk(node, reversed),
            Some("zip" | "prange" | "simd") => {
                self.unsupported(
                    node,
                    "this iterator inside `enumerate`, `zip` or `reversed`",
                );
                None
            }
            _ => self.elements_walk(node, reversed),
        }
    }

    /// The walk of `range(…)` at `node`: its first number, and how many numbers it has, which
    /// is `ceil((stop - start) / step)` when that is above zero and zero otherwise, as Python's
    /// `len(range(…))` is. `None` after an error, or for a step known only at run time.
    fn range_walk(&mut self, node: NodeId, reversed: bool) -> Option<Walk<'db>> {
        let (positional, _) = self.call_arguments(node);
        let (first, bound, step) = match *positional.as_slice() {
            [bound] => (None, bound, Some(1)),
            [first, bound] => (Some(first), bound, Some(1)),
            [first, bound, stride] => (Some(first), bound, self.constant_integer(stride)),
            _ => None?,
        };
        let Some(step) = step.filter(|&step| step != 0) else {
            self.unsupported(
                node,
                "a `range` step that is not a constant inside `enumerate`, `zip` or `reversed`",
            );
            return None;
        };
        let ty = self.ty(first.unwrap_or(bound));
        let start = self.temporary(ty, node);
        match first {
            Some(first) => self.write(Place::from(start), ty, first),
            None => self.assign(
                Place::from(start),
                Rvalue::Use(Operand::Constant(Constant::integer(ty, 0))),
                node,
            ),
        }
        let end = self.temporary(ty, bound);
        self.write(Place::from(end), ty, bound);
        // The distance to cover in the step's direction, then the steps it takes, rounded up.
        let (from, to) = if step > 0 { (start, end) } else { (end, start) };
        let distance = self.binary_locals(BinaryOperator::Subtract, (to, from), ty, node);
        let length = self.temporary(self.int_type(), node);
        let zero = Rvalue::Use(Operand::Constant(Constant::integer(self.int_type(), 0)));
        self.assign(Place::from(length), zero, node);
        let positive = self.temporary(self.boolean_type(), node);
        self.assign(
            Place::from(positive),
            Rvalue::Binary(
                BinaryOperator::Greater,
                Operand::Copy(Place::from(distance)),
                Operand::Constant(Constant::integer(ty, 0)),
            ),
            node,
        );
        let (count, join) = (self.new_block(), self.new_block());
        self.branch(Operand::Copy(Place::from(positive)), (count, join), node);
        self.start(count);
        let size = step.unsigned_abs();
        let rounded = Rvalue::Binary(
            BinaryOperator::Add,
            Operand::Copy(Place::from(distance)),
            Operand::Constant(Constant::integer(ty, i128::try_from(size - 1).ok()?)),
        );
        self.assign(Place::from(distance), rounded, node);
        let steps = Rvalue::Binary(
            BinaryOperator::FloorDivide,
            Operand::Copy(Place::from(distance)),
            Operand::Constant(Constant::integer(ty, i128::try_from(size).ok()?)),
        );
        let counted = self.temporary(ty, node);
        self.assign(Place::from(counted), steps, node);
        let as_int = self.convert(
            Operand::Copy(Place::from(counted)),
            ty,
            self.int_type(),
            node,
        );
        self.assign(Place::from(length), Rvalue::Use(as_int), node);
        self.jump(&mut Some(join), node);
        self.start(join);
        Some(Walk::Range {
            first: start,
            step,
            length,
            reversed,
            ty,
        })
    }

    /// `left operator right` of two locals of type `ty`, into a new temporary.
    fn binary_locals(
        &mut self,
        operator: BinaryOperator,
        (left, right): (Local, Local),
        ty: Type<'db>,
        node: NodeId,
    ) -> Local {
        let result = self.temporary(ty, node);
        self.assign(
            Place::from(result),
            Rvalue::Binary(
                operator,
                Operand::Copy(Place::from(left)),
                Operand::Copy(Place::from(right)),
            ),
            node,
        );
        result
    }

    /// The walk of the collection `node`, computed once and borrowed, as a plain `for` loop
    /// over it is. `None` after an error.
    fn elements_walk(&mut self, node: NodeId, reversed: bool) -> Option<Walk<'db>> {
        let ty = self.ty(node);
        let element = match ty.kind(self.db) {
            _ if self.is_text_array(node) => Type::new(self.db, TypeKind::Str),
            TypeKind::Array { element, rank: 1 } | TypeKind::List(element) => *element,
            TypeKind::Fixed { element, shape } if shape.rank() == 1 => *element,
            TypeKind::Tuple(items) if !items.is_empty() => items[0],
            TypeKind::Str if !reversed => ty,
            TypeKind::Str => {
                self.unsupported(node, "`reversed` of a `str`");
                return None;
            }
            _ => {
                self.unsupported(node, "`for` loops over values of this type");
                return None;
            }
        };
        let collection = self.borrow(node);
        let (length, elements) = self.loop_elements(&collection, ty, node);
        let count = self.temporary(self.int_type(), node);
        self.assign(Place::from(count), Rvalue::Use(length), node);
        Some(Walk::Elements {
            collection,
            elements,
            element,
            length: count,
            reversed,
        })
    }

    /// The type of the value `walk` gives on each pass: a tuple for `enumerate` and `zip`.
    fn walk_type(&self, walk: &Walk<'db>) -> Type<'db> {
        match walk {
            Walk::Elements { element, .. } => *element,
            Walk::Range { ty, .. } => *ty,
            Walk::Count { .. } => self.int_type(),
            Walk::Rows(rows, _) => rows.row,
            Walk::Group(parts) => {
                let items = parts.iter().map(|part| self.walk_type(part)).collect();
                Type::new(self.db, TypeKind::Tuple(items))
            }
        }
    }

    /// Adds to `lengths` the local holding the length of each sequence in `walk` that ends.
    fn walk_lengths(walk: &Walk<'db>, lengths: &mut Vec<Local>) {
        match walk {
            Walk::Elements { length, .. } | Walk::Range { length, .. } => lengths.push(*length),
            Walk::Count { .. } => {}
            Walk::Rows(rows, _) => lengths.push(rows.length),
            Walk::Group(parts) => {
                for part in parts {
                    Self::walk_lengths(part, lengths);
                }
            }
        }
    }

    /// Lowers the `i64` at `limit` to the one at `length` when that is smaller.
    fn lower_limit(&mut self, limit: Local, length: Local, node: NodeId) {
        let shorter = self.temporary(self.boolean_type(), node);
        self.assign(
            Place::from(shorter),
            Rvalue::Binary(
                BinaryOperator::Less,
                Operand::Copy(Place::from(length)),
                Operand::Copy(Place::from(limit)),
            ),
            node,
        );
        let (lower, join) = (self.new_block(), self.new_block());
        self.branch(Operand::Copy(Place::from(shorter)), (lower, join), node);
        self.start(lower);
        self.assign(
            Place::from(limit),
            Rvalue::Use(Operand::Copy(Place::from(length))),
            node,
        );
        self.jump(&mut Some(join), node);
        self.start(join);
    }

    /// The position `walk` reads on the pass at `counter`: the counter itself, or
    /// `length - 1 - counter` when it runs backward.
    fn position(
        &mut self,
        counter: Local,
        (length, reversed): (Local, bool),
        node: NodeId,
    ) -> Local {
        if !reversed {
            return counter;
        }
        let int = self.int_type();
        let last = self.temporary(int, node);
        self.assign(
            Place::from(last),
            Rvalue::Binary(
                BinaryOperator::Subtract,
                Operand::Copy(Place::from(length)),
                Operand::Constant(Constant::integer(int, 1)),
            ),
            node,
        );
        self.binary_locals(BinaryOperator::Subtract, (last, counter), int, node)
    }

    /// The place holding the value of `walk` on the pass at `counter`, for the loop target
    /// `target`. An array's or list's element is read in place; every other value is written
    /// to a temporary first, and a value that is not copied is copied bit for bit into one that
    /// is never dropped, so the loop variable borrows it as a plain loop's does.
    fn walk_element(&mut self, walk: &Walk<'db>, counter: Local, target: NodeId) -> Place {
        match walk {
            Walk::Elements {
                collection,
                elements,
                element,
                length,
                reversed,
            } => {
                let at = self.position(counter, (*length, *reversed), target);
                match elements {
                    Elements::Indexed => collection.clone().project(Projection::Index(vec![at])),
                    Elements::TextArray => self.text_array_element(collection, at, target),
                    Elements::Tuple(count) => {
                        let item = self.temporary(*element, target);
                        self.tuple_item(target, (collection, *count), at, &mut |builder, place| {
                            builder.assign(
                                Place::from(item),
                                Rvalue::Use(Operand::Copy(place)),
                                target,
                            );
                        });
                        Place::from(item)
                    }
                    Elements::Text(cursor) => {
                        let character = self.temporary(*element, target);
                        self.text_call(
                            Place::from(character),
                            Builtin::CharacterStr,
                            vec![
                                Argument::Borrow(collection.clone()),
                                Argument::InOut(Place::from(*cursor)),
                            ],
                            target,
                        );
                        Place::from(character)
                    }
                }
            }
            Walk::Range {
                first,
                step,
                length,
                reversed,
                ty,
            } => {
                let at = self.position(counter, (*length, *reversed), target);
                let offset =
                    self.convert(Operand::Copy(Place::from(at)), self.int_type(), *ty, target);
                let value = self.temporary(*ty, target);
                self.assign(
                    Place::from(value),
                    Rvalue::Binary(
                        BinaryOperator::Multiply,
                        offset,
                        Operand::Constant(Constant::integer(*ty, *step)),
                    ),
                    target,
                );
                self.assign(
                    Place::from(value),
                    Rvalue::Binary(
                        BinaryOperator::Add,
                        Operand::Copy(Place::from(*first)),
                        Operand::Copy(Place::from(value)),
                    ),
                    target,
                );
                Place::from(value)
            }
            Walk::Count { start } => {
                let int = self.int_type();
                Place::from(self.binary_locals(BinaryOperator::Add, (*start, counter), int, target))
            }
            Walk::Rows(rows, reversed) => {
                let at = self.position(counter, (rows.length, *reversed), target);
                self.row_at(rows, at, target)
            }
            Walk::Group(parts) => {
                let operands = parts
                    .iter()
                    .map(|part| Operand::Copy(self.walk_element(part, counter, target)))
                    .collect();
                let ty = self.walk_type(walk);
                let tuple = self.temporary(ty, target);
                self.assign(Place::from(tuple), Rvalue::Aggregate(ty, operands), target);
                Place::from(tuple)
            }
        }
    }
}
