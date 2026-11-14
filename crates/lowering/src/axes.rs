//! Reductions along one axis (roadmap 3.9): `a.sum(axis=k)`, `a.min(axis=k)` and
//! `a.max(axis=k)` on an `Array` or a `Fixed` array, as loop nests, with `NumPy`'s results.
//!
//! The result has the array's lengths without axis `k`, and each element reduces the line of
//! elements along `k` that passes through it. The order of the additions is `NumPy`'s, so a float
//! sum has the same bits as `numpy.sum(a, axis=k)` (Collins, 2026-11-11):
//!
//! - Along the last axis, each line is one row in memory, and each is added in the pairwise tree
//!   of `a.sum()` by a call of [`Builtin::Sum`] on a view of the row.
//! - Along any other axis, the result starts at 0 and the loop nest walks the array in row-major
//!   order (decision 0008), adding each element into its place in the result. That adds each line
//!   one element at a time from the first, as `NumPy` does, and reads memory in order.
//!
//! Integer sums use the second form for every axis, since integer addition is exact in any order;
//! each `+` is checked for overflow as the operator is (decision 0030). `min` and `max` start
//! each place at the line's first element and scan the rest in row-major order with the rule of
//! `a.min()`: a NaN anywhere in a line gives NaN. A `min` or `max` along an axis of length 0
//! panics, as `NumPy`'s does, even when the result would be empty. An array of rank 1 reduces to
//! one number, as `a.sum()` does.
//!
//! Cost: one pass over the array, plus one call per row for a float sum along the last axis.

use crate::builder::Builder;
use mir::{
    Argument, BinaryOperator, Builtin, Callee, Constant, Local, Operand, Place, Projection, Rvalue,
    SliceIndex, TerminatorKind,
};
use syntax::NodeId;
use types::TypeKind;

impl<'db> Builder<'db> {
    /// `a.sum(axis)`, `a.min(axis)` or `a.max(axis)`, the method `method` of `array` called with
    /// the axis argument `axis`, into `place`.
    pub(crate) fn axis_reduction(
        &mut self,
        place: Place,
        method: &str,
        (array, axis): (NodeId, NodeId),
        node: NodeId,
    ) {
        let ty = self.ty(array);
        let rank = self.rank(ty);
        let Some(axis) = self.axis_position(axis, rank) else {
            return;
        };
        if rank == 1 {
            return match method {
                "sum" => self.array_sum(place, array, node),
                _ => self.array_extreme(place, method == "min", array, node),
            };
        }
        let element = self.element(ty);
        if !matches!(element.kind(self.db), TypeKind::Primitive(_)) || self.is_complex(element) {
            return self.unsupported(
                node,
                &format!(
                    "`{method}` along an axis of an array whose elements are not real numbers"
                ),
            );
        }
        let source = self.borrow(array);
        let result_type = self.ty(node);
        self.reduce_along(place, method, (&source, ty, result_type), axis, node);
    }

    /// The dimension, from 0, that the constant axis argument `axis` names in an array of rank
    /// `rank`, where -1 is the last. `None`, after reporting it, when it is not a constant or
    /// names no dimension; the checker has reported both already.
    pub(crate) fn axis_position(&mut self, axis: NodeId, rank: u32) -> Option<u32> {
        let Some(given) = self.constant_integer(axis) else {
            self.unsupported(axis, "an axis that is not a literal or a `Final` constant");
            return None;
        };
        let position = if given < 0 {
            given + i128::from(rank)
        } else {
            given
        };
        let found = u32::try_from(position).ok().filter(|axis| *axis < rank);
        if found.is_none() {
            self.unsupported(axis, "an axis outside the array");
        }
        found
    }

    /// The reduction `method`, `"sum"`, `"min"` or `"max"`, along `axis` of the array at
    /// `source`, of type `ty` with rank 2 or more and real elements, into `place` as
    /// `result_type`, the array's type without that axis. `a.mean(axis=k)` and `a.var(axis=k)`
    /// sum through here too.
    pub(crate) fn reduce_along(
        &mut self,
        place: Place,
        method: &str,
        (source, ty, result_type): (&Place, types::Type<'db>, types::Type<'db>),
        axis: u32,
        node: NodeId,
    ) {
        let rank = self.rank(ty);
        let element = self.element(ty);
        let lengths: Vec<Operand<'db>> = (0..rank)
            .map(|dimension| self.length(source, ty, dimension, node))
            .collect();
        let mut kept = lengths.clone();
        let along = kept.remove(axis as usize);
        let result = self.temporary(result_type, node);
        let start = if let TypeKind::Array { .. } = result_type.kind(self.db) {
            Rvalue::NewArray(result_type, kept.clone())
        } else {
            let Some(zeros) = self.filled(result_type, 0, node) else {
                return;
            };
            Rvalue::Use(zeros)
        };
        self.assign(Place::from(result), start, node);
        let reduced = Reduced {
            source: source.clone(),
            result: Place::from(result),
            lengths: &lengths,
            kept: &kept,
            axis,
        };
        match method {
            "sum" if self.is_float(element) && axis == rank - 1 => {
                self.sum_rows(&reduced, (along, element), node);
            }
            "sum" => self.sum_along(&reduced, element, node),
            _ => {
                if matches!(ty.kind(self.db), TypeKind::Array { .. }) {
                    self.not_empty(std::slice::from_ref(&along), false, node);
                }
                self.extreme_along(&reduced, (element, method == "min"), node);
            }
        }
        let moved = self.take(Place::from(result), result_type);
        self.assign(place, Rvalue::Use(moved), node);
    }

    /// Writes into each place of the result the pairwise sum of its row, the elements along the
    /// last axis, whose length is `along`, of type `element`.
    fn sum_rows(
        &mut self,
        reduced: &Reduced<'_, 'db>,
        (along, element): (Operand<'db>, types::Type<'db>),
        node: NodeId,
    ) {
        let int = self.int_type();
        let range = [0, 1].map(|value| {
            let local = self.temporary(int, node);
            let constant = Operand::Constant(Constant::integer(int, value));
            self.assign(Place::from(local), Rvalue::Use(constant), node);
            local
        });
        let length = self.temporary(int, node);
        self.assign(Place::from(length), Rvalue::Use(along), node);
        let total = self.temporary(element, node);
        self.loop_nest(
            reduced.kept,
            &mut Vec::new(),
            node,
            &mut |builder, outer| {
                let mut entries: Vec<SliceIndex> =
                    outer.iter().copied().map(SliceIndex::Index).collect();
                entries.push(SliceIndex::Range {
                    start: range[0],
                    length,
                    step: range[1],
                });
                let row = reduced.source.clone().project(Projection::Slice(entries));
                let next = builder.new_block();
                builder.terminate(
                    TerminatorKind::Call {
                        callee: Callee::Builtin(Builtin::Sum),
                        arguments: vec![Argument::Borrow(row)],
                        destination: Place::from(total),
                        target: next,
                        error: None,
                    },
                    node,
                );
                builder.start(next);
                let at = reduced.at(outer);
                builder.assign(at, Rvalue::Use(Operand::Copy(Place::from(total))), node);
            },
        );
    }

    /// Sets each place of the result to 0, then adds every element of the array into its place,
    /// in row-major order; a `bool` element adds as an `int`, counting the `True` ones.
    fn sum_along(&mut self, reduced: &Reduced<'_, 'db>, element: types::Type<'db>, node: NodeId) {
        let counted = self.summand(element);
        let zero = Operand::Constant(self.number_constant(counted, 0));
        self.loop_nest(
            reduced.kept,
            &mut Vec::new(),
            node,
            &mut |builder, outer| {
                builder.assign(reduced.at(outer), Rvalue::Use(zero.clone()), node);
            },
        );
        self.loop_nest(
            reduced.lengths,
            &mut Vec::new(),
            node,
            &mut |builder, indices| {
                let at = reduced.at(&reduced.without_axis(indices));
                let value = reduced
                    .source
                    .clone()
                    .project(Projection::Index(indices.to_vec()));
                let value = builder.as_summand(value, element, node);
                let terms = (Operand::Copy(at.clone()), value);
                builder.arithmetic(at, BinaryOperator::Add, terms, counted, node);
            },
        );
    }

    /// Sets each place of the result to the first element of its line, then keeps the
    /// `smallest` or largest of every element in row-major order.
    fn extreme_along(
        &mut self,
        reduced: &Reduced<'_, 'db>,
        (element, smallest): (types::Type<'db>, bool),
        node: NodeId,
    ) {
        let first = self.zero_index(node);
        self.loop_nest(
            reduced.kept,
            &mut Vec::new(),
            node,
            &mut |builder, outer| {
                let mut indices = outer.to_vec();
                indices.insert(reduced.axis as usize, first);
                let value = reduced.source.clone().project(Projection::Index(indices));
                builder.assign(reduced.at(outer), Rvalue::Use(Operand::Copy(value)), node);
            },
        );
        let test = if smallest {
            BinaryOperator::Less
        } else {
            BinaryOperator::Greater
        };
        let float = self.is_float(element);
        let scratch = (
            self.temporary(element, node),
            self.temporary(self.boolean_type(), node),
            self.temporary(self.boolean_type(), node),
        );
        self.loop_nest(
            reduced.lengths,
            &mut Vec::new(),
            node,
            &mut |builder, indices| {
                let best = reduced.at(&reduced.without_axis(indices));
                let value = reduced
                    .source
                    .clone()
                    .project(Projection::Index(indices.to_vec()));
                builder.keep_extreme(best, value, (test, float), scratch, node);
            },
        );
    }
}

/// One reduction along an axis, as the loops that build it see it.
struct Reduced<'a, 'db> {
    /// The array reduced, borrowed.
    source: Place,
    /// The new array, or `Fixed` value, the reductions are written into.
    result: Place,
    /// The array's lengths, one per dimension.
    lengths: &'a [Operand<'db>],
    /// The result's lengths: the array's without the axis.
    kept: &'a [Operand<'db>],
    /// The dimension reduced, from 0.
    axis: u32,
}

impl Reduced<'_, '_> {
    /// The place in the result at `indices`, one per dimension of the result.
    fn at(&self, indices: &[Local]) -> Place {
        self.result
            .clone()
            .project(Projection::Index(indices.to_vec()))
    }

    /// `indices`, one per dimension of the array, without the one along the axis.
    fn without_axis(&self, indices: &[Local]) -> Vec<Local> {
        let mut kept = indices.to_vec();
        kept.remove(self.axis as usize);
        kept
    }
}
