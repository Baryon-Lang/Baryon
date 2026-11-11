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
    Place, Rvalue, TerminatorKind,
};
use syntax::NodeId;
use types::{Type, TypeKind};

impl<'db> Builder<'db> {
    /// `a.mean()`, `a.var(ddof)` or `a.std(ddof)`, the method `method` called with `arguments`,
    /// the receiver first and then `ddof` if it is given, into `place`.
    pub(crate) fn array_statistic(
        &mut self,
        place: Place,
        method: &str,
        arguments: &[NodeId],
        node: NodeId,
    ) {
        let (array, ddof) = match arguments {
            [array] => (*array, None),
            [array, ddof] if method != "mean" => (*array, Some(*ddof)),
            _ => return self.unsupported(node, &format!("these arguments to `{method}`")),
        };
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
        self.divide(variance, freedom, result, node);
        let variance = Operand::Copy(Place::from(variance));
        if method == "std" {
            self.math_call(place, MathFunction::Sqrt, vec![variance], node);
        } else {
            self.assign(place, Rvalue::Use(variance), node);
        }
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
}
