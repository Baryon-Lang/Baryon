//! `sorted` and `round(x, digits)` (roadmap 1.13.13), each a call of the runtime:
//!
//! | Built-in | Runtime call |
//! | --- | --- |
//! | [`Sort`](mir::Builtin::Sort) of numbers or `bool`s | `baryon_rt_sort(data, count, element, reverse)` |
//! | [`Sort`](mir::Builtin::Sort) of `str`s | `baryon_rt_sort_str(data, count, reverse)` |
//! | [`RoundDigits`](mir::Builtin::RoundDigits) | `baryon_rt_round_f64(x, digits)`, its `f32` form, or `baryon_rt_round_i64(n, digits, location)` |
//!
//! A sort passes the list's element pointer and length from its header, and for numbers the
//! element type as the runtime's `ELEMENT_*` number, the one `print` of an array uses.

use crate::Unsupported;
use crate::function::{Function, block_label};
use crate::list::HEADER;
use mir::{Argument, BlockId, Place};
use types::{Float, Integer, Primitive, TypeKind};

impl<'db> Function<'_, '_, 'db> {
    /// Sorts the list passed `InOut` in place, from the largest when the `bool` passed `Owned`
    /// is true, and goes to `target`.
    pub(crate) fn list_sort(
        &mut self,
        arguments: &[Argument<'db>],
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let [Argument::InOut(list), Argument::Owned(reverse)] = arguments else {
            return Err(Unsupported::new("`sorted` with these arguments"));
        };
        let (header, ty) = self.place(list)?;
        let (element, _) = self.list(ty)?;
        let (reverse, _) = self.value(reverse)?;
        let data = self.emit(&format!("load ptr, ptr {header}"));
        let length_at = self.emit(&format!(
            "getelementptr inbounds {HEADER}, ptr {header}, i32 0, i32 1"
        ));
        let length = self.emit(&format!("load i64, ptr {length_at}"));
        if matches!(element.kind(self.types.db), TypeKind::Str) {
            self.declare(
                "baryon_rt_sort_str",
                "declare void @baryon_rt_sort_str(ptr, i64, i1 zeroext) nounwind",
            );
            self.line(&format!(
                "call void @baryon_rt_sort_str(ptr {data}, i64 {length}, i1 zeroext {reverse})"
            ));
        } else {
            let code = match self.element_code(element) {
                Some(code) if self.complex_part(element).is_none() => code,
                _ => return Err(self.types.unsupported(element)),
            };
            self.declare(
                "baryon_rt_sort",
                "declare void @baryon_rt_sort(ptr, i64, i32, i1 zeroext) nounwind",
            );
            self.line(&format!(
                "call void @baryon_rt_sort(ptr {data}, i64 {length}, i32 {code}, i1 zeroext {reverse})"
            ));
        }
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }

    /// Writes `destination = round(x, digits)` for an `f32`, `f64` or `i64` passed `Owned` and
    /// the digits, an `i64` passed `Owned`, and goes to `target`.
    pub(crate) fn round_digits(
        &mut self,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let [Argument::Owned(number), Argument::Owned(digits)] = arguments else {
            return Err(Unsupported::new("`round` with these arguments"));
        };
        let (number, ty) = self.value(number)?;
        let (digits, _) = self.value(digits)?;
        let (address, _) = self.place(destination)?;
        let (symbol, llvm, location) = match ty.kind(self.types.db) {
            TypeKind::Primitive(Primitive::Float(Float::F64)) => {
                ("baryon_rt_round_f64", "double", None)
            }
            TypeKind::Primitive(Primitive::Float(Float::F32)) => {
                ("baryon_rt_round_f32", "float", None)
            }
            TypeKind::Primitive(Primitive::Integer(Integer::I64)) => (
                "baryon_rt_round_i64",
                "i64",
                Some(self.constants.location(self.span)),
            ),
            _ => return Err(self.types.unsupported(ty)),
        };
        let (parameters, passed) = match &location {
            Some(location) => (", ptr", format!(", ptr {location}")),
            None => ("", String::new()),
        };
        self.declare(
            symbol,
            &format!("declare {llvm} @{symbol}({llvm}, i64{parameters}) nounwind"),
        );
        let rounded = self.emit(&format!(
            "call {llvm} @{symbol}({llvm} {number}, i64 {digits}{passed})"
        ));
        self.line(&format!("store {llvm} {rounded}, ptr {address}"));
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }
}
