//! Mathematical functions: a call of [`Builtin::Math`](mir::Builtin::Math) becomes a call of the
//! LLVM intrinsic for the function, such as `llvm.sqrt.f64`, or, for a function LLVM 18 has no
//! intrinsic for, such as `tan`, of the C math library's own `tan` or `tanf`, with the result
//! stored at the destination.
//!
//! The intrinsics are LLVM's own names for the C library's functions: LLVM turns `llvm.sqrt` into
//! one square-root instruction, folds them on constants, and calls `libm` for the rest, which the
//! driver links with `-lm` (decision 0020). Either way the result is the C library's, so it can
//! differ in the last bit between platforms, as `NumPy`'s does. Since none of them can fail, the call needs no error
//! edge and no check. Every operand and the result share one float type, as `mir` requires.
//!
//! A [`Builtin::Sum`](mir::Builtin::Sum) of floats calls the runtime's pairwise sum. A `Fixed`
//! array is one contiguous run in row-major order, so it passes the address of the first element
//! and the element count to `baryon_rt_sum_f64` or `baryon_rt_sum_f32`. An `Array` may be a
//! view, a slice that skips elements or walks backwards, so it passes its element pointer,
//! rank, lengths and strides to `baryon_rt_sum_strided_f64` or `baryon_rt_sum_strided_f32`,
//! which sum a contiguous array where it lies and gather any other first.

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
        let result = match routine(function) {
            Routine::Intrinsic(name) => {
                let intrinsic = format!("llvm.{name}.{suffix}");
                let parameters = vec![llvm; function.arity()].join(", ");
                self.declare(
                    &intrinsic,
                    &format!("declare {llvm} @{intrinsic}({parameters})"),
                );
                self.emit(&format!("call {llvm} @{intrinsic}({})", values.join(", ")))
            }
            Routine::Library(name) => {
                let symbol = library_symbol(name, suffix);
                let parameters = vec![llvm; function.arity()].join(", ");
                self.declare(
                    &symbol,
                    &format!("declare {llvm} @{symbol}({parameters}) nounwind"),
                );
                self.emit(&format!("call {llvm} @{symbol}({})", values.join(", ")))
            }
            Routine::LogGamma => {
                // `lgamma_r` writes the sign of gamma through its pointer instead of into C's
                // global `signgam`, so calls in parallel loops never race.
                let symbol = library_symbol("lgamma", suffix) + "_r";
                self.declare(
                    &symbol,
                    &format!("declare {llvm} @{symbol}({llvm}, ptr) nounwind"),
                );
                let sign = self.slot("i32");
                self.emit(&format!(
                    "call {llvm} @{symbol}({}, ptr {sign})",
                    values.join(", ")
                ))
            }
        };
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
        let (llvm, whole_run, through_strides) = match self.types.number(ty) {
            Some(Number::Float(32)) => ("float", "baryon_rt_sum_f32", "baryon_rt_sum_strided_f32"),
            Some(Number::Float(_)) => ("double", "baryon_rt_sum_f64", "baryon_rt_sum_strided_f64"),
            _ => return Err(self.types.unsupported(ty)),
        };
        let (array_address, array_type) = self.place(array)?;
        let result = match array_type.kind(self.types.db) {
            TypeKind::Fixed { shape, .. } => {
                self.declare(
                    whole_run,
                    &format!("declare {llvm} @{whole_run}(ptr, i64) nounwind"),
                );
                let count = shape.elements();
                self.emit(&format!(
                    "call {llvm} @{whole_run}(ptr {array_address}, i64 {count})"
                ))
            }
            &TypeKind::Array { rank, .. } => {
                // Any `Array` may be a view with strides, so the runtime reads them.
                let header = self.types.value(array_type)?;
                let data = self.emit(&format!("load ptr, ptr {array_address}"));
                let [lengths, strides] = [1, 2].map(|part| {
                    self.emit(&format!(
                        "getelementptr inbounds {header}, ptr {array_address}, i32 0, i32 {part}, i32 0"
                    ))
                });
                self.declare(
                    through_strides,
                    &format!("declare {llvm} @{through_strides}(ptr, i32, ptr, ptr) nounwind"),
                );
                self.emit(&format!(
                    "call {llvm} @{through_strides}(ptr {data}, i32 {rank}, ptr {lengths}, ptr {strides})"
                ))
            }
            _ => return Err(self.types.unsupported(array_type)),
        };
        self.line(&format!("store {llvm} {result}, ptr {address}"));
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }
}

/// How the backend computes a [`MathFunction`].
enum Routine {
    /// An LLVM intrinsic, named without its `llvm.` prefix and type suffix. LLVM folds these on
    /// constants and turns the ones the target has an instruction for, such as `sqrt` and
    /// `floor`, into that instruction.
    Intrinsic(&'static str),
    /// A function of the C math library, named by its `f64` form: `tan`, whose `f32` form is
    /// `tanf`.
    Library(&'static str),
    /// `lgamma_r` or `lgammaf_r`, which take a pointer for the sign of gamma.
    LogGamma,
}

/// How `function` is computed. Intrinsics cover the functions LLVM 18, the oldest the driver
/// supports, has one for; the C library covers the rest.
fn routine(function: MathFunction) -> Routine {
    match function {
        MathFunction::Sqrt => Routine::Intrinsic("sqrt"),
        MathFunction::Exp => Routine::Intrinsic("exp"),
        MathFunction::Exp2 => Routine::Intrinsic("exp2"),
        MathFunction::Log => Routine::Intrinsic("log"),
        MathFunction::Log2 => Routine::Intrinsic("log2"),
        MathFunction::Log10 => Routine::Intrinsic("log10"),
        MathFunction::Sin => Routine::Intrinsic("sin"),
        MathFunction::Cos => Routine::Intrinsic("cos"),
        MathFunction::Power => Routine::Intrinsic("pow"),
        MathFunction::Fabs => Routine::Intrinsic("fabs"),
        MathFunction::CopySign => Routine::Intrinsic("copysign"),
        MathFunction::FusedMultiplyAdd => Routine::Intrinsic("fma"),
        MathFunction::Floor => Routine::Intrinsic("floor"),
        MathFunction::Ceil => Routine::Intrinsic("ceil"),
        MathFunction::Truncate => Routine::Intrinsic("trunc"),
        MathFunction::RoundEven => Routine::Intrinsic("roundeven"),
        MathFunction::Cbrt => Routine::Library("cbrt"),
        MathFunction::ExpMinusOne => Routine::Library("expm1"),
        MathFunction::LogOnePlus => Routine::Library("log1p"),
        MathFunction::Tan => Routine::Library("tan"),
        MathFunction::Asin => Routine::Library("asin"),
        MathFunction::Acos => Routine::Library("acos"),
        MathFunction::Atan => Routine::Library("atan"),
        MathFunction::Atan2 => Routine::Library("atan2"),
        MathFunction::Sinh => Routine::Library("sinh"),
        MathFunction::Cosh => Routine::Library("cosh"),
        MathFunction::Tanh => Routine::Library("tanh"),
        MathFunction::Asinh => Routine::Library("asinh"),
        MathFunction::Acosh => Routine::Library("acosh"),
        MathFunction::Atanh => Routine::Library("atanh"),
        MathFunction::Erf => Routine::Library("erf"),
        MathFunction::Erfc => Routine::Library("erfc"),
        MathFunction::Gamma => Routine::Library("tgamma"),
        MathFunction::Hypot => Routine::Library("hypot"),
        MathFunction::TruncatedRemainder => Routine::Library("fmod"),
        MathFunction::LogGamma => Routine::LogGamma,
    }
}

/// The C library's name for `name` at the float type `suffix`: `name` itself for `f64`, and
/// `name` with `f` after it for `f32`, as C names `tanf`.
fn library_symbol(name: &str, suffix: &str) -> String {
    match suffix {
        "f32" => format!("{name}f"),
        _ => name.to_owned(),
    }
}
