//! Sums of a whole array, `a.sum()` (language reference, "Methods" in the arrays chapter), on
//! `Array` and `Fixed` arrays: floats through the runtime's pairwise sum, so they add in
//! `NumPy`'s order, and integers and `bool`s in a loop nest of their own. A `bool` adds as an
//! `int`, counting the `True` elements, as `NumPy` does (roadmap 1.9.36). `a.mean()` and
//! `a.var()` sum through here too, and sums along one axis are in the `axes` module.

use crate::builder::Builder;
use mir::{
    Argument, BinaryOperator, Builtin, Callee, LocalKind, Operand, Place, Projection, Rvalue,
    StatementKind, TerminatorKind,
};
use syntax::NodeId;
use types::TypeKind;

impl<'db> Builder<'db> {
    /// `a.sum()` on an `Array` or `Fixed` array of numbers, into `place`.
    ///
    /// Floats are added in `NumPy`'s pairwise tree, by a call of [`Builtin::Sum`], so the result
    /// has the same bits as `numpy.sum` and rounding error grows only with the logarithm of the
    /// length (Collins, 2026-11-08, roadmap I.22). Integers are added in a loop nest from 0 in
    /// row-major order (decision 0008), since integer addition is exact in any order; each
    /// addition is checked for overflow as `+` is, so a debug build panics and a release build
    /// wraps (decision 0030). An empty array sums to 0. The sum of a whole-array expression,
    /// such as `(a * b).sum()`, of floats or integers, is left to array expansion, which adds in
    /// the same order without storing the expression (decision 0041, amended).
    pub(crate) fn array_sum(&mut self, place: Place, array: NodeId, node: NodeId) {
        let ty = self.ty(array);
        let element = self.element(ty);
        if !matches!(element.kind(self.db), TypeKind::Primitive(_)) {
            return self.unsupported(node, "`sum` of an array whose elements are not numbers");
        }
        // A complex sum must add in `NumPy`'s pairwise tree, as a float sum does, which the
        // runtime does not do for complex numbers yet; adding in a loop would give other bits.
        if self.is_complex(element) {
            return self.unsupported(node, "`sum` of an array of complex numbers");
        }
        let source = self.borrow(array);
        let result = self.ty(node);
        if self.just_assigned_expression(&source) {
            return self.expression_sum(place, (&source, element, result), node);
        }
        self.sum_of(place, (&source, ty, result), node);
    }

    /// Whether the last statement of the current block assigns a whole-array expression to
    /// `place`, a temporary `Array` with no projection: the shape array expansion sums or
    /// reduces in one pass when a call of `Builtin::Sum` or `Builtin::SumAlong` reads it next
    /// and nothing else does (decision 0041, amended).
    pub(crate) fn just_assigned_expression(&self, place: &Place) -> bool {
        let Some(declaration) = self.body.locals.get(place.local.index()) else {
            return false;
        };
        let last = self.body.blocks[self.current().index()].statements.last();
        place.projections.is_empty()
            && declaration.kind == LocalKind::Temporary
            && matches!(declaration.ty.kind(self.db), TypeKind::Array { .. })
            && matches!(
                last.map(|statement| &statement.kind),
                Some(StatementKind::Assign(stored, Rvalue::Elementwise(_))) if stored == place
            )
    }

    /// `e.sum()` of a whole-array expression `e` that has just been computed into the
    /// temporary `source`, with elements of type `element`, into `place` as `result`: a call of
    /// [`Builtin::Sum`] and a drop of `source` right after it, the shape array expansion sums
    /// without storing `e` (decision 0041, amended). It adds as [`sum_of`](Self::sum_of)
    /// would: floats in the pairwise tree, integers and `bool`s one at a time with checked `+`.
    fn expression_sum(
        &mut self,
        place: Place,
        (source, element, result): (&Place, types::Type<'db>, types::Type<'db>),
        node: NodeId,
    ) {
        let counted = self.summand(element);
        let total = self.temporary(counted, node);
        let next = self.new_block();
        self.terminate(
            TerminatorKind::Call {
                callee: Callee::Builtin(Builtin::Sum),
                arguments: vec![Argument::Borrow(source.clone())],
                destination: Place::from(total),
                target: next,
                error: None,
            },
            node,
        );
        self.start(next);
        self.drop_now(source.local, node);
        let rvalue = self.coerce(Operand::Copy(Place::from(total)), counted, result);
        self.assign(place, rvalue, node);
    }

    /// The sum of the array at `source`, of type `ty`, into `place` as type `result`, as
    /// [`array_sum`](Self::array_sum) describes; the caller has checked the elements are real
    /// numbers. `a.mean()` and `a.var()` sum through here too.
    pub(crate) fn sum_of(
        &mut self,
        place: Place,
        (source, ty, result): (&Place, types::Type<'db>, types::Type<'db>),
        node: NodeId,
    ) {
        let element = self.element(ty);
        if self.is_float(element) {
            let total = self.temporary(element, node);
            let next = self.new_block();
            self.terminate(
                TerminatorKind::Call {
                    callee: Callee::Builtin(Builtin::Sum),
                    arguments: vec![Argument::Borrow(source.clone())],
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
            .map(|dimension| self.length(source, ty, dimension, node))
            .collect();
        let counted = self.summand(element);
        let total = self.temporary(counted, node);
        let zero = Operand::Constant(self.number_constant(counted, 0));
        self.assign(Place::from(total), Rvalue::Use(zero), node);
        self.loop_nest(&lengths, &mut Vec::new(), node, &mut |builder, indices| {
            let value = source.clone().project(Projection::Index(indices.to_vec()));
            let value = builder.as_summand(value, element, node);
            let operands = (Operand::Copy(Place::from(total)), value);
            builder.arithmetic(
                Place::from(total),
                BinaryOperator::Add,
                operands,
                counted,
                node,
            );
        });
        let rvalue = self.coerce(Operand::Copy(Place::from(total)), counted, result);
        self.assign(place, rvalue, node);
    }

    /// `mask.any()`, or `mask.all()` when `all` is set, of the array of `bool`s at `source`,
    /// of type `ty`, into `place`: whether an element is `True`, or whether every one is.
    ///
    /// The `True` elements are counted as `mask.sum()` counts them, a loop the backend
    /// vectorizes, and the count is compared with 0 or with the number of elements. So an
    /// empty array gives `False` for `any` and `True` for `all`, as `NumPy` does. Cost: one
    /// pass over the whole array; it does not stop at the first element that settles it.
    pub(crate) fn truth_of(
        &mut self,
        place: Place,
        all: bool,
        (source, ty): (&Place, types::Type<'db>),
        node: NodeId,
    ) {
        let int = self.int_type();
        let count = self.temporary(int, node);
        self.sum_of(Place::from(count), (source, ty, int), node);
        let count = Operand::Copy(Place::from(count));
        if !all {
            let zero = Operand::Constant(self.number_constant(int, 0));
            let found = Rvalue::Binary(BinaryOperator::NotEqual, count, zero);
            return self.assign(place, found, node);
        }
        let size = self.temporary(int, node);
        let one = Operand::Constant(self.number_constant(int, 1));
        self.assign(Place::from(size), Rvalue::Use(one), node);
        for dimension in 0..self.rank(ty) {
            let length = self.length(source, ty, dimension, node);
            let operands = (Operand::Copy(Place::from(size)), length);
            self.arithmetic(
                Place::from(size),
                BinaryOperator::Multiply,
                operands,
                int,
                node,
            );
        }
        let size = Operand::Copy(Place::from(size));
        self.assign(
            place,
            Rvalue::Binary(BinaryOperator::Equal, count, size),
            node,
        );
    }

    /// The type a sum of elements of type `element` adds in: `int` for `bool`, which counts the
    /// `True` elements as `NumPy` does (roadmap 1.9.36), and `element` itself otherwise.
    pub(crate) fn summand(&self, element: types::Type<'db>) -> types::Type<'db> {
        if matches!(
            element.kind(self.db),
            TypeKind::Primitive(types::Primitive::Bool)
        ) {
            self.int_type()
        } else {
            element
        }
    }

    /// The element at `value`, of type `element`, as a term of a sum: converted to `int`
    /// through a temporary when it is a `bool`, as [`summand`](Self::summand) says, and read as
    /// it is otherwise.
    pub(crate) fn as_summand(
        &mut self,
        value: Place,
        element: types::Type<'db>,
        node: NodeId,
    ) -> Operand<'db> {
        let counted = self.summand(element);
        if counted == element {
            return Operand::Copy(value);
        }
        let term = self.temporary(counted, node);
        self.assign(
            Place::from(term),
            Rvalue::Convert(Operand::Copy(value), counted),
            node,
        );
        Operand::Copy(Place::from(term))
    }
}
