//! Array methods (language reference, "Methods" in the arrays chapter): `a.copy()` and `a.sum()`.
//!
//! `a.copy()` is the whole-array expression whose only leaf is `a`, so array expansion allocates
//! the new array and copies each element in the same loop nest as `a + b`. `a.sum()` adds every
//! element into an accumulator in one loop nest of its own.

use crate::builder::Builder;
use mir::{BinaryOperator, Elementwise, Operand, Place, Projection, Rvalue};
use syntax::NodeId;
use types::TypeKind;

impl<'db> Builder<'db> {
    /// `a.copy()` on an `Array`, into `place`: a new array with `a`'s lengths and elements.
    pub(crate) fn array_copy(&mut self, place: Place, array: NodeId, node: NodeId) {
        let source = self.borrow(array);
        let tree = Elementwise::Leaf(Operand::Copy(source));
        self.assign(place, Rvalue::Elementwise(Box::new(tree)), node);
    }

    /// `a.sum()` on an `Array` or `Fixed` array of numbers, into `place`: 0, then each element
    /// added in row-major order (decision 0008), one at a time.
    ///
    /// The order is fixed, so a float sum is the same on every run. It is not the most accurate
    /// order: rounding error grows with the element count, where a pairwise sum would keep it
    /// near constant. An empty array sums to 0. Integer sums wrap on overflow, as integer `+`
    /// does until decision D.11.
    pub(crate) fn array_sum(&mut self, place: Place, array: NodeId, node: NodeId) {
        let ty = self.ty(array);
        let element = self.element(ty);
        if !matches!(element.kind(self.db), TypeKind::Primitive(_)) {
            return self.unsupported(node, "`sum` of an array whose elements are not numbers");
        }
        let source = self.borrow(array);
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
        let result = self.ty(node);
        let rvalue = self.coerce(Operand::Copy(Place::from(total)), element, result);
        self.assign(place, rvalue, node);
    }
}
