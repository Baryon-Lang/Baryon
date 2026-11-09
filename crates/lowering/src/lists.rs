//! Lists (language reference, "Lists"): a display `[a, b]` builds one, `xs.append(value)`
//! grows one, `len(xs)` and `xs[i]` read one, and `Array(xs)` copies one into a new array.
//!
//! The elements can be of any type. A list of `str`s, arrays, structs or other lists owns its
//! elements: a display or `append` moves each value in, writing `xs[i] = v` drops the element
//! it replaces, and dropping the list drops every element. An element is read in place, as a
//! borrow, since the checker rejects moving one out.

use crate::builder::Builder;
use mir::{Argument, Builtin, Callee, Operand, Place, Projection, Rvalue, TerminatorKind};
use syntax::{NodeId, Role};
use types::{Type, TypeKind};

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
        let Some(list) = self.place(list) else {
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
    fn list_element(&mut self, ty: Type<'db>, node: NodeId) -> Option<Type<'db>> {
        let &TypeKind::List(element) = ty.kind(self.db) else {
            self.unsupported(node, "this list");
            return None;
        };
        Some(element)
    }
}
