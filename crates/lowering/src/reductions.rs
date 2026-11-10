//! `sum(values)` and `sum(values, start)` of a list or a rank-1 array (roadmap 1.13.13), and
//! where `min` and `max` go: of one sequence, to the `methods` module's scan, shared with
//! `a.min()`; of two or more numbers, to the `calls` module's comparisons.
//!
//! A list sums as CPython 3.12 sums it, so a program ported from Python gets the same bits
//! (Collins, 2026-11-10):
//!
//! - Integers, with an integer start, add exactly from the first, each `+` checked for overflow
//!   as the operator is (decision 0030).
//! - Integers with a float start add as floats, one at a time, with no compensation, as
//!   CPython's float loop adds an `int`.
//! - Floats add with Neumaier's compensation: a second float, `c`, collects the rounding error
//!   of each `+`, and is added once at the end when it is not zero and is finite, so it cannot
//!   turn an infinite sum into a NaN. With an integer start, or none, the first element is
//!   added to the start without compensation, as CPython adds it before its float loop starts.
//!
//! An array sums as `a.sum()` does, in `NumPy`'s pairwise order, so array code matches `NumPy`.

use crate::builder::Builder;
use mir::{
    BinaryOperator, Constant, Local, MathFunction, Operand, Place, Projection, Rvalue,
    TerminatorKind,
};
use syntax::NodeId;
use types::{Type, TypeKind};

impl<'db> Builder<'db> {
    /// Whether the call `node`, `name(values, rest…)`, is a reduction: `sum` of a sequence
    /// with at most a start, or `min` or `max` of one sequence or of two or more numbers.
    pub(crate) fn reduces(
        &self,
        name: &str,
        (values, rest): (NodeId, &[NodeId]),
        node: NodeId,
    ) -> bool {
        let number = |ty: Type<'db>| matches!(ty.kind(self.db), TypeKind::Primitive(_));
        match name {
            "sum" => rest.len() <= 1,
            "min" | "max" if rest.is_empty() => !number(self.ty(values)),
            "min" | "max" => number(self.ty(node)),
            _ => false,
        }
    }

    /// `name(values, rest…)` into `place`, a call [`Self::reduces`] accepts: `sum` here, `min`
    /// and `max` of one sequence through the scan `a.min()` uses, and of numbers by comparing
    /// each in turn.
    pub(crate) fn reduce(
        &mut self,
        place: Place,
        name: &str,
        (values, rest): (NodeId, &[NodeId]),
        node: NodeId,
    ) {
        match name {
            "sum" => self.sum(place, (values, rest.first().copied()), node),
            _ if rest.is_empty() => self.array_extreme(place, name == "min", values, node),
            _ => self.min_max(place, name == "min", values, rest, node),
        }
    }

    /// `sum(sequence)` or `sum(sequence, start)` into `place` (the module documentation).
    ///
    /// Cost: one pass over the list; a float list does about six float operations per element.
    pub(crate) fn sum(
        &mut self,
        place: Place,
        (sequence, start): (NodeId, Option<NodeId>),
        node: NodeId,
    ) {
        let ty = self.ty(sequence);
        let element = match ty.kind(self.db) {
            TypeKind::List(element) => *element,
            _ if self.is_array(ty) && start.is_none() => {
                return self.array_sum(place, sequence, node);
            }
            _ if self.is_array(ty) => {
                return self.unsupported(node, "`sum` of an array with a start");
            }
            _ => return self.unsupported(node, "`sum` of this value"),
        };
        if !matches!(element.kind(self.db), TypeKind::Primitive(_)) || self.is_complex(element) {
            return self.unsupported(node, "`sum` of a list of complex numbers");
        }
        let result = self.ty(node);
        let list = self.borrow(sequence);
        let length = self.length(&list, ty, 0, node);
        let total = self.temporary(result, node);
        let initial = match start {
            Some(start) => self.operand_as(start, result),
            None => Operand::Constant(self.number_constant(result, 0)),
        };
        self.assign(Place::from(total), Rvalue::Use(initial), node);
        let read = |builder: &mut Self, index: Local| {
            let at = list.clone().project(Projection::Index(vec![index]));
            builder.convert(Operand::Copy(at), element, result, node)
        };
        if !self.is_float(element) {
            self.index_loop((0, length), node, &mut |builder, index| {
                let value = read(builder, index);
                let operands = (Operand::Copy(Place::from(total)), value);
                builder.arithmetic(
                    Place::from(total),
                    BinaryOperator::Add,
                    operands,
                    result,
                    node,
                );
            });
            return self.assign(place, Rvalue::Use(Operand::Copy(Place::from(total))), node);
        }
        // CPython adds the first float to an integer start before its compensated loop.
        let plain_first = start.is_none_or(|start| self.is_integer(self.ty(start)));
        let first = if plain_first {
            self.add_first(
                total,
                &length,
                &mut |builder| {
                    let index = builder.zero_index(node);
                    read(builder, index)
                },
                node,
            );
            1
        } else {
            0
        };
        let compensation = self.temporary(result, node);
        let zero = Operand::Constant(self.number_constant(result, 0));
        self.assign(Place::from(compensation), Rvalue::Use(zero), node);
        self.index_loop((first, length), node, &mut |builder, index| {
            let value = read(builder, index);
            builder.neumaier_step((total, compensation), value, result, node);
        });
        self.add_compensation((total, compensation), result, node);
        self.assign(place, Rvalue::Use(Operand::Copy(Place::from(total))), node);
    }

    /// `total += value()` when the list of `length` elements is not empty: the first element
    /// added to the start, plainly.
    fn add_first(
        &mut self,
        total: Local,
        length: &Operand<'db>,
        value: &mut dyn FnMut(&mut Self) -> Operand<'db>,
        node: NodeId,
    ) {
        let int = self.int_type();
        let some = self.temporary(self.boolean_type(), node);
        self.assign(
            Place::from(some),
            Rvalue::Binary(
                BinaryOperator::Greater,
                length.clone(),
                Operand::Constant(Constant::integer(int, 0)),
            ),
            node,
        );
        let (add, join) = (self.new_block(), self.new_block());
        self.branch(Operand::Copy(Place::from(some)), (add, join), node);
        self.start(add);
        let value = value(self);
        self.assign(
            Place::from(total),
            Rvalue::Binary(
                BinaryOperator::Add,
                Operand::Copy(Place::from(total)),
                value,
            ),
            node,
        );
        self.terminate(TerminatorKind::Goto(join), node);
        self.start(join);
    }

    /// One step of Neumaier's sum, of floats of type `ty`: `t = total + value`; the rounding
    /// error of that `+`, found from whichever of the two is larger in magnitude, is added to
    /// `compensation`; then `total = t`.
    fn neumaier_step(
        &mut self,
        (total, compensation): (Local, Local),
        value: Operand<'db>,
        ty: Type<'db>,
        node: NodeId,
    ) {
        let copy = |local: Local| Operand::Copy(Place::from(local));
        let x = self.temporary(ty, node);
        self.assign(Place::from(x), Rvalue::Use(value), node);
        let sum = self.temporary(ty, node);
        self.assign(
            Place::from(sum),
            Rvalue::Binary(BinaryOperator::Add, copy(total), copy(x)),
            node,
        );
        let (total_size, value_size) = (self.temporary(ty, node), self.temporary(ty, node));
        self.math_call(
            Place::from(total_size),
            MathFunction::Fabs,
            vec![copy(total)],
            node,
        );
        self.math_call(
            Place::from(value_size),
            MathFunction::Fabs,
            vec![copy(x)],
            node,
        );
        let larger = self.temporary(self.boolean_type(), node);
        self.assign(
            Place::from(larger),
            Rvalue::Binary(
                BinaryOperator::GreaterEqual,
                copy(total_size),
                copy(value_size),
            ),
            node,
        );
        let (total_larger, value_larger, join) =
            (self.new_block(), self.new_block(), self.new_block());
        self.branch(copy(larger), (total_larger, value_larger), node);
        for (block, (big, small)) in [(total_larger, (total, x)), (value_larger, (x, total))] {
            self.start(block);
            // The error of `big + small` is `(big - sum) + small`, exactly, when `big` is the
            // larger in magnitude.
            let error = self.temporary(ty, node);
            self.assign(
                Place::from(error),
                Rvalue::Binary(BinaryOperator::Subtract, copy(big), copy(sum)),
                node,
            );
            self.assign(
                Place::from(error),
                Rvalue::Binary(BinaryOperator::Add, copy(error), copy(small)),
                node,
            );
            self.assign(
                Place::from(compensation),
                Rvalue::Binary(BinaryOperator::Add, copy(compensation), copy(error)),
                node,
            );
            self.terminate(TerminatorKind::Goto(join), node);
        }
        self.start(join);
        self.assign(Place::from(total), Rvalue::Use(copy(sum)), node);
    }

    /// `total += compensation` when the compensation is neither zero nor infinite nor NaN, as
    /// CPython ends its sum: `c - c` is zero exactly when `c` is finite.
    fn add_compensation(
        &mut self,
        (total, compensation): (Local, Local),
        ty: Type<'db>,
        node: NodeId,
    ) {
        let copy = |local: Local| Operand::Copy(Place::from(local));
        let zero_constant = self.number_constant(ty, 0);
        let zero = || Operand::Constant(zero_constant.clone());
        let boolean = self.boolean_type();
        let (nonzero, finite) = (self.temporary(boolean, node), self.temporary(boolean, node));
        self.assign(
            Place::from(nonzero),
            Rvalue::Binary(BinaryOperator::NotEqual, copy(compensation), zero()),
            node,
        );
        let difference = self.temporary(ty, node);
        self.assign(
            Place::from(difference),
            Rvalue::Binary(
                BinaryOperator::Subtract,
                copy(compensation),
                copy(compensation),
            ),
            node,
        );
        self.assign(
            Place::from(finite),
            Rvalue::Binary(BinaryOperator::Equal, copy(difference), zero()),
            node,
        );
        self.assign(
            Place::from(finite),
            Rvalue::Binary(BinaryOperator::BitAnd, copy(finite), copy(nonzero)),
            node,
        );
        let (add, join) = (self.new_block(), self.new_block());
        self.branch(copy(finite), (add, join), node);
        self.start(add);
        self.assign(
            Place::from(total),
            Rvalue::Binary(BinaryOperator::Add, copy(total), copy(compensation)),
            node,
        );
        self.terminate(TerminatorKind::Goto(join), node);
        self.start(join);
    }

    /// A loop that runs `body` with an `i64` index from `first` up to `length`, exclusive.
    fn index_loop(
        &mut self,
        (first, length): (i128, Operand<'db>),
        node: NodeId,
        body: &mut dyn FnMut(&mut Self, Local),
    ) {
        let int = self.int_type();
        let (index, limit) = (self.temporary(int, node), self.temporary(int, node));
        let start = Operand::Constant(Constant::integer(int, first));
        self.assign(Place::from(index), Rvalue::Use(start), node);
        self.assign(Place::from(limit), Rvalue::Use(length), node);
        let header = self.new_block();
        self.terminate(TerminatorKind::Goto(header), node);
        self.start(header);
        let (pass, end) = (self.new_block(), self.new_block());
        self.loop_test((BinaryOperator::Less, index, limit, pass, end), node);
        self.start(pass);
        body(self, index);
        self.assign(
            Place::from(index),
            Rvalue::Binary(
                BinaryOperator::Add,
                Operand::Copy(Place::from(index)),
                Operand::Constant(Constant::integer(int, 1)),
            ),
            node,
        );
        self.terminate(TerminatorKind::Goto(header), node);
        self.start(end);
    }
}
