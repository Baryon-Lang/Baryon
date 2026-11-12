//! Choosing elements of an array with another array (language reference, "Arrays", "Choosing
//! elements"; roadmap 3.13.1): `a[mask]` keeps the elements where a `bool` array is `True`, and
//! `a[positions]` takes the elements at the positions an `i64` array lists. Each is a new
//! array, built by plain loops: a mask is counted first, so the result is allocated once.

use crate::builder::Builder;
use mir::{BinaryOperator, Check, Constant, Local, Operand, Place, Projection, Rvalue};
use mir::{StatementKind, TerminatorKind};
use syntax::{NodeId, Role, SyntaxKind};

impl<'db> Builder<'db> {
    /// Whether the subscript `node` chooses elements with an array, `a[mask]` or
    /// `a[positions]`, which gives a new array rather than naming an element or a view.
    pub(crate) fn is_selection(&self, node: NodeId) -> bool {
        self.tree.kind(node) == SyntaxKind::Subscript
            && self
                .tree
                .child(node, Role::Index)
                .is_some_and(|index| self.is_array(self.ty(index)))
    }

    /// `a[mask]` or `a[positions]` into `place`: a new rank-1 array of the chosen elements.
    ///
    /// The result is built in a temporary and moved into `place`, so `a = a[mask]` reads `a`
    /// before it is replaced.
    pub(crate) fn selection(&mut self, place: Place, node: NodeId) {
        let (Some(value), Some(index)) = (
            self.tree.child(node, Role::Value),
            self.tree.child(node, Role::Index),
        ) else {
            return;
        };
        let (ty, by_type) = (self.ty(node), self.ty(index));
        let source = self.borrow(value);
        let by = self.borrow(index);
        let length = self.length(&source, self.ty(value), 0, node);
        let chosen = self.length(&by, by_type, 0, index);
        let result = self.temporary(ty, node);
        if self.element(by_type) == self.boolean_type() {
            self.same_length(0, length, chosen.clone(), index);
            self.masked(result, &source, &by, &chosen, node);
        } else {
            self.assign(
                Place::from(result),
                Rvalue::NewArray(ty, vec![chosen.clone()]),
                node,
            );
            self.loop_nest(&[chosen], &mut Vec::new(), node, &mut |builder, indices| {
                let position = builder.chosen_position(&by, indices, &length, index);
                let from = source.clone().project(Projection::Index(vec![position]));
                let to = Place::from(result).project(Projection::Index(indices.to_vec()));
                builder.assign(to, Rvalue::Use(Operand::Copy(from)), node);
            });
        }
        self.assign(place, Rvalue::Use(Operand::Move(Place::from(result))), node);
    }

    /// `a[mask] = value` or `a[positions] = value`: sets every chosen element of `a` to the one
    /// `value`, which is computed once, before anything is written.
    pub(crate) fn selection_store(&mut self, target: NodeId, value: NodeId) {
        let (Some(array), Some(index)) = (
            self.tree.child(target, Role::Value),
            self.tree.child(target, Role::Index),
        ) else {
            return;
        };
        let (array_type, by_type) = (self.ty(array), self.ty(index));
        let element = self.element(array_type);
        let stored = self.temporary(element, value);
        let operand = self.operand_as(value, element);
        self.assign(Place::from(stored), Rvalue::Use(operand), value);
        let by = self.borrow(index);
        let destination = self.borrow(array);
        let length = self.length(&destination, array_type, 0, target);
        let chosen = self.length(&by, by_type, 0, index);
        let masked = self.element(by_type) == self.boolean_type();
        if masked {
            self.same_length(0, length.clone(), chosen.clone(), index);
        }
        self.loop_nest(
            &[chosen],
            &mut Vec::new(),
            target,
            &mut |builder, indices| {
                let value = Rvalue::Use(Operand::Copy(Place::from(stored)));
                if masked {
                    builder.where_set(&by, indices, target, &mut |builder| {
                        let to = destination
                            .clone()
                            .project(Projection::Index(indices.to_vec()));
                        builder.assign(to, value.clone(), target);
                    });
                } else {
                    let position = builder.chosen_position(&by, indices, &length, index);
                    let to = destination
                        .clone()
                        .project(Projection::Index(vec![position]));
                    builder.assign(to, value, target);
                }
            },
        );
    }

    /// The position `by[indices]` names, as an `i64` local, checked to lie within `length` as an
    /// index written in the source is (decision 0010): a position below 0 or past the end
    /// panics.
    fn chosen_position(
        &mut self,
        by: &Place,
        indices: &[Local],
        length: &Operand<'db>,
        node: NodeId,
    ) -> Local {
        let (int, boolean) = (self.int_type(), self.boolean_type());
        let position = self.temporary(int, node);
        let entry = by.clone().project(Projection::Index(indices.to_vec()));
        self.assign(
            Place::from(position),
            Rvalue::Use(Operand::Copy(entry)),
            node,
        );
        let copy = Operand::Copy(Place::from(position));
        let zero = Operand::Constant(Constant::integer(int, 0));
        let (above, below, inside) = (
            self.temporary(boolean, node),
            self.temporary(boolean, node),
            self.temporary(boolean, node),
        );
        self.assign(
            Place::from(above),
            Rvalue::Binary(BinaryOperator::GreaterEqual, copy.clone(), zero),
            node,
        );
        self.assign(
            Place::from(below),
            Rvalue::Binary(BinaryOperator::Less, copy.clone(), length.clone()),
            node,
        );
        self.assign(
            Place::from(inside),
            Rvalue::Binary(
                BinaryOperator::BitAnd,
                Operand::Copy(Place::from(above)),
                Operand::Copy(Place::from(below)),
            ),
            node,
        );
        let length = length.clone();
        self.then(node, |target| TerminatorKind::Assert {
            condition: Operand::Copy(Place::from(inside)),
            check: Check::IndexInBounds {
                dimension: 0,
                index: copy,
                length,
            },
            target,
        });
        position
    }

    /// The elements of `source` where `mask` is `True`, into `result`: one loop counts them,
    /// the array is allocated with that length, and a second loop copies them in order.
    /// `length` is the length of both, which the caller has checked to be equal.
    fn masked(
        &mut self,
        result: Local,
        source: &Place,
        mask: &Place,
        length: &Operand<'db>,
        node: NodeId,
    ) {
        let int = self.int_type();
        let ty = self.local_type(result);
        let lengths = [length.clone()];
        let zero = || Rvalue::Use(Operand::Constant(Constant::integer(int, 0)));
        let count = self.temporary(int, node);
        self.assign(Place::from(count), zero(), node);
        self.loop_nest(&lengths, &mut Vec::new(), node, &mut |builder, indices| {
            builder.where_set(mask, indices, node, &mut |builder| {
                builder.increment(count, node);
            });
        });
        self.assign(
            Place::from(result),
            Rvalue::NewArray(ty, vec![Operand::Copy(Place::from(count))]),
            node,
        );
        let next = self.temporary(int, node);
        self.assign(Place::from(next), zero(), node);
        self.loop_nest(&lengths, &mut Vec::new(), node, &mut |builder, indices| {
            builder.where_set(mask, indices, node, &mut |builder| {
                let from = source.clone().project(Projection::Index(indices.to_vec()));
                let to = Place::from(result).project(Projection::Index(vec![next]));
                builder.assign(to, Rvalue::Use(Operand::Copy(from)), node);
                builder.increment(next, node);
            });
        });
    }

    /// Runs `then` when the `bool` at `mask[indices]` is `True`, and carries on after it either
    /// way.
    fn where_set(
        &mut self,
        mask: &Place,
        indices: &[Local],
        node: NodeId,
        then: &mut dyn FnMut(&mut Self),
    ) {
        let (set, after) = (self.new_block(), self.new_block());
        let flag = mask.clone().project(Projection::Index(indices.to_vec()));
        self.terminate(
            TerminatorKind::Branch {
                condition: Operand::Copy(flag),
                then: set,
                otherwise: after,
            },
            node,
        );
        self.start(set);
        then(self);
        self.terminate(TerminatorKind::Goto(after), node);
        self.start(after);
    }

    /// Adds 1 to the `i64` counter `local`. It counts elements of an array that exists, so it
    /// cannot overflow.
    fn increment(&mut self, local: Local, node: NodeId) {
        let one = Operand::Constant(Constant::integer(self.int_type(), 1));
        self.push(
            StatementKind::Assign(
                Place::from(local),
                Rvalue::Binary(BinaryOperator::Add, Operand::Copy(Place::from(local)), one),
            ),
            node,
        );
    }
}
