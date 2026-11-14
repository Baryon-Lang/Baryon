//! The run-time checks the expanded loops make on each element, as lowering makes them on two
//! numbers: integer `+`, `-`, `*` and `-x` checked for overflow (decision 0030), and integer `//`
//! and `%` checked for a zero divisor (roadmap 1.9.37).

use super::Expander;
use mir::{
    BinaryOperator, Check, Constant, ConstantValue, Operand, Place, Projection, Rvalue,
    TerminatorKind, UnaryOperator,
};
use types::{Primitive, Type, TypeKind};

impl<'db> Expander<'_, '_, 'db> {
    /// `left op right` for integer `+`, `-` or `*` on elements of type `ty`, checked for
    /// overflow as lowering checks the same operator on two integers (decision 0030): a
    /// [`CheckedBinary`](Rvalue::CheckedBinary) into a `tuple[T, bool]` local and an `Assert`
    /// that its flag is `False`, so a debug build panics at the expression and a release
    /// build's backend drops the check and keeps the wrapped result. Returns the result.
    pub(super) fn checked(
        &mut self,
        operator: BinaryOperator,
        (left, right): (Operand<'db>, Operand<'db>),
        ty: Type<'db>,
    ) -> Operand<'db> {
        let bool = Type::new(self.db, TypeKind::Primitive(Primitive::Bool));
        let pair = self.local(Type::new(self.db, TypeKind::Tuple(vec![ty, bool])));
        self.assign(
            Place::from(pair),
            Rvalue::CheckedBinary(operator, left, right),
        );
        let fits = self.local(bool);
        let flag = Operand::Copy(Place::from(pair).project(Projection::Field(1)));
        self.assign(Place::from(fits), Rvalue::Unary(UnaryOperator::Not, flag));
        let next = self.new_block();
        self.finish(
            TerminatorKind::Assert {
                condition: Operand::Copy(Place::from(fits)),
                check: Check::NoOverflow(operator),
                target: next,
            },
            next,
        );
        Operand::Copy(Place::from(pair).project(Projection::Field(0)))
    }

    /// Checks that `divisor`, an element of integer type `ty`, is not zero before integer `//`
    /// or `%` divides by it, as lowering checks the same operator on two integers: a division
    /// by zero panics with Python's message rather than trapping the processor (roadmap
    /// 1.9.37). A constant other than zero needs no check.
    pub(super) fn nonzero_divisor(&mut self, divisor: &Operand<'db>, ty: Type<'db>) {
        if let Operand::Constant(Constant {
            value: ConstantValue::Integer(value),
            ..
        }) = divisor
            && *value != 0
        {
            return;
        }
        let bool = Type::new(self.db, TypeKind::Primitive(Primitive::Bool));
        let nonzero = self.local(bool);
        let zero = Operand::Constant(Constant::integer(ty, 0));
        self.assign(
            Place::from(nonzero),
            Rvalue::Binary(BinaryOperator::NotEqual, divisor.clone(), zero),
        );
        let next = self.new_block();
        self.finish(
            TerminatorKind::Assert {
                condition: Operand::Copy(Place::from(nonzero)),
                check: Check::NonZeroDivisor,
                target: next,
            },
            next,
        );
    }
}
