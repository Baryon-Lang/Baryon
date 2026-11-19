//! Sums of a whole-array expression that lowering used to write over the stored expression
//! (decision 0041, amended): an integer or `bool` sum, `(a * b + r).sum()`, and a sum along an
//! axis, `((u - v) * (u - v)).sum(axis=0)`.
//!
//! Each is written as the loops lowering wrote over the stored temporary, in the same order,
//! with the expression's value computed in place of each element read:
//!
//! - An integer sum keeps one running total from 0 and adds each value in row-major order with
//!   a checked `+` (decision 0030), so an overflow panics at the same element, reported at the
//!   call, as before. A `bool` adds as an `int`.
//! - A sum along the last axis of floats writes each row into a buffer as long as a row and
//!   sums the buffer with [`Builtin::Sum`], so each row has its pairwise bits.
//! - Every other sum along an axis sets the result to 0, then adds each value into its place in
//!   row-major order, checked for integers.
//!
//! Cost: one pass over the operands. The only memory is the result, and one row for a float sum
//! along the last axis, where storing the expression costs a whole array.

use super::Expander;
use super::broadcast::Broadcast;
use super::summed::Summed;
use crate::leaves::{ArrayLeaf, fused};
use mir::{
    Argument, BinaryOperator, Builtin, Constant, Elementwise, Local, Operand, Place, Projection,
    Rvalue, StatementKind, TerminatorKind,
};
use types::{Primitive, Type, TypeKind};

impl<'db> Expander<'_, '_, 'db> {
    /// Writes the integer or `bool` sum of `tree`, whose array leaves are `leaves`, into the
    /// destination of the call `summed` found.
    pub(super) fn sum_exact(
        mut self,
        tree: &Elementwise<'db>,
        leaves: &[ArrayLeaf<'db>],
        summed: &Summed<'db>,
    ) {
        let (broadcast, lengths) = self.sum_lengths(tree);
        let counted = self.counted(summed.element);
        let total = self.local(counted);
        let zero = Operand::Constant(Constant::integer(counted, 0));
        self.assign(Place::from(total), Rvalue::Use(zero));
        self.versioned(&broadcast, |this, scaled| {
            this.loop_nest(&lengths, |this, indices| {
                let (value, ty) = this.element(tree, leaves, indices, (&broadcast, scaled));
                let value = this.convert(value, ty, counted);
                this.add_into(Place::from(total), value, counted, summed);
            });
        });
        let total = Operand::Copy(Place::from(total));
        self.assign(summed.destination.clone(), Rvalue::Use(total));
        self.end_sum(leaves, summed);
    }

    /// Writes the sum of `tree` along the call's axis, whose array leaves are `leaves`, into
    /// a new array that the destination of the call `summed` found gets.
    pub(super) fn sum_along(
        mut self,
        tree: &Elementwise<'db>,
        leaves: &[ArrayLeaf<'db>],
        summed: &Summed<'db>,
    ) {
        let (broadcast, lengths) = self.sum_lengths(tree);
        let axis = summed.axis.unwrap_or(0) as usize;
        let mut kept = lengths.clone();
        let along = kept.remove(axis);
        let counted = self.counted(summed.element);
        let result_type = Type::new(
            self.db,
            TypeKind::Array {
                element: counted,
                rank: u32::try_from(kept.len()).unwrap_or(1),
            },
        );
        let result = self.local(result_type);
        self.assign(
            Place::from(result),
            Rvalue::NewArray(result_type, kept.clone()),
        );
        let at =
            |indices: &[Local]| Place::from(result).project(Projection::Index(indices.to_vec()));
        let float = matches!(
            counted.kind(self.db),
            TypeKind::Primitive(Primitive::Float(_))
        );
        if float && axis == kept.len() {
            let row_type = Type::new(
                self.db,
                TypeKind::Array {
                    element: counted,
                    rank: 1,
                },
            );
            let row = self.local(row_type);
            self.assign(
                Place::from(row),
                Rvalue::NewArray(row_type, vec![along.clone()]),
            );
            let total = self.local(counted);
            self.versioned(&broadcast, |this, scaled| {
                this.loop_nest(&kept, |this, outer| {
                    this.loop_nest(std::slice::from_ref(&along), |this, column| {
                        let mut all = outer.to_vec();
                        all.push(column[0]);
                        let (value, ty) = this.element(tree, leaves, &all, (&broadcast, scaled));
                        let value = this.convert(value, ty, counted);
                        let slot = Place::from(row).project(Projection::Index(column.to_vec()));
                        this.assign(slot, Rvalue::Use(value));
                    });
                    let arguments = vec![Argument::Borrow(Place::from(row))];
                    this.call(Builtin::Sum, arguments, Place::from(total));
                    this.assign(at(outer), Rvalue::Use(Operand::Copy(Place::from(total))));
                });
            });
            self.push(StatementKind::Drop(Place::from(row)));
        } else {
            let zero = if float {
                Operand::Constant(Constant::float(counted, 0.0))
            } else {
                Operand::Constant(Constant::integer(counted, 0))
            };
            self.loop_nest(&kept, |this, outer| {
                this.assign(at(outer), Rvalue::Use(zero));
            });
            self.versioned(&broadcast, |this, scaled| {
                this.loop_nest(&lengths, |this, indices| {
                    let (value, ty) = this.element(tree, leaves, indices, (&broadcast, scaled));
                    let value = this.convert(value, ty, counted);
                    let mut outer = indices.to_vec();
                    outer.remove(axis);
                    this.add_into(at(&outer), value, counted, summed);
                });
            });
        }
        let moved = Operand::Move(Place::from(result));
        self.assign(summed.destination.clone(), Rvalue::Use(moved));
        self.end_sum(leaves, summed);
    }

    /// The broadcasting `tree` needs, with each fused subexpression's operands checked first,
    /// and the lengths of its result.
    fn sum_lengths(&mut self, tree: &Elementwise<'db>) -> (Broadcast<'db>, Vec<Operand<'db>>) {
        let mut broadcast = Broadcast::default();
        for (subexpression, span) in fused(tree) {
            self.fused_lengths(subexpression, span, &mut broadcast);
        }
        let lengths = self.broadcast(tree, None, &mut broadcast);
        (broadcast, lengths)
    }

    /// The type a sum of `element`s adds in: `i64` for `bool`, which counts the `True` ones as
    /// `NumPy` does, and `element` itself otherwise.
    fn counted(&self, element: Type<'db>) -> Type<'db> {
        if matches!(element.kind(self.db), TypeKind::Primitive(Primitive::Bool)) {
            self.i64
        } else {
            element
        }
    }

    /// Writes `place = place + value`, both of type `counted`: checked for overflow, and
    /// reported at the call, for integers, and rounded once for floats.
    fn add_into(
        &mut self,
        place: Place,
        value: Operand<'db>,
        counted: Type<'db>,
        summed: &Summed<'db>,
    ) {
        let terms = (Operand::Copy(place.clone()), value);
        if matches!(
            counted.kind(self.db),
            TypeKind::Primitive(Primitive::Float(_))
        ) {
            return self.assign(place, Rvalue::Binary(BinaryOperator::Add, terms.0, terms.1));
        }
        let outer = std::mem::replace(&mut self.span, summed.span);
        let sum = self.checked(BinaryOperator::Add, terms, counted);
        self.span = outer;
        self.assign(place, Rvalue::Use(sum));
    }

    /// Drops the leaves the expression moved and goes on to the call's target block.
    fn end_sum(mut self, leaves: &[ArrayLeaf<'db>], summed: &Summed<'db>) {
        self.drop_moved(leaves);
        self.finish(TerminatorKind::Goto(summed.target), summed.target);
    }
}
