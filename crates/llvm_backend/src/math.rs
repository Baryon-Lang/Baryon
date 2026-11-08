//! Mathematical functions: a call of [`Builtin::Math`](mir::Builtin::Math) becomes a call of the
//! LLVM intrinsic for the function, such as `llvm.sqrt.f64`, with the result stored at the
//! destination.
//!
//! The intrinsics are LLVM's own names for the C library's functions: LLVM turns `llvm.sqrt` into
//! one square-root instruction, folds them on constants, and calls `libm` for the rest, which the
//! driver links with `-lm` (decision 0020). Since none of them can fail, the call needs no error
//! edge and no check. Every operand and the result share one float type, as `mir` requires.

use crate::Unsupported;
use crate::function::{Function, block_label};
use crate::ty::Number;
use mir::{Argument, BlockId, MathFunction, Place};

impl<'db> Function<'_, '_, 'db> {
    /// Writes `destination = function(arguments…)` and goes to `target`.
    pub(crate) fn math(
        &mut self,
        function: MathFunction,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let (address, ty) = self.place(destination)?;
        let (llvm, suffix) = match self.types.number(ty) {
            Some(Number::Float(32)) => ("float", "f32"),
            Some(Number::Float(_)) => ("double", "f64"),
            _ => return Err(self.types.unsupported(ty)),
        };
        let mut values = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let value = match argument {
                Argument::Owned(operand) => self.value(operand)?.0,
                Argument::Borrow(place) | Argument::InOut(place) => {
                    let (address, ty) = self.place(place)?;
                    self.load(&address, ty)?
                        .ok_or_else(|| Unsupported::new("`None` where a value is needed"))?
                }
            };
            values.push(format!("{llvm} {value}"));
        }
        let intrinsic = format!("llvm.{}.{suffix}", intrinsic(function));
        let parameters = vec![llvm; function.arity()].join(", ");
        self.declare(
            &intrinsic,
            &format!("declare {llvm} @{intrinsic}({parameters})"),
        );
        let result = self.emit(&format!("call {llvm} @{intrinsic}({})", values.join(", ")));
        self.line(&format!("store {llvm} {result}, ptr {address}"));
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }
}

/// The name of the LLVM intrinsic for `function`, without its `llvm.` prefix and type suffix.
fn intrinsic(function: MathFunction) -> &'static str {
    match function {
        MathFunction::Sqrt => "sqrt",
        MathFunction::Exp => "exp",
        MathFunction::Log => "log",
        MathFunction::Sin => "sin",
        MathFunction::Cos => "cos",
        MathFunction::Power => "pow",
        MathFunction::RoundEven => "roundeven",
    }
}
