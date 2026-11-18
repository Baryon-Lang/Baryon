//! The walk of `range(…)` inside `enumerate`, `zip` and `reversed`, as in
//! `for i, x in zip(range(n), xs)`: the [`Walk`] the `iterators` module reads a number from
//! on each pass. It is here to keep that module to one responsibility and one screenful.

use crate::builder::Builder;
use crate::iterators::Walk;
use mir::{BinaryOperator, Constant, Operand, Place, Rvalue};
use syntax::NodeId;

impl<'db> Builder<'db> {
    /// The walk of `range(…)` at `node`: its first number, and how many numbers it has, which
    /// is `ceil((stop - start) / step)` when that is above zero and zero otherwise, as Python's
    /// `len(range(…))` is. `None` after an error, or for a step known only at run time.
    pub(crate) fn range_walk(&mut self, node: NodeId, reversed: bool) -> Option<Walk<'db>> {
        let (positional, _) = self.call_arguments(node);
        let (first, bound, step) = match *positional.as_slice() {
            [bound] => (None, bound, Some(1)),
            [first, bound] => (Some(first), bound, Some(1)),
            [first, bound, stride] => (Some(first), bound, self.constant_integer(stride)),
            _ => None?,
        };
        let Some(step) = step.filter(|&step| step != 0) else {
            self.unsupported(
                node,
                "a `range` step that is not a constant inside `enumerate`, `zip` or `reversed`",
            );
            return None;
        };
        let ty = self.ty(first.unwrap_or(bound));
        let start = self.temporary(ty, node);
        match first {
            Some(first) => self.write(Place::from(start), ty, first),
            None => self.assign(
                Place::from(start),
                Rvalue::Use(Operand::Constant(Constant::integer(ty, 0))),
                node,
            ),
        }
        let end = self.temporary(ty, bound);
        self.write(Place::from(end), ty, bound);
        // The distance to cover in the step's direction, then the steps it takes, rounded up.
        let (from, to) = if step > 0 { (start, end) } else { (end, start) };
        let distance = self.binary_locals(BinaryOperator::Subtract, (to, from), ty, node);
        let length = self.temporary(self.int_type(), node);
        let zero = Rvalue::Use(Operand::Constant(Constant::integer(self.int_type(), 0)));
        self.assign(Place::from(length), zero, node);
        let positive = self.temporary(self.boolean_type(), node);
        self.assign(
            Place::from(positive),
            Rvalue::Binary(
                BinaryOperator::Greater,
                Operand::Copy(Place::from(distance)),
                Operand::Constant(Constant::integer(ty, 0)),
            ),
            node,
        );
        let (count, join) = (self.new_block(), self.new_block());
        self.branch(Operand::Copy(Place::from(positive)), (count, join), node);
        self.start(count);
        let size = step.unsigned_abs();
        let rounded = Rvalue::Binary(
            BinaryOperator::Add,
            Operand::Copy(Place::from(distance)),
            Operand::Constant(Constant::integer(ty, i128::try_from(size - 1).ok()?)),
        );
        self.assign(Place::from(distance), rounded, node);
        let steps = Rvalue::Binary(
            BinaryOperator::FloorDivide,
            Operand::Copy(Place::from(distance)),
            Operand::Constant(Constant::integer(ty, i128::try_from(size).ok()?)),
        );
        let counted = self.temporary(ty, node);
        self.assign(Place::from(counted), steps, node);
        let as_int = self.convert(
            Operand::Copy(Place::from(counted)),
            ty,
            self.int_type(),
            node,
        );
        self.assign(Place::from(length), Rvalue::Use(as_int), node);
        self.jump(&mut Some(join), node);
        self.start(join);
        Some(Walk::Range {
            first: start,
            step,
            length,
            reversed,
            ty,
        })
    }
}
