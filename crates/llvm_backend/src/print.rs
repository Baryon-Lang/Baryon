//! `print`: a call of [`Builtin::Print`](mir::Builtin::Print) becomes one runtime call per
//! argument. Lowering passes the separator and the line end as arguments of their own, so this
//! module writes nothing between them.
//!
//! The runtime formats each value as Python does (`baryon_rt_print_*`, roadmap 1.12.5). A signed
//! integer is widened to `i64` and an unsigned one to `u64`, so two functions cover every width;
//! a float keeps its width, so an `f32` prints its own shortest digits. A string literal, which
//! lowering passes as a constant, becomes a byte constant printed with `baryon_rt_print_str`. A
//! `str` value prints its bytes the same way, with the "not owned" bit masked off its length
//! (decision 0022). Each runtime call names its stream first: 1, standard output, for `print`.
//! Complex numbers, enums and anything else are not printed yet.

use crate::Unsupported;
use crate::function::{Function, block_label};
use crate::ty::Number;
use mir::{Argument, BlockId, ConstantValue, Operand};
use types::{Type, TypeKind};

/// The runtime's number for standard output, where `print` writes.
pub(crate) const STDOUT: u8 = 1;

/// The runtime's number for standard error, where the report of an error that ends `main`
/// goes.
pub(crate) const STDERR: u8 = 2;

impl<'db> Function<'_, '_, 'db> {
    /// Writes `print(arguments…)` and goes to `target`.
    pub(crate) fn print(
        &mut self,
        arguments: &[Argument<'db>],
        target: BlockId,
    ) -> Result<(), Unsupported> {
        for argument in arguments {
            self.print_argument(argument)?;
        }
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }

    /// Prints one argument with the runtime function for its type.
    fn print_argument(&mut self, argument: &Argument<'db>) -> Result<(), Unsupported> {
        let (value, ty) = match argument {
            Argument::Owned(Operand::Constant(constant)) => {
                if let ConstantValue::Str(text) = &constant.value {
                    self.print_text(STDOUT, text.as_bytes());
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
        self.print_value(STDOUT, &value, ty)
    }

    /// Prints `value`, of type `ty`, to `stream` with the runtime function for its type.
    pub(crate) fn print_value(
        &mut self,
        stream: u8,
        value: &str,
        ty: Type<'db>,
    ) -> Result<(), Unsupported> {
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
            TypeKind::Str => {
                let (bytes, length) = self.text_parts(value);
                self.print_bytes(stream, &bytes, &length);
                return Ok(());
            }
            _ => None,
        };
        let (function, parameter, value) = match number.ok_or_else(printing)? {
            Number::Bool => ("baryon_rt_print_bool", "i1 zeroext", value.to_owned()),
            Number::Float(32) => ("baryon_rt_print_f32", "float", value.to_owned()),
            Number::Float(_) => ("baryon_rt_print_f64", "double", value.to_owned()),
            Number::Integer { bits, signed } => {
                let function = if signed {
                    "baryon_rt_print_i64"
                } else {
                    "baryon_rt_print_u64"
                };
                let value = match (bits, signed) {
                    (64, _) => value.to_owned(),
                    (_, true) => self.emit(&format!("sext i{bits} {value} to i64")),
                    (_, false) => self.emit(&format!("zext i{bits} {value} to i64")),
                };
                (function, "i64", value)
            }
        };
        self.declare(
            function,
            &format!("declare void @{function}(i32, {parameter}) nounwind"),
        );
        self.line(&format!(
            "call void @{function}(i32 {stream}, {parameter} {value})"
        ));
        Ok(())
    }

    /// Prints the bytes `text` to `stream` through a byte constant.
    pub(crate) fn print_text(&mut self, stream: u8, text: &[u8]) {
        let constant = self.constants.bytes(text);
        self.print_bytes(stream, &constant, &text.len().to_string());
    }

    /// Prints the `str` value `value` to `stream` quoted, as Python's `repr` writes it.
    pub(crate) fn print_quoted(&mut self, stream: u8, value: &str) {
        let (bytes, length) = self.text_parts(value);
        self.declare(
            "baryon_rt_print_str_quoted",
            "declare void @baryon_rt_print_str_quoted(i32, ptr, i64) nounwind",
        );
        self.line(&format!(
            "call void @baryon_rt_print_str_quoted(i32 {stream}, ptr {bytes}, i64 {length})"
        ));
    }

    /// Prints the `length` bytes at `bytes` to `stream`.
    fn print_bytes(&mut self, stream: u8, bytes: &str, length: &str) {
        self.declare(
            "baryon_rt_print_str",
            "declare void @baryon_rt_print_str(i32, ptr, i64) nounwind",
        );
        self.line(&format!(
            "call void @baryon_rt_print_str(i32 {stream}, ptr {bytes}, i64 {length})"
        ));
    }
}
