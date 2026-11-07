//! `print`: a call of [`Builtin::Print`](mir::Builtin::Print) becomes one runtime call per
//! argument, with a space between arguments and a line end after the last.
//!
//! The runtime formats each value as Python does (`npl_rt_print_*`, roadmap 1.12.5). A signed
//! integer is widened to `i64` and an unsigned one to `u64`, so two functions cover every width;
//! a float keeps its width, so an `f32` prints its own shortest digits. A string literal, which
//! lowering passes as a constant, becomes a byte constant printed with `npl_rt_print_str`, as
//! the separator and the line end are. `str` values, complex numbers, enums and anything else
//! are not printed yet.

use crate::Unsupported;
use crate::function::{Function, block_label};
use crate::ty::Number;
use mir::{Argument, BlockId, ConstantValue, Operand};
use types::TypeKind;

impl<'db> Function<'_, '_, 'db> {
    /// Writes `print(arguments…)` and goes to `target`.
    pub(crate) fn print(
        &mut self,
        arguments: &[Argument<'db>],
        target: BlockId,
    ) -> Result<(), Unsupported> {
        for (position, argument) in arguments.iter().enumerate() {
            if position > 0 {
                self.print_text(b" ");
            }
            self.print_argument(argument)?;
        }
        self.print_text(b"\n");
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }

    /// Prints one argument with the runtime function for its type.
    fn print_argument(&mut self, argument: &Argument<'db>) -> Result<(), Unsupported> {
        let (value, ty) = match argument {
            Argument::Owned(Operand::Constant(constant)) => {
                if let ConstantValue::Str(text) = &constant.value {
                    self.print_text(text.as_bytes());
                    return Ok(());
                }
                self.value(&Operand::Constant(constant.clone()))?
            }
            Argument::Owned(operand) => self.value(operand)?,
            Argument::Borrow(place) | Argument::InOut(place) => {
                let (address, ty) = self.place(place)?;
                let value = self
                    .load(&address, ty)?
                    .ok_or_else(|| Unsupported::new("printing `None`"))?;
                (value, ty)
            }
        };
        let printing = || {
            Unsupported::new(format!(
                "printing values of type `{}`",
                ty.display(self.types.db)
            ))
        };
        // An enum is a number to the backend, but prints by its case's name, which needs the
        // runtime to know the names.
        let number = match ty.kind(self.types.db) {
            TypeKind::Primitive(_) => self.types.number(ty),
            _ => None,
        };
        let (function, parameter, value) = match number.ok_or_else(printing)? {
            Number::Bool => ("npl_rt_print_bool", "i1 zeroext", value),
            Number::Float(32) => ("npl_rt_print_f32", "float", value),
            Number::Float(_) => ("npl_rt_print_f64", "double", value),
            Number::Integer { bits, signed } => {
                let function = if signed {
                    "npl_rt_print_i64"
                } else {
                    "npl_rt_print_u64"
                };
                let value = match (bits, signed) {
                    (64, _) => value,
                    (_, true) => self.emit(&format!("sext i{bits} {value} to i64")),
                    (_, false) => self.emit(&format!("zext i{bits} {value} to i64")),
                };
                (function, "i64", value)
            }
        };
        self.declare(
            function,
            &format!("declare void @{function}({parameter}) nounwind"),
        );
        self.line(&format!("call void @{function}({parameter} {value})"));
        Ok(())
    }

    /// Prints the bytes `text` through a byte constant.
    fn print_text(&mut self, text: &[u8]) {
        let constant = self.constants.bytes(text);
        self.declare(
            "npl_rt_print_str",
            "declare void @npl_rt_print_str(ptr, i64) nounwind",
        );
        self.line(&format!(
            "call void @npl_rt_print_str(ptr {constant}, i64 {})",
            text.len()
        ));
    }
}
