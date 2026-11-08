//! Array methods (language reference, "Methods" in the arrays chapter): `a.copy()`, `a.sum()`,
//! and `v.dot(w)` and `v.round()` on `Fixed` arrays.
//!
//! `a.copy()` is the whole-array expression whose only leaf is `a`, so array expansion allocates
//! the new array and copies each element in the same loop nest as `a + b`. `a.sum()` of floats
//! calls the runtime's pairwise sum; of integers, it and `v.dot(w)` add into an accumulator in a
//! loop nest of their own. `v.round()` builds a new
//! `Fixed` value from each element rounded, with one call per element, since a `Fixed` array is
//! small and its length is known when compiling.

use crate::builder::Builder;
use mir::{
    Argument, BinaryOperator, Builtin, Callee, Constant, Elementwise, MathFunction, Operand, Place,
    Projection, Rvalue, TerminatorKind,
};
use syntax::NodeId;
use types::TypeKind;

impl<'db> Builder<'db> {
    /// `a.copy()` on an `Array`, into `place`: a new array with `a`'s lengths and elements.
    pub(crate) fn array_copy(&mut self, place: Place, array: NodeId, node: NodeId) {
        let source = self.borrow(array);
        let tree = Elementwise::Leaf(Operand::Copy(source));
        self.assign(place, Rvalue::Elementwise(Box::new(tree)), node);
    }

    /// `a.sum()` on an `Array` or `Fixed` array of numbers, into `place`.
    ///
    /// Floats are added in `NumPy`'s pairwise tree, by a call of [`Builtin::Sum`], so the result
    /// has the same bits as `numpy.sum` and rounding error grows only with the logarithm of the
    /// length (Collins, 2026-11-08, roadmap I.22). Integers are added in a loop nest from 0 in
    /// row-major order (decision 0008), since integer addition is exact in any order; they wrap
    /// on overflow, as integer `+` does until decision D.11. An empty array sums to 0.
    pub(crate) fn array_sum(&mut self, place: Place, array: NodeId, node: NodeId) {
        let ty = self.ty(array);
        let element = self.element(ty);
        if !matches!(element.kind(self.db), TypeKind::Primitive(_)) {
            return self.unsupported(node, "`sum` of an array whose elements are not numbers");
        }
        let source = self.borrow(array);
        let result = self.ty(node);
        if self.is_float(element) {
            let total = self.temporary(element, node);
            let next = self.new_block();
            self.terminate(
                TerminatorKind::Call {
                    callee: Callee::Builtin(Builtin::Sum),
                    arguments: vec![Argument::Borrow(source)],
                    destination: Place::from(total),
                    target: next,
                    error: None,
                },
                node,
            );
            self.start(next);
            let rvalue = self.coerce(Operand::Copy(Place::from(total)), element, result);
            return self.assign(place, rvalue, node);
        }
        let lengths: Vec<Operand<'db>> = (0..self.rank(ty))
            .map(|dimension| self.length(&source, ty, dimension, node))
            .collect();
        let total = self.temporary(element, node);
        let zero = Operand::Constant(self.number_constant(element, 0));
        self.assign(Place::from(total), Rvalue::Use(zero), node);
        self.loop_nest(&lengths, &mut Vec::new(), node, &mut |builder, indices| {
            let value = source.clone().project(Projection::Index(indices.to_vec()));
            builder.assign(
                Place::from(total),
                Rvalue::Binary(
                    BinaryOperator::Add,
                    Operand::Copy(Place::from(total)),
                    Operand::Copy(value),
                ),
                node,
            );
        });
        let rvalue = self.coerce(Operand::Copy(Place::from(total)), element, result);
        self.assign(place, rvalue, node);
    }

    /// `v.dot(w)` on two rank-1 `Fixed` arrays of numbers, into `place`: the products of the
    /// elements at each index, added in order from index 0. `Array.dot` waits on the length
    /// check two `Array` operands need.
    pub(crate) fn fixed_dot(
        &mut self,
        place: Place,
        (left, right): (NodeId, NodeId),
        node: NodeId,
    ) {
        let ty = self.ty(left);
        let element = self.element(ty);
        if !matches!(element.kind(self.db), TypeKind::Primitive(_)) {
            return self.unsupported(node, "`dot` of vectors whose elements are not numbers");
        }
        let (left, right) = (self.borrow(left), self.borrow(right));
        let length = self.length(&left, ty, 0, node);
        let total = self.temporary(element, node);
        let zero = Operand::Constant(self.number_constant(element, 0));
        self.assign(Place::from(total), Rvalue::Use(zero), node);
        let product = self.temporary(element, node);
        self.loop_nest(&[length], &mut Vec::new(), node, &mut |builder, indices| {
            let at = |place: &Place| {
                Operand::Copy(place.clone().project(Projection::Index(indices.to_vec())))
            };
            builder.assign(
                Place::from(product),
                Rvalue::Binary(BinaryOperator::Multiply, at(&left), at(&right)),
                node,
            );
            builder.assign(
                Place::from(total),
                Rvalue::Binary(
                    BinaryOperator::Add,
                    Operand::Copy(Place::from(total)),
                    Operand::Copy(Place::from(product)),
                ),
                node,
            );
        });
        self.assign(place, Rvalue::Use(Operand::Copy(Place::from(total))), node);
    }

    /// `v.round()` on a `Fixed` array of floats, into `place`: each element rounded to the
    /// nearest whole number, ties to even, still a float. On integers it is a copy.
    pub(crate) fn fixed_round(&mut self, place: Place, array: NodeId, node: NodeId) {
        let ty = self.ty(array);
        let TypeKind::Fixed { element, shape } = ty.kind(self.db) else {
            return self.unsupported(node, "`round` of this array");
        };
        let (element, lengths) = (*element, shape.lengths().to_vec());
        let source = self.borrow(array);
        if !self.is_float(element) {
            return self.assign(place, Rvalue::Use(Operand::Copy(source)), node);
        }
        let int = self.int_type();
        let mut rounded = Vec::new();
        let mut position = vec![0u64; lengths.len()];
        while position
            .iter()
            .zip(&lengths)
            .all(|(index, length)| index < length)
        {
            let indices = position
                .iter()
                .map(|&index| {
                    let local = self.temporary(int, node);
                    let constant = Constant::integer(int, i128::from(index));
                    self.assign(
                        Place::from(local),
                        Rvalue::Use(Operand::Constant(constant)),
                        node,
                    );
                    local
                })
                .collect();
            let value = Operand::Copy(source.clone().project(Projection::Index(indices)));
            let result = self.temporary(element, node);
            self.math_call(
                Place::from(result),
                MathFunction::RoundEven,
                vec![value],
                node,
            );
            rounded.push(Operand::Copy(Place::from(result)));
            // The next index in row-major order: the last dimension counts fastest.
            for dimension in (0..lengths.len()).rev() {
                position[dimension] += 1;
                if position[dimension] < lengths[dimension] || dimension == 0 {
                    break;
                }
                position[dimension] = 0;
            }
        }
        self.assign(place, Rvalue::Aggregate(ty, rounded), node);
    }
}
