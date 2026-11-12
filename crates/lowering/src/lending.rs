//! Lending: a conditional expression in a place that only borrows its value, such as the
//! argument of `size(name[0:1] if len(name) > 9 else name)` to a read-only parameter.
//!
//! A borrow of a variable or parameter passes its place and copies nothing (decision 0006).
//! A borrow of a computed value, such as `name[0:1]`, passes a temporary that the statement
//! drops when it ends. A conditional expression can be either, decided when the program runs,
//! and the IR has no reference to stand for "whichever branch was taken" (decision 0018). So
//! the borrowed value goes through one temporary, the *view*, on every branch:
//!
//! - a branch that computes a value writes it into the view, which owns it;
//! - a branch that names a place moves the place's value into the view and records, in an
//!   `i64` *source* local, which branch it was.
//!
//! When the statement ends, the source decides what happens to the view. A value lent by a
//! variable, an `Owned` parameter or a temporary moves back to its place. A value lent by a
//! borrowed parameter belongs to the caller and stays where it is: the view moves into a
//! *scratch* temporary that nothing drops, which tells drop elaboration the view is empty. Then
//! the view is dropped, which drop elaboration keeps only on the paths where a branch computed
//! it. Before, the view was dropped on every path, freeing the variable or the caller's value
//! it had borrowed: a read after the statement used freed memory, and the owner's own drop
//! freed it a second time.
//!
//! No value is copied: lending costs two moves of the value's handle (a pointer and a length
//! for a `str`, `list` or `Array`) and one integer switch.

use crate::builder::Builder;
use mir::{Constant, Local, LocalKind, Operand, Place, Rvalue, StatementKind, TerminatorKind};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Integer, Mode, Primitive, Type, TypeKind};

/// A temporary that a statement made and ends when the statement ends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Temporary {
    /// A temporary that owns its value; ending the statement drops it.
    Owned(Local),
    /// The view of a lent conditional expression; ending the statement gives each lent value
    /// back to its place and drops only a value a branch computed.
    Lent(Lent),
}

/// The view of a conditional expression that is borrowed, and the places it may hold the value
/// of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Lent {
    /// The temporary passed where the value is borrowed.
    view: Local,
    /// The `i64` local that says which branch filled the view: 0 when a branch computed its
    /// value, or the position in `places` plus one.
    source: Local,
    /// The place each lending branch moved its value from, in branch order.
    places: Vec<Place>,
}

impl<'db> Builder<'db> {
    /// Whether the borrow of `node` must lend: a conditional expression whose value owns
    /// memory. A conditional of copy values, such as `f64`, is computed into a temporary that
    /// needs no drop.
    pub(crate) fn lends(&self, node: NodeId) -> bool {
        self.tree.kind(node) == SyntaxKind::Conditional && self.needs_drop(self.ty(node))
    }

    /// The place a borrow of the conditional expression `node` passes: its view, filled on each
    /// branch and ended with the statement that made it (the module documentation).
    pub(crate) fn lend(&mut self, node: NodeId) -> Place {
        let ty = self.ty(node);
        let view = self.temporary(ty, node);
        let source = self.temporary(self.i64(), node);
        self.set_source(source, 0, node);
        let mut places = Vec::new();
        self.lend_branches(node, (view, source), &mut places);
        self.end_with_statement(Temporary::Lent(Lent {
            view,
            source,
            places,
        }));
        Place::from(view)
    }

    /// Fills `view` with the value of `node`, a branch of a lent conditional or the
    /// conditional itself, recording in `source` the place a lending branch moved from.
    fn lend_branches(
        &mut self,
        node: NodeId,
        (view, source): (Local, Local),
        places: &mut Vec<Place>,
    ) {
        let ty = self.local_type(view);
        if self.tree.kind(node) == SyntaxKind::Conditional {
            let (Some(value), Some(condition), Some(other)) = (
                self.tree.child(node, Role::Value),
                self.tree.child(node, Role::Condition),
                self.tree.child(node, Role::Else),
            ) else {
                return;
            };
            let condition = self.operand(condition);
            let (then, otherwise) = (self.new_block(), self.new_block());
            self.terminate(
                TerminatorKind::Branch {
                    condition,
                    then,
                    otherwise,
                },
                node,
            );
            let mut join = None;
            self.start(then);
            self.lend_branches(value, (view, source), places);
            self.jump(&mut join, node);
            self.start(otherwise);
            self.lend_branches(other, (view, source), places);
            self.jump(&mut join, node);
            return self.join(join);
        }
        // The checker gives both branches the conditional's type, so a lent value needs no
        // conversion.
        if let Some(place) = self.lent_place(node) {
            places.push(place.clone());
            let position = i128::try_from(places.len()).unwrap_or(i128::MAX);
            self.assign(Place::from(view), Rvalue::Use(Operand::Move(place)), node);
            return self.set_source(source, position, node);
        }
        self.write(Place::from(view), ty, node);
    }

    /// The place the branch `node` names, when a borrow of it would pass that place: a
    /// variable, a parameter or a field of one, as [`borrow`](Self::borrow) decides.
    fn lent_place(&mut self, node: NodeId) -> Option<Place> {
        if self.is_literal(node)
            || self.named_constant(node).is_some()
            || self.is_array_attribute(node)
            || self.is_list_slice(node)
            || self.is_text_subscript(node)
            || self.is_computed(node)
            || !matches!(
                self.tree.kind(node),
                SyntaxKind::Name | SyntaxKind::Attribute | SyntaxKind::Subscript
            )
        {
            return None;
        }
        self.place(node).filter(|place| !place.is_view())
    }

    /// Ends `temporary` for `node`: drops an owned one, and gives a lent view's value back.
    pub(crate) fn end_temporary(&mut self, temporary: &Temporary, node: NodeId) {
        match temporary {
            Temporary::Owned(local) => self.push(StatementKind::Drop(Place::from(*local)), node),
            Temporary::Lent(lent) => self.end_lent(lent, node),
        }
    }

    /// Ends the view `lent` when its statement ends, for `node`: gives each lent value back to
    /// its place, then drops the view, which holds a value only when a branch computed it.
    fn end_lent(&mut self, lent: &Lent, node: NodeId) {
        if !lent.places.is_empty() {
            let blocks: Vec<_> = lent.places.iter().map(|_| self.new_block()).collect();
            let computed = self.new_block();
            self.terminate(
                TerminatorKind::Switch {
                    value: Operand::Copy(Place::from(lent.source)),
                    cases: (1..).zip(blocks.iter().copied()).collect(),
                    otherwise: computed,
                },
                node,
            );
            let mut join = Some(computed);
            for (place, block) in lent.places.iter().zip(blocks) {
                self.start(block);
                let back = if self.belongs_to_caller(place) {
                    Place::from(self.temporary(self.local_type(lent.view), node))
                } else {
                    place.clone()
                };
                let value = Rvalue::Use(Operand::Move(Place::from(lent.view)));
                self.assign(back, value, node);
                self.jump(&mut join, node);
            }
            self.start(computed);
        }
        self.push(StatementKind::Drop(Place::from(lent.view)), node);
    }

    /// Whether `place` is inside a parameter borrowed read-only or `InOut`, whose value belongs
    /// to the caller and is never written back from a lent view.
    fn belongs_to_caller(&self, place: &Place) -> bool {
        matches!(
            self.body.locals[place.local.index()].kind,
            LocalKind::Parameter {
                mode: Mode::Borrow | Mode::InOut,
                ..
            }
        )
    }

    /// Writes `position` into the `i64` local `source`.
    fn set_source(&mut self, source: Local, position: i128, node: NodeId) {
        let constant = Constant::integer(self.i64(), position);
        self.assign(
            Place::from(source),
            Rvalue::Use(Operand::Constant(constant)),
            node,
        );
    }

    /// The type `i64`.
    fn i64(&self) -> Type<'db> {
        Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::I64)),
        )
    }
}
