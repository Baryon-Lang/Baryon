//! Mathematical functions: a call of [`Builtin::Math`](mir::Builtin::Math) becomes a call of the
//! LLVM intrinsic for the function, such as `llvm.sqrt.f64`, with the result stored at the
//! destination.
//!
//! The intrinsics are LLVM's own names for the C library's functions: LLVM turns `llvm.sqrt` into
//! one square-root instruction, folds them on constants, and calls `libm` for the rest, which the
//! driver links with `-lm` (decision 0020). Since none of them can fail, the call needs no error
//! edge and no check. Every operand and the result share one float type, as `mir` requires.
//!
//! A [`Builtin::Sum`](mir::Builtin::Sum) of floats calls the runtime's pairwise sum,
//! `baryon_rt_sum_f64` or `baryon_rt_sum_f32`, with the address of the first element and the
//! element count. That relies on the elements being one contiguous run in row-major order, which
//! every `Array` is today (decision 0008) and a `Fixed` array always is; array views, once slices
//! build, will need their strides checked first.

use crate::Unsupported;
use crate::function::{Function, block_label};
use crate::ty::Number;
use mir::{Argument, BlockId, MathFunction, Place};
use types::TypeKind;

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

    /// Writes `destination = array.sum()` for an `Array` or `Fixed` array of floats, through
    /// the runtime's pairwise sum, and goes to `target`.
    pub(crate) fn sum(
        &mut self,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let [Argument::Borrow(array)] = arguments else {
            return Err(Unsupported::new(
                "`sum` with arguments other than one borrowed array",
            ));
        };
        let (address, ty) = self.place(destination)?;
        let (llvm, function) = match self.types.number(ty) {
            Some(Number::Float(32)) => ("float", "baryon_rt_sum_f32"),
            Some(Number::Float(_)) => ("double", "baryon_rt_sum_f64"),
            _ => return Err(self.types.unsupported(ty)),
        };
        let (array_address, array_type) = self.place(array)?;
        let (data, count) = match array_type.kind(self.types.db) {
            TypeKind::Fixed { shape, .. } => (array_address, shape.elements().to_string()),
            &TypeKind::Array { rank, .. } => {
                let data = self.emit(&format!("load ptr, ptr {array_address}"));
                let mut count = "1".to_owned();
                for dimension in 0..rank {
                    let length = self.length(array, dimension)?;
                    count = self.emit(&format!("mul i64 {count}, {length}"));
                }
                (data, count)
            }
            _ => return Err(self.types.unsupported(array_type)),
        };
        self.declare(
            function,
            &format!("declare {llvm} @{function}(ptr, i64) nounwind"),
        );
        let result = self.emit(&format!("call {llvm} @{function}(ptr {data}, i64 {count})"));
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
