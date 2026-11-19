//! `**` of each pair of elements (roadmap 1.9.58), made as lowering makes `**` on two numbers,
//! so `x ** 2` of an array gives, element by element, what `x[i] ** 2` gives: a call of
//! [`MathFunction::Power`] for floats and complex numbers, and for integers repeated squaring
//! with each multiplication checked for overflow and a negative exponent a panic.

use super::Expander;
use mir::{
    Argument, BinaryOperator, Builtin, Check, Constant, MathFunction, Operand, Place, Rvalue,
    TerminatorKind,
};
use types::{Primitive, Type, TypeKind};

impl<'db> Expander<'_, '_, 'db> {
    /// `base ** exponent` on two elements of type `ty`, already converted to it. Returns the
    /// result.
    pub(super) fn power(
        &mut self,
        (base, exponent): (Operand<'db>, Operand<'db>),
        ty: Type<'db>,
    ) -> Operand<'db> {
        let result = self.local(ty);
        if let TypeKind::Primitive(Primitive::Integer(integer)) = ty.kind(self.db) {
            self.integer_power(
                Place::from(result),
                (base, exponent),
                ty,
                integer.is_signed(),
            );
        } else {
            let arguments = vec![Argument::Owned(base), Argument::Owned(exponent)];
            self.call(
                Builtin::Math(MathFunction::Power),
                arguments,
                Place::from(result),
            );
        }
        Operand::Copy(Place::from(result))
    }

    /// `place = base ** exponent` on the integer type `ty` by repeated squaring, as lowering's
    /// `integer_power` makes it for two integers: `result` starts at 1, and for each bit of the
    /// exponent, lowest first, `result *= base` when the bit is set, then `base *= base` while
    /// higher bits remain, about `2 * log2(n)` multiplications. Each multiplication is checked
    /// as [`NoPowerOverflow`](Check::NoPowerOverflow), and a signed exponent below zero panics
    /// naming it; `0 ** 0` is 1, as in Python.
    fn integer_power(
        &mut self,
        place: Place,
        (base, exponent): (Operand<'db>, Operand<'db>),
        ty: Type<'db>,
        signed: bool,
    ) {
        let constant = |value| Operand::Constant(Constant::integer(ty, value));
        let copy = |local| Operand::Copy(Place::from(local));
        let boolean = Type::new(self.db, TypeKind::Primitive(Primitive::Bool));
        let factor = self.local(ty);
        self.assign(Place::from(factor), Rvalue::Use(base));
        let remaining = self.local(ty);
        self.assign(Place::from(remaining), Rvalue::Use(exponent));
        if signed {
            let valid = self.local(boolean);
            let test = Rvalue::Binary(BinaryOperator::GreaterEqual, copy(remaining), constant(0));
            self.assign(Place::from(valid), test);
            let next = self.new_block();
            let check = Check::NonNegativeExponent {
                exponent: copy(remaining),
            };
            self.finish(
                TerminatorKind::Assert {
                    condition: copy(valid),
                    check,
                    target: next,
                },
                next,
            );
        }
        let result = self.local(ty);
        self.assign(Place::from(result), Rvalue::Use(constant(1)));
        let header = self.new_block();
        self.finish(TerminatorKind::Goto(header), header);

        // header: multiply in the factor when the lowest bit is set.
        let bit = self.local(ty);
        let lowest = Rvalue::Binary(BinaryOperator::BitAnd, copy(remaining), constant(1));
        self.assign(Place::from(bit), lowest);
        let set = self.local(boolean);
        let test = Rvalue::Binary(BinaryOperator::NotEqual, copy(bit), constant(0));
        self.assign(Place::from(set), test);
        let (multiply, shift) = (self.new_block(), self.new_block());
        self.branch(copy(set), (multiply, shift), multiply);
        let product = self.checked_as(
            (BinaryOperator::Multiply, Check::NoPowerOverflow),
            (copy(result), copy(factor)),
            ty,
        );
        self.assign(Place::from(result), Rvalue::Use(product));
        self.finish(TerminatorKind::Goto(shift), shift);

        // shift: drop the bit; stop when none remain, else square the factor and go again.
        let shifted = Rvalue::Binary(BinaryOperator::ShiftRight, copy(remaining), constant(1));
        self.assign(Place::from(remaining), shifted);
        let done = self.local(boolean);
        let test = Rvalue::Binary(BinaryOperator::Equal, copy(remaining), constant(0));
        self.assign(Place::from(done), test);
        let (square, end) = (self.new_block(), self.new_block());
        self.branch(copy(done), (end, square), square);
        let squared = self.checked_as(
            (BinaryOperator::Multiply, Check::NoPowerOverflow),
            (copy(factor), copy(factor)),
            ty,
        );
        self.assign(Place::from(factor), Rvalue::Use(squared));
        self.finish(TerminatorKind::Goto(header), end);

        self.assign(place, Rvalue::Use(copy(result)));
    }

    /// Ends the block being written by going to `then` when `condition` holds, else to
    /// `otherwise`, and continues writing `next`.
    fn branch(
        &mut self,
        condition: Operand<'db>,
        (then, otherwise): (mir::BlockId, mir::BlockId),
        next: mir::BlockId,
    ) {
        let terminator = TerminatorKind::Branch {
            condition,
            then,
            otherwise,
        };
        self.finish(terminator, next);
    }
}
