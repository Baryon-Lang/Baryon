//! The statistics methods of an array (roadmap 3.16): `a.mean()`, `a.var(ddof=0)` and
//! `a.std(ddof=0)` on an `Array` or a `Fixed` array, with the bits of `numpy.mean`, `numpy.var`
//! and `numpy.std`.
//!
//! Each follows `NumPy`'s own steps, so each rounding happens where `NumPy`'s does:
//!
//! - `a.mean()` is `a.sum()` divided by the number of elements, `n`. A float sum adds in the
//!   pairwise tree of `a.sum()`, as `NumPy`'s does. An integer sum is exact and converted to `f64`
//!   once; `NumPy` adds the elements as floats, which is exact too while the sum stays below
//!   2⁵³, so the bits are the same up to there.
//! - `a.var(ddof)` is the pairwise sum of `(a - m) * (a - m)`, where `m` is the mean, divided by
//!   `max(n - ddof, 0)`. The squares are written as the whole-array expression they are, so
//!   array expansion sums them in one pass without storing them (decision 0041), with the bits
//!   of summing the stored array.
//! - `a.std(ddof)` is the square root of `a.var(ddof)`.
//!
//! An empty array's mean is NaN, 0 / 0, as `NumPy`'s is; it warns where this is silent. A
//! variance whose `n - ddof` is 0 or less divides by 0, giving infinity or NaN as `NumPy`'s does.
//!
//! Cost: `a.mean()` reads the array once; `a.var()` and `a.std()` read it twice, once for the
//! mean and once for the squares, and allocate one leaf buffer of 128 elements, not a second
//! array.

use crate::builder::Builder;
use mir::{
    Argument, BinaryOperator, Builtin, Callee, Constant, Elementwise, Local, MathFunction, Operand,
    Place, Projection, Rvalue, SliceIndex, TerminatorKind,
};
use syntax::NodeId;
use types::{Type, TypeKind};

impl<'db> Builder<'db> {
    /// `a.mean()`, `a.var(ddof)` or `a.std(ddof)`, each with an optional axis, the method
    /// `method` called with `arguments`, the receiver first, into `place`. Which argument is the
    /// axis and which `ddof` is read from the call's syntax, since either may be a keyword.
    pub(crate) fn array_statistic(
        &mut self,
        place: Place,
        method: &str,
        arguments: &[NodeId],
        node: NodeId,
    ) {
        let Some(&array) = arguments.first() else {
            return self.unsupported(node, &format!("`{method}` without an array"));
        };
        let (positional, keywords) = self.call_arguments(node);
        let keyword = |name: &str| {
            keywords
                .iter()
                .find(|(keyword, _)| *keyword == name)
                .map(|&(_, value)| value)
        };
        let axis = positional.first().copied().or_else(|| keyword("axis"));
        let ddof = keyword("ddof");
        if let Some(axis) = axis {
            let rank = self.rank(self.ty(array));
            let Some(axis) = self.axis_position(axis, rank) else {
                return;
            };
            if rank > 1 {
                return self.axis_statistic(place, method, (array, axis, ddof), node);
            }
        }
        let ty = self.ty(array);
        let element = self.element(ty);
        let result = self.ty(node);
        let real = matches!(element.kind(self.db), TypeKind::Primitive(_))
            && !self.is_complex(element)
            && self.is_float(result);
        if !real {
            return self.unsupported(
                node,
                &format!("`{method}` of an array whose elements are not real numbers"),
            );
        }
        let source = self.borrow(array);
        let count = self.element_count(&source, ty, node);
        let mean = self.temporary(result, node);
        self.sum_of(Place::from(mean), (&source, ty, result), node);
        self.divide(mean, count, result, node);
        if method == "mean" {
            return self.assign(place, Rvalue::Use(Operand::Copy(Place::from(mean))), node);
        }
        let variance = self.temporary(result, node);
        self.sum_of_squares(variance, (&source, ty), mean, node);
        let freedom = self.freedom(count, ddof, node);
        self.divide(variance, freedom, result, node);
        let variance = Operand::Copy(Place::from(variance));
        if method == "std" {
            self.math_call(place, MathFunction::Sqrt, vec![variance], node);
        } else {
            self.assign(place, Rvalue::Use(variance), node);
        }
    }

    /// A new `i64` local holding `max(count - ddof, 0)`, the divisor of a variance, where
    /// `count` is an `i64` local and `ddof` the argument, 0 when it is not given.
    fn freedom(&mut self, count: Local, ddof: Option<NodeId>, node: NodeId) -> Local {
        let int = self.int_type();
        let ddof = match ddof {
            Some(ddof) => self.operand_as(ddof, int),
            None => Operand::Constant(Constant::integer(int, 0)),
        };
        let freedom = self.temporary(int, node);
        let operands = (Operand::Copy(Place::from(count)), ddof);
        self.arithmetic(
            Place::from(freedom),
            BinaryOperator::Subtract,
            operands,
            int,
            node,
        );
        self.clamp_to_zero(freedom, node);
        freedom
    }

    /// A new `i64` local holding the number of elements of the array at `source`, of type `ty`:
    /// the product of its lengths.
    fn element_count(&mut self, source: &Place, ty: Type<'db>, node: NodeId) -> Local {
        let int = self.int_type();
        let count = self.temporary(int, node);
        let one = Operand::Constant(Constant::integer(int, 1));
        self.assign(Place::from(count), Rvalue::Use(one), node);
        for dimension in 0..self.rank(ty) {
            let length = self.length(source, ty, dimension, node);
            let product = Rvalue::Binary(
                BinaryOperator::Multiply,
                Operand::Copy(Place::from(count)),
                length,
            );
            self.assign(Place::from(count), product, node);
        }
        count
    }

    /// `value = value / float(count)`, where `value` is a local of the float type `float` and
    /// `count` an `i64` local.
    fn divide(&mut self, value: Local, count: Local, float: Type<'db>, node: NodeId) {
        let int = self.int_type();
        let divisor = self.temporary(float, node);
        let converted = self.coerce(Operand::Copy(Place::from(count)), int, float);
        self.assign(Place::from(divisor), converted, node);
        let quotient = Rvalue::Binary(
            BinaryOperator::Divide,
            Operand::Copy(Place::from(value)),
            Operand::Copy(Place::from(divisor)),
        );
        self.assign(Place::from(value), quotient, node);
    }

    /// Sets the `i64` local `value` to 0 when it is negative, as `NumPy`'s `max(n - ddof, 0)`.
    fn clamp_to_zero(&mut self, value: Local, node: NodeId) {
        let int = self.int_type();
        let negative = self.temporary(self.boolean_type(), node);
        let zero = Operand::Constant(Constant::integer(int, 0));
        let test = Rvalue::Binary(
            BinaryOperator::Less,
            Operand::Copy(Place::from(value)),
            zero.clone(),
        );
        self.assign(Place::from(negative), test, node);
        let (clamp, join) = (self.new_block(), self.new_block());
        self.terminate(
            TerminatorKind::Branch {
                condition: Operand::Copy(Place::from(negative)),
                then: clamp,
                otherwise: join,
            },
            node,
        );
        self.start(clamp);
        self.assign(Place::from(value), Rvalue::Use(zero), node);
        self.terminate(TerminatorKind::Goto(join), node);
        self.start(join);
    }

    /// Writes into `total` the pairwise sum of `(a - mean) * (a - mean)` over the float array
    /// at `source`, of type `ty`, as `NumPy`'s `var` computes it before dividing.
    ///
    /// The squares are a whole-array expression into a temporary that only a call of
    /// [`Builtin::Sum`] reads and its target block drops, the form array expansion sums in one
    /// pass (decision 0041). A `Fixed` array's squares are stored, since it is small.
    fn sum_of_squares(
        &mut self,
        total: Local,
        (source, ty): (&Place, Type<'db>),
        mean: Local,
        node: NodeId,
    ) {
        let deviation = Elementwise::Binary(
            BinaryOperator::Subtract,
            Box::new(Elementwise::Leaf(Operand::Copy(source.clone()))),
            Box::new(Elementwise::Leaf(Operand::Copy(Place::from(mean)))),
        );
        let squares = Elementwise::Binary(
            BinaryOperator::Multiply,
            Box::new(deviation.clone()),
            Box::new(deviation),
        );
        self.enter();
        let stored = self.temporary(ty, node);
        self.assign(
            Place::from(stored),
            Rvalue::Elementwise(Box::new(squares)),
            node,
        );
        self.drop_later(stored);
        let next = self.new_block();
        self.terminate(
            TerminatorKind::Call {
                callee: Callee::Builtin(Builtin::Sum),
                arguments: vec![Argument::Borrow(Place::from(stored))],
                destination: Place::from(total),
                target: next,
                error: None,
            },
            node,
        );
        self.start(next);
        self.leave(node);
    }

    /// `a.mean(axis=k)`, `a.var(axis=k, ddof)` or `a.std(axis=k, ddof)` of `array`, of rank 2
    /// or more with float elements, the method `method`, into `place`: an array without
    /// dimension `axis`, each element the statistic of one line along it, with `NumPy`'s steps.
    ///
    /// The means are `a.sum(axis=k)`, in its order, each divided by the length along `k`. A
    /// variance subtracts the means, given a new dimension of length 1 at `k` so they broadcast
    /// along it, as `NumPy`'s `keepdims=True` does, squares the differences into a new array,
    /// sums that along `k` in `a.sum(axis=k)`'s order, and divides by `max(n - ddof, 0)`.
    fn axis_statistic(
        &mut self,
        place: Place,
        method: &str,
        (array, axis, ddof): (NodeId, u32, Option<NodeId>),
        node: NodeId,
    ) {
        let ty = self.ty(array);
        let element = self.element(ty);
        let result = self.ty(node);
        if !self.is_float(element) {
            return self.unsupported(
                node,
                &format!("`{method}` along an axis of an array whose elements are not floats"),
            );
        }
        let source = self.borrow(array);
        let int = self.int_type();
        let along = self.temporary(int, node);
        let length = self.length(&source, ty, axis, node);
        self.assign(Place::from(along), Rvalue::Use(length), node);
        let means = self.temporary(result, node);
        self.reduce_along(Place::from(means), "sum", (&source, ty, result), axis, node);
        self.divide_each(means, along, result, node);
        if method == "mean" {
            let moved = self.take(Place::from(means), result);
            return self.assign(place, Rvalue::Use(moved), node);
        }
        self.drop_later(means);
        let spread = self.with_new_axis(Place::from(means), result, axis, node);
        let deviation = Elementwise::Binary(
            BinaryOperator::Subtract,
            Box::new(Elementwise::Leaf(Operand::Copy(source.clone()))),
            Box::new(Elementwise::Leaf(Operand::Copy(spread))),
        );
        let squares = Elementwise::Binary(
            BinaryOperator::Multiply,
            Box::new(deviation.clone()),
            Box::new(deviation),
        );
        let stored = self.temporary(ty, node);
        self.assign(
            Place::from(stored),
            Rvalue::Elementwise(Box::new(squares)),
            node,
        );
        self.drop_later(stored);
        let variances = self.temporary(result, node);
        let summed = (&Place::from(stored), ty, result);
        self.reduce_along(Place::from(variances), "sum", summed, axis, node);
        let freedom = self.freedom(along, ddof, node);
        self.divide_each(variances, freedom, result, node);
        if method == "std" {
            self.each_element(variances, result, node, &mut |builder, at| {
                builder.math_call(
                    at.clone(),
                    MathFunction::Sqrt,
                    vec![Operand::Copy(at)],
                    node,
                );
            });
        }
        let moved = self.take(Place::from(variances), result);
        self.assign(place, Rvalue::Use(moved), node);
    }

    /// Divides every element of the float array `values`, of type `ty`, in place by
    /// `float(count)`, where `count` is an `i64` local.
    fn divide_each(&mut self, values: Local, count: Local, ty: Type<'db>, node: NodeId) {
        let float = self.element(ty);
        let divisor = self.temporary(float, node);
        let converted = self.coerce(Operand::Copy(Place::from(count)), self.int_type(), float);
        self.assign(Place::from(divisor), converted, node);
        self.each_element(values, ty, node, &mut |builder, at| {
            let quotient = Rvalue::Binary(
                BinaryOperator::Divide,
                Operand::Copy(at.clone()),
                Operand::Copy(Place::from(divisor)),
            );
            builder.assign(at, quotient, node);
        });
    }

    /// Runs `body` with the place of each element of the array `values`, of type `ty`, in
    /// row-major order.
    fn each_element(
        &mut self,
        values: Local,
        ty: Type<'db>,
        node: NodeId,
        body: &mut dyn FnMut(&mut Self, Place),
    ) {
        let lengths: Vec<Operand<'db>> = (0..self.rank(ty))
            .map(|dimension| self.length(&Place::from(values), ty, dimension, node))
            .collect();
        self.loop_nest(&lengths, &mut Vec::new(), node, &mut |builder, indices| {
            let at = Place::from(values).project(Projection::Index(indices.to_vec()));
            body(builder, at);
        });
    }

    /// A view of the whole array at `values`, of type `ty`, with a new dimension of length 1
    /// inserted at `axis`, as `NumPy`'s `keepdims=True` leaves it, so it broadcasts along that
    /// dimension against the array it was reduced from.
    fn with_new_axis(&mut self, values: Place, ty: Type<'db>, axis: u32, node: NodeId) -> Place {
        let int = self.int_type();
        let constant = |builder: &mut Self, value: i128| {
            let local = builder.temporary(int, node);
            let operand = Operand::Constant(Constant::integer(int, value));
            builder.assign(Place::from(local), Rvalue::Use(operand), node);
            local
        };
        let (start, step) = (constant(self, 0), constant(self, 1));
        let mut entries = Vec::new();
        for dimension in 0..self.rank(ty) {
            let length = self.temporary(int, node);
            let operand = self.length(&values, ty, dimension, node);
            self.assign(Place::from(length), Rvalue::Use(operand), node);
            entries.push(SliceIndex::Range {
                start,
                length,
                step,
            });
        }
        entries.insert(axis as usize, SliceIndex::NewAxis);
        values.project(Projection::Slice(entries))
    }
}
