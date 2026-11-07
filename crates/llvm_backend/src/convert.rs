//! Conversions between number types, with the range checks the language reference's
//! "Conversions" requires: a narrowing integer conversion, or a float to integer one, that does
//! not keep the value panics.

use crate::Unsupported;
use crate::function::{Function, float};
use crate::panic::Failure;
use crate::ty::Number;
use mir::Operand;
use types::Type;

impl<'db> Function<'_, '_, 'db> {
    /// A number converted to the number type `to`, checked as the language reference's
    /// "Conversions" requires: a narrowing integer conversion, or a float to integer one, that
    /// does not keep the value panics, naming the target type.
    pub(crate) fn convert(
        &mut self,
        operand: &Operand<'db>,
        to: Type<'db>,
    ) -> Result<String, Unsupported> {
        let (value, from) = self.value(operand)?;
        let (source, source_llvm) = self.scalar(from)?;
        let (target, target_llvm) = self.scalar(to)?;
        let instruction = match (source, target) {
            _ if source == target => return Ok(value),
            (Number::Bool, Number::Integer { .. }) => "zext",
            (Number::Bool, Number::Float(_)) => "uitofp",
            (Number::Integer { signed, .. }, Number::Float(_)) => {
                if signed {
                    "sitofp"
                } else {
                    "uitofp"
                }
            }
            (Number::Float(from_bits), Number::Float(to_bits)) => {
                if to_bits > from_bits {
                    "fpext"
                } else {
                    "fptrunc"
                }
            }
            (
                Number::Integer {
                    bits: from_bits,
                    signed: from_signed,
                },
                Number::Integer {
                    bits: to_bits,
                    signed: to_signed,
                },
            ) => {
                let widening = (from_signed == to_signed && to_bits >= from_bits)
                    || (!from_signed && to_signed && to_bits > from_bits);
                if !widening {
                    self.check_integer_fits(
                        &value,
                        &source_llvm,
                        from_signed,
                        to_bits,
                        to_signed,
                        to,
                    );
                }
                match to_bits.cmp(&from_bits) {
                    std::cmp::Ordering::Greater => {
                        if from_signed {
                            "sext"
                        } else {
                            "zext"
                        }
                    }
                    std::cmp::Ordering::Less => "trunc",
                    std::cmp::Ordering::Equal => return Ok(value),
                }
            }
            (Number::Float(from_bits), Number::Integer { bits, signed }) => {
                self.check_float_fits(&value, &source_llvm, from_bits, bits, signed, to);
                if signed { "fptosi" } else { "fptoui" }
            }
            _ => {
                return Err(Unsupported::new(format!(
                    "converting `{}` to `{}`",
                    from.display(self.types.db),
                    to.display(self.types.db)
                )));
            }
        };
        Ok(self.emit(&format!(
            "{instruction} {source_llvm} {value} to {target_llvm}"
        )))
    }

    /// Checks that the integer `value` fits an integer type `bits` wide, signed or not, by
    /// comparing it as an `i128`, which holds every value of every integer type.
    fn check_integer_fits(
        &mut self,
        value: &str,
        llvm: &str,
        from_signed: bool,
        bits: u32,
        signed: bool,
        to: Type<'db>,
    ) {
        let extend = if from_signed { "sext" } else { "zext" };
        let wide = self.emit(&format!("{extend} {llvm} {value} to i128"));
        let (minimum, maximum) = integer_range(bits, signed);
        let above = self.emit(&format!("icmp sge i128 {wide}, {minimum}"));
        let below = self.emit(&format!("icmp sle i128 {wide}, {maximum}"));
        let fits = self.emit(&format!("and i1 {above}, {below}"));
        self.check(&fits, &self.lost_value(to));
    }

    /// Checks that the float `value`, rounded toward zero, fits an integer type `bits` wide,
    /// signed or not; NaN fails both comparisons. The comparisons are in `f64`, where every
    /// bound below is exact.
    fn check_float_fits(
        &mut self,
        value: &str,
        llvm: &str,
        from_bits: u32,
        bits: u32,
        signed: bool,
        to: Type<'db>,
    ) {
        let wide = if from_bits == 32 {
            self.emit(&format!("fpext {llvm} {value} to double"))
        } else {
            value.to_owned()
        };
        let (minimum, maximum) = integer_range(bits, signed);
        // Above `minimum - 1` truncates to `minimum` or more. For 64-bit types `minimum - 1` has
        // no exact `f64`, but nothing lies between it and `minimum` either.
        #[expect(
            clippy::cast_precision_loss,
            reason = "each bound used is a power of two or below 2^53, so it is exact"
        )]
        let above = if bits == 64 {
            format!("fcmp oge double {wide}, {}", float(minimum as f64, false))
        } else {
            format!(
                "fcmp ogt double {wide}, {}",
                float((minimum - 1) as f64, false)
            )
        };
        let above = self.emit(&above);
        #[expect(
            clippy::cast_precision_loss,
            reason = "maximum + 1 is a power of two, exact in f64"
        )]
        let limit = float((maximum + 1) as f64, false);
        let below = self.emit(&format!("fcmp olt double {wide}, {limit}"));
        let fits = self.emit(&format!("and i1 {above}, {below}"));
        self.check(&fits, &self.lost_value(to));
    }

    /// The failure of a conversion to `to` that cannot keep its value.
    fn lost_value(&self, to: Type<'db>) -> Failure {
        Failure::Message(format!(
            "the value does not fit in `{}`",
            to.display(self.types.db)
        ))
    }
}

/// The smallest and largest values of an integer type `bits` wide.
fn integer_range(bits: u32, signed: bool) -> (i128, i128) {
    if signed {
        (-(1_i128 << (bits - 1)), (1_i128 << (bits - 1)) - 1)
    } else {
        (0, (1_i128 << bits) - 1)
    }
}
