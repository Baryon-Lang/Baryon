//! Lists (language reference, "Lists"): a display `[a, b]` builds one, `xs.append(value)`
//! grows one, `len(xs)`, `xs[i]` and `value in xs` read one, `xs.copy()` and `xs[i:j:k]` copy
//! one or part of one into a new list, `xs[i:j] = ys` replaces part of one, and `Array(xs)`
//! copies one into a new array.
//!
//! The elements can be of any type. A list of `str`s, arrays, structs or other lists owns its
//! elements: a display or `append` moves each value in, writing `xs[i] = v` drops the element
//! it replaces, and dropping the list drops every element. An element is read in place, as a
//! borrow, since the checker rejects moving one out.

use crate::builder::Builder;
use mir::{
    Argument, BinaryOperator, Builtin, Callee, Constant, ConstantValue, Local, Operand, Piece,
    Place, Projection, Rvalue, SliceIndex, TerminatorKind, UnaryOperator,
};
use syntax::{NodeId, Operator, Role, SyntaxKind};
use types::{FormatSpec, Type, TypeKind};

impl<'db> Builder<'db> {
    /// The list display `node`, such as `[0]` or `[]`, into `place`: one
    /// [`Aggregate`](Rvalue::Aggregate) of its items, each converted to the element type, so
    /// `[0]` in a `list[f64]` holds `0.0`.
    pub(crate) fn list_display(&mut self, place: Place, node: NodeId) {
        let ty = self.ty(node);
        let Some(element) = self.list_element(ty, node) else {
            return;
        };
        let items: Vec<NodeId> = self.tree.children_with(node, Role::Item).collect();
        let operands = items
            .into_iter()
            .map(|item| self.operand_as(item, element))
            .collect();
        self.assign(place, Rvalue::Aggregate(ty, operands), node);
    }

    /// `xs.append(value)`, its `None` result written into `place`: a call of
    /// [`Builtin::Append`] with the list `InOut` and the value, as the element type, `Owned`.
    pub(crate) fn list_append(&mut self, place: Place, list: NodeId, value: NodeId, node: NodeId) {
        let Some(element) = self.list_element(self.ty(list), node) else {
            return;
        };
        let value = self.operand_as(value, element);
        let Some(list) = self.changed_place(list) else {
            return;
        };
        let next = self.new_block();
        self.terminate(
            TerminatorKind::Call {
                callee: Callee::Builtin(Builtin::Append),
                arguments: vec![Argument::InOut(list), Argument::Owned(value)],
                destination: place,
                target: next,
                error: None,
            },
            node,
        );
        self.start(next);
    }

    /// `xs.pop()` into `place`: a call of [`Builtin::Pop`] with the list `InOut`, which moves
    /// the last element out and panics on an empty list, as Python's `IndexError` stops a
    /// program that does not catch it.
    pub(crate) fn list_pop(&mut self, place: Place, list: NodeId, node: NodeId) {
        if self.list_element(self.ty(list), node).is_none() {
            return;
        }
        let Some(list) = self.changed_place(list) else {
            return;
        };
        let next = self.new_block();
        self.terminate(
            TerminatorKind::Call {
                callee: Callee::Builtin(Builtin::Pop),
                arguments: vec![Argument::InOut(list)],
                destination: place,
                target: next,
                error: None,
            },
            node,
        );
        self.start(next);
    }

    /// `Array(xs)` into `place`: a new one-dimensional array as long as the list, then a loop
    /// that copies each element in order. The list is borrowed, so it stays usable.
    ///
    /// Cost: one allocation and one pass over the elements.
    pub(crate) fn array_from_list(&mut self, place: &Place, list: NodeId, node: NodeId) {
        let ty = self.ty(list);
        if self.list_element(ty, node).is_none() {
            return;
        }
        let source = self.borrow(list);
        let length = self.length(&source, ty, 0, node);
        let array = self.ty(node);
        self.assign(
            place.clone(),
            Rvalue::NewArray(array, vec![length.clone()]),
            node,
        );
        // Each index is below the length the loop runs to, so neither step needs a check.
        self.loop_nest(&[length], &mut Vec::new(), node, &mut |builder, indices| {
            let from = source.clone().project(Projection::Index(indices.to_vec()));
            let to = place.clone().project(Projection::Index(indices.to_vec()));
            builder.assign(to, Rvalue::Use(Operand::Copy(from)), node);
        });
    }

    /// The element type of the list type `ty`, or `None` after reporting a type that is not a
    /// list.
    pub(crate) fn list_element(&mut self, ty: Type<'db>, node: NodeId) -> Option<Type<'db>> {
        let &TypeKind::List(element) = ty.kind(self.db) else {
            self.unsupported(node, "this list");
            return None;
        };
        Some(element)
    }

    /// `xs.copy()` into `place`: a new list holding a copy of each element, in order, appended
    /// one at a time. Elements of a copy type are copied as they are and `str`s with `copy()`;
    /// a list of any other element type is reported as not built yet.
    ///
    /// Cost: one pass over the elements, and the appends' amortized growth of the new buffer.
    pub(crate) fn list_copy(&mut self, place: &Place, list: NodeId, node: NodeId) {
        let ty = self.ty(list);
        if !self.copyable_elements(ty, "copy", node) {
            return;
        }
        let source = self.borrow(list);
        let length = self.length(&source, ty, 0, node);
        self.assign(place.clone(), Rvalue::Aggregate(ty, Vec::new()), node);
        self.loop_nest(&[length], &mut Vec::new(), node, &mut |builder, indices| {
            let from = source.clone().project(Projection::Index(indices.to_vec()));
            builder.append_copy(place, from, ty, node);
        });
    }

    /// `sorted(list)` or `sorted(list, reverse=flag)` into `place` (roadmap 1.13.13): a copy of
    /// the list, as `copy()` makes, sorted in place by [`Builtin::Sort`], which is stable and
    /// puts every float NaN last (Collins, 2026-11-10). The flag is a `bool`, `False` when left
    /// out.
    ///
    /// Cost: the copy, then O(n log n) comparisons.
    pub(crate) fn sorted(
        &mut self,
        place: &Place,
        list: NodeId,
        reverse: Option<NodeId>,
        node: NodeId,
    ) {
        let ty = self.ty(list);
        let TypeKind::List(element) = ty.kind(self.db) else {
            return self.unsupported(node, "`sorted` of anything but a list");
        };
        let orderable = matches!(
            element.kind(self.db),
            TypeKind::Str | TypeKind::Primitive(_)
        ) && !self.is_complex(*element);
        if !orderable {
            return self.unsupported(node, "`sorted` of a list of these elements");
        }
        let names: Vec<NodeId> = self
            .tree
            .child(node, Role::Arguments)
            .into_iter()
            .flat_map(|list| self.tree.children_with(list, Role::Item))
            .filter(|&item| self.tree.kind(item) == SyntaxKind::Keyword)
            .filter_map(|item| self.tree.child(item, Role::Name))
            .collect();
        if names.iter().any(|&name| self.text_of(name) != "reverse") {
            return self.unsupported(node, "`sorted` with `key=`");
        }
        self.list_copy(place, list, node);
        let boolean = self.boolean_type();
        let flag = match reverse {
            Some(reverse) => self.operand_as(reverse, boolean),
            None => Operand::Constant(Constant {
                ty: boolean,
                value: ConstantValue::Bool(false),
            }),
        };
        let nothing = self.temporary(Type::new(self.db, TypeKind::None), node);
        self.then(node, |next| TerminatorKind::Call {
            callee: Callee::Builtin(Builtin::Sort),
            arguments: vec![Argument::InOut(place.clone()), Argument::Owned(flag)],
            destination: Place::from(nothing),
            target: next,
            error: None,
        });
    }

    /// Whether `xs[i:j:k]` names a slice of a list: a subscript of a list by one slice.
    pub(crate) fn is_list_slice(&self, node: NodeId) -> bool {
        self.tree.kind(node) == SyntaxKind::Subscript
            && self
                .tree
                .child(node, Role::Index)
                .is_some_and(|index| self.tree.kind(index) == SyntaxKind::Slice)
            && self
                .tree
                .child(node, Role::Value)
                .is_some_and(|value| matches!(self.ty(value).kind(self.db), TypeKind::List(_)))
    }

    /// The list slice `node`, `xs[i:j:k]`, into `place`: a new list holding a copy of each
    /// element the range takes, in order, as Python's list slice is a new list. The range
    /// follows the rules of an array slice: the defaults, a negative literal counted from the
    /// end, a negative step walking backwards, and a panic for a bound outside the list.
    ///
    /// Cost: one pass over the elements taken, with the appends that grow the new list.
    pub(crate) fn list_slice(&mut self, place: &Place, node: NodeId) {
        let (Some(list), Some(slice)) = (
            self.tree.child(node, Role::Value),
            self.tree.child(node, Role::Index),
        ) else {
            return;
        };
        let ty = self.ty(list);
        if !self.copyable_elements(ty, "a slice", node) {
            return;
        }
        let source = self.borrow(list);
        let SliceIndex::Range {
            start,
            length,
            step,
        } = self.slice_range(&source, ty, 0, slice)
        else {
            return;
        };
        self.assign(place.clone(), Rvalue::Aggregate(ty, Vec::new()), node);
        let int = self.int_type();
        let copy = |local: Local| Operand::Copy(Place::from(local));
        self.loop_nest(
            &[copy(length)],
            &mut Vec::new(),
            node,
            &mut |builder, indices| {
                // The element at start + i * step.
                let index = builder.temporary(int, node);
                let offset = Rvalue::Binary(BinaryOperator::Multiply, copy(indices[0]), copy(step));
                builder.assign(Place::from(index), offset, node);
                let at = Rvalue::Binary(BinaryOperator::Add, copy(index), copy(start));
                builder.assign(Place::from(index), at, node);
                let from = source.clone().project(Projection::Index(vec![index]));
                builder.append_copy(place, from, ty, node);
            },
        );
    }

    /// `xs[i:j] = values`, the assignment into the list slice `target`: the elements in the
    /// range are replaced by copies of those of `values`, a list of the same type, which may be
    /// longer or shorter, so the list grows or shrinks as Python's does. The range follows the
    /// rules of a list slice read, [`Self::list_slice`]; an empty one, such as `xs[2:2]`,
    /// inserts at its start. A slice with a step, `xs[::2] = values`, is not lowered yet.
    ///
    /// The new contents are built in a new list, from the elements before the range, the
    /// values, and the elements after it, which then replaces the old one, dropping it. Reading
    /// everything before writing anything makes `xs[1:2] = xs` correct.
    ///
    /// Cost: one pass over the list and the values, with the appends that grow the new list.
    pub(crate) fn list_slice_store(&mut self, target: NodeId, values: NodeId) {
        let (Some(list), Some(slice)) = (
            self.tree.child(target, Role::Value),
            self.tree.child(target, Role::Index),
        ) else {
            return;
        };
        if self.tree.child(slice, Role::Step).is_some() {
            return self.unsupported(target, "assigning into a list slice with a step");
        }
        let ty = self.ty(list);
        let Some(element) = self.list_element(ty, target) else {
            return;
        };
        if !self.is_copy(element) && !matches!(element.kind(self.db), TypeKind::Str) {
            let element = element.display(self.db);
            let what = format!("assigning into a slice of a list of `{element}`");
            return self.unsupported(target, &what);
        }
        let source = self.borrow(values);
        let Some(destination) = self.changed_place(list) else {
            return;
        };
        let SliceIndex::Range { start, length, .. } = self.slice_range(&destination, ty, 0, slice)
        else {
            return;
        };
        let int = self.int_type();
        let copy = |local: Local| Operand::Copy(Place::from(local));
        // The first index after the range, and how many elements follow it.
        let (after, rest) = (self.temporary(int, target), self.temporary(int, target));
        let end = Rvalue::Binary(BinaryOperator::Add, copy(start), copy(length));
        self.assign(Place::from(after), end, target);
        let total = self.length(&destination, ty, 0, target);
        let remaining = Rvalue::Binary(BinaryOperator::Subtract, total, copy(after));
        self.assign(Place::from(rest), remaining, target);
        let count = self.length(&source, ty, 0, target);
        let result = self.temporary(ty, target);
        self.assign(
            Place::from(result),
            Rvalue::Aggregate(ty, Vec::new()),
            target,
        );
        let parts = [
            (destination.clone(), None, copy(start)),
            (source, None, count),
            (destination.clone(), Some(after), copy(rest)),
        ];
        for (from, offset, count) in parts {
            self.loop_nest(
                &[count],
                &mut Vec::new(),
                target,
                &mut |builder, indices| {
                    let index = match offset {
                        Some(offset) => {
                            let index = builder.temporary(int, target);
                            let at =
                                Rvalue::Binary(BinaryOperator::Add, copy(indices[0]), copy(offset));
                            builder.assign(Place::from(index), at, target);
                            index
                        }
                        None => indices[0],
                    };
                    let element = from.clone().project(Projection::Index(vec![index]));
                    builder.append_copy(&Place::from(result), element, ty, target);
                },
            );
        }
        self.replace(destination, Operand::Move(Place::from(result)), target);
    }

    /// Whether the elements of the list type `ty` can be copied into a new list: copy types and
    /// `str`s. Reports `operation`, such as "copy", as not lowered yet for other elements.
    fn copyable_elements(&mut self, ty: Type<'db>, operation: &str, node: NodeId) -> bool {
        let Some(element) = self.list_element(ty, node) else {
            return false;
        };
        let is_text = matches!(element.kind(self.db), TypeKind::Str);
        if !self.is_copy(element) && !is_text {
            let element = element.display(self.db);
            self.unsupported(node, &format!("`{operation}` of a list of `{element}`"));
            return false;
        }
        true
    }

    /// Appends a copy of the element at `from` to the list at `place`, of type `ty`: the value
    /// itself for a copy type, new text for a `str`.
    fn append_copy(&mut self, place: &Place, from: Place, ty: Type<'db>, node: NodeId) {
        let Some(element) = self.list_element(ty, node) else {
            return;
        };
        let value = if matches!(element.kind(self.db), TypeKind::Str) {
            let copy = self.temporary(element, node);
            let piece = Piece::Value(from, FormatSpec::default());
            self.assign(Place::from(copy), Rvalue::Format(vec![piece]), node);
            Operand::Move(Place::from(copy))
        } else {
            Operand::Copy(from)
        };
        let nothing = self.temporary(Type::new(self.db, TypeKind::None), node);
        self.then(node, |next| TerminatorKind::Call {
            callee: Callee::Builtin(Builtin::Append),
            arguments: vec![Argument::InOut(place.clone()), Argument::Owned(value)],
            destination: Place::from(nothing),
            target: next,
            error: None,
        });
    }

    /// `value in xs` or `value not in xs` on a list, into the `bool` at `place`: compares
    /// `value` with each element in order with `==` and stops at the first that is equal, as
    /// Python does. Elements may be numbers, `bool`s, enums or `str`s; `value` converts to the
    /// element type first, so `2 in weights` finds `2.0`.
    ///
    /// Cost: up to one pass over the elements.
    pub(crate) fn list_contains(
        &mut self,
        place: Place,
        operator: Operator,
        (value, list): (NodeId, NodeId),
        node: NodeId,
    ) {
        let ty = self.ty(list);
        let Some(element) = self.list_element(ty, node) else {
            return;
        };
        let is_text = matches!(element.kind(self.db), TypeKind::Str);
        let comparable = is_text
            || matches!(
                element.kind(self.db),
                TypeKind::Primitive(_) | TypeKind::Enum(_)
            );
        if !comparable {
            let element = element.display(self.db);
            return self.unsupported(node, &format!("`in` on a list of `{element}`"));
        }
        let wanted = if is_text {
            Operand::Copy(self.borrow(value))
        } else {
            let wanted = self.temporary(element, value);
            let operand = self.operand_as(value, element);
            self.assign(Place::from(wanted), Rvalue::Use(operand), value);
            Operand::Copy(Place::from(wanted))
        };
        let source = self.borrow(list);
        let int = self.int_type();
        let boolean = self.boolean_type();
        let length = self.length(&source, ty, 0, node);
        let (index, found) = (self.temporary(int, node), self.temporary(boolean, node));
        self.assign(
            Place::from(index),
            constant(Constant::integer(int, 0)),
            node,
        );
        self.assign(
            Place::from(found),
            constant(Constant {
                ty: boolean,
                value: ConstantValue::Bool(false),
            }),
            node,
        );
        let (test, body, step, done) = (
            self.new_block(),
            self.new_block(),
            self.new_block(),
            self.new_block(),
        );
        self.terminate(TerminatorKind::Goto(test), node);
        self.start(test);
        let more = self.temporary(boolean, node);
        let counter = Operand::Copy(Place::from(index));
        let below = Rvalue::Binary(BinaryOperator::Less, counter.clone(), length);
        self.assign(Place::from(more), below, node);
        self.branch(Operand::Copy(Place::from(more)), (body, done), node);
        self.start(body);
        let at = source.project(Projection::Index(vec![index]));
        let Operand::Copy(wanted) = wanted else {
            return;
        };
        if is_text {
            let arguments = vec![Argument::Borrow(at), Argument::Borrow(wanted)];
            self.text_equal(Place::from(found), arguments, node);
        } else {
            let equal = Rvalue::Binary(
                BinaryOperator::Equal,
                Operand::Copy(at),
                Operand::Copy(wanted),
            );
            self.assign(Place::from(found), equal, node);
        }
        self.branch(Operand::Copy(Place::from(found)), (done, step), node);
        self.start(step);
        let one = Operand::Constant(Constant::integer(int, 1));
        let next = Rvalue::Binary(BinaryOperator::Add, counter, one);
        self.assign(Place::from(index), next, node);
        self.terminate(TerminatorKind::Goto(test), node);
        self.start(done);
        let found = Operand::Copy(Place::from(found));
        let rvalue = if operator == Operator::NotIn {
            Rvalue::Unary(UnaryOperator::Not, found)
        } else {
            Rvalue::Use(found)
        };
        self.assign(place, rvalue, node);
    }
}

/// The rvalue that is the constant `constant`.
fn constant(constant: Constant<'_>) -> Rvalue<'_> {
    Rvalue::Use(Operand::Constant(constant))
}
