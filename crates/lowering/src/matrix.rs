//! The matrix product `a @ b` (language reference, "Whole-array arithmetic"), as a loop nest.
//!
//! `@` takes two `Array`s or two `Fixed` arrays: a matrix times a matrix, `(n, k) @ (k, m)`
//! giving `(n, m)`; a matrix times a vector, `(n, k) @ (k,)` giving `(n,)`; or a vector times a
//! matrix, `(k,) @ (k, m)` giving `(m,)`. Each entry of the result is the sum over `p` of
//! `a[…, p] * b[p, …]`, added one product at a time from `p = 0`, with the elements converted to
//! the result's element type first. For two `Array`s the inner lengths must match, or the
//! program panics, as whole-array arithmetic does; for `Fixed` arrays the checker proved it.
//!
//! The loops run over the result's rows, then its columns, then `p`. That is the textbook order,
//! not the fastest one for large matrices: calling BLAS, whose order is its own, comes with
//! roadmap 3.10. The result is built in a temporary and moved to its place at the end, so
//! `x = m @ x` reads the old `x` throughout.

use crate::builder::Builder;
use mir::{BinaryOperator, Local, Operand, Place, Projection, Rvalue};
use syntax::{NodeId, Role};
use types::TypeKind;

impl Builder<'_> {
    /// `left @ right`, the binary operation `node`, into `place`.
    pub(crate) fn matrix_product(&mut self, place: Place, node: NodeId) {
        let (Some(left), Some(right)) = (
            self.tree.child(node, Role::Left),
            self.tree.child(node, Role::Right),
        ) else {
            return;
        };
        let (left_type, right_type, ty) = (self.ty(left), self.ty(right), self.ty(node));
        let (left_rank, right_rank) = (self.rank(left_type), self.rank(right_type));
        let (left_element, right_element) = (self.element(left_type), self.element(right_type));
        let element = self.element(ty);
        if !matches!(element.kind(self.db), TypeKind::Primitive(_)) {
            return self.unsupported(node, "`@` of arrays whose elements are not numbers");
        }
        let (left, right) = (self.borrow(left), self.borrow(right));
        // The length of the dimension `p` runs over, on each side.
        let inner = self.length(&left, left_type, left_rank - 1, node);
        if matches!(left_type.kind(self.db), TypeKind::Array { .. }) {
            let other = self.length(&right, right_type, 0, node);
            self.same_length(left_rank - 1, inner.clone(), other, node);
        }
        // The result's lengths: `left`'s rows, if it is a matrix, then `right`'s columns, if it
        // is a matrix.
        let mut lengths = Vec::new();
        if left_rank == 2 {
            lengths.push(self.length(&left, left_type, 0, node));
        }
        if right_rank == 2 {
            lengths.push(self.length(&right, right_type, 1, node));
        }
        let result = self.temporary(ty, node);
        let start = if let TypeKind::Array { .. } = ty.kind(self.db) {
            Rvalue::NewArray(ty, lengths.clone())
        } else {
            let Some(zeros) = self.filled(ty, 0, node) else {
                return;
            };
            Rvalue::Use(zeros)
        };
        self.assign(Place::from(result), start, node);
        let (total, product) = (self.temporary(element, node), self.temporary(element, node));
        let (a, b) = (self.temporary(element, node), self.temporary(element, node));
        self.loop_nest(&lengths, &mut Vec::new(), node, &mut |builder, outer| {
            let zero = Operand::Constant(builder.number_constant(element, 0));
            builder.assign(Place::from(total), Rvalue::Use(zero), node);
            builder.loop_nest(
                std::slice::from_ref(&inner),
                &mut Vec::new(),
                node,
                &mut |builder, step| {
                    let p = step[0];
                    // `left[i, p]` or `left[p]`, and `right[p, j]` or `right[p]`.
                    let mut row: Vec<Local> = outer[..usize::from(left_rank == 2)].to_vec();
                    row.push(p);
                    let mut column = vec![p];
                    column.extend_from_slice(&outer[usize::from(left_rank == 2)..]);
                    let read = |builder: &mut Self, local, array: &Place, indices, from| {
                        let value =
                            Operand::Copy(array.clone().project(Projection::Index(indices)));
                        let rvalue = builder.coerce(value, from, element);
                        builder.assign(Place::from(local), rvalue, node);
                    };
                    read(builder, a, &left, row, left_element);
                    read(builder, b, &right, column, right_element);
                    let copy = |local: Local| Operand::Copy(Place::from(local));
                    builder.assign(
                        Place::from(product),
                        Rvalue::Binary(BinaryOperator::Multiply, copy(a), copy(b)),
                        node,
                    );
                    builder.assign(
                        Place::from(total),
                        Rvalue::Binary(BinaryOperator::Add, copy(total), copy(product)),
                        node,
                    );
                },
            );
            let at = Place::from(result).project(Projection::Index(outer.to_vec()));
            builder.assign(at, Rvalue::Use(Operand::Copy(Place::from(total))), node);
        });
        let moved = self.take(Place::from(result), ty);
        self.assign(place, Rvalue::Use(moved), node);
    }
}
