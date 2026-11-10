//! Complex numbers: `complex64` and `complex128` values, their operators, conversions to them,
//! `**`, and printing.
//!
//! A complex number is the literal structure `{ float, float }` or `{ double, double }`, the real
//! part then the imaginary part, the layout `types::layout` gives it. The operators work on the
//! parts at their own width, with the formulas CPython 3.12 uses, so a `complex128` result has
//! the bits Python's gives:
//!
//! | Operator | Result |
//! | --- | --- |
//! | `a + b`, `a - b`, `-a` | Part by part |
//! | `a * b` | `(ar*br - ai*bi) + (ar*bi + ai*br)j`, with no fused multiply-add |
//! | `a / b` | Smith's method: divide through by the larger part of `b`, as `_Py_c_quot` does |
//! | `a == b`, `a != b` | Both parts equal; either part differs, so NaN is never equal |
//!
//! Python raises `ZeroDivisionError` for `a / 0j`. Here, as for floats, division by zero gives
//! IEEE 754's result, as `NumPy` computes it: each part of `a` divided by `+0.0`, so `(1+1j) / 0`
//! is `inf+infj` and `1j / 0` is `nan+infj`. `**` calls the runtime, which computes it as
//! CPython does (`baryon_rt_power_complex128`), and `print` calls the runtime with the two parts.

use crate::Unsupported;
use crate::function::{Function, block_label};
use crate::literal::float;
use mir::{BinaryOperator, BlockId, UnaryOperator};
use types::{Float, Primitive, Type, TypeKind};

/// A complex number's two parts, each the name of an LLVM value of its parts' type.
struct Parts {
    /// The real part.
    real: String,
    /// The imaginary part.
    imaginary: String,
}

impl<'db> Function<'_, '_, 'db> {
    /// The float type of the parts of `ty`, `f32` or `f64`, when `ty` is a complex number.
    pub(crate) fn complex_part(&self, ty: Type<'db>) -> Option<Float> {
        match ty.kind(self.types.db) {
            TypeKind::Primitive(Primitive::Complex(complex)) => Some(complex.part()),
            _ => None,
        }
    }

    /// The constant `real + imaginary j` of a complex type whose parts are `part`, written as
    /// an LLVM structure constant.
    pub(crate) fn complex_constant(part: Float, real: f64, imaginary: f64) -> String {
        let single = part == Float::F32;
        let llvm = part_llvm(part);
        format!(
            "{{ {llvm} {}, {llvm} {} }}",
            float(real, single),
            float(imaginary, single)
        )
    }

    /// The two parts of the complex value `value`.
    fn parts(&mut self, part: Float, value: &str) -> Parts {
        let llvm = structure(part);
        Parts {
            real: self.emit(&format!("extractvalue {llvm} {value}, 0")),
            imaginary: self.emit(&format!("extractvalue {llvm} {value}, 1")),
        }
    }

    /// The complex value whose parts are `parts`.
    fn complex_value(&mut self, part: Float, parts: &Parts) -> String {
        let (llvm, part_llvm) = (structure(part), part_llvm(part));
        let real = self.emit(&format!(
            "insertvalue {llvm} poison, {part_llvm} {}, 0",
            parts.real
        ));
        self.emit(&format!(
            "insertvalue {llvm} {real}, {part_llvm} {}, 1",
            parts.imaginary
        ))
    }

    /// The float operation `instruction` on two parts.
    fn part_operation(&mut self, instruction: &str, part: Float, a: &str, b: &str) -> String {
        self.emit(&format!("{instruction} {} {a}, {b}", part_llvm(part)))
    }

    /// `-z` on the complex value `value`; `not` and `~` have no meaning on one.
    pub(crate) fn complex_unary(
        &mut self,
        operator: UnaryOperator,
        part: Float,
        value: &str,
    ) -> Result<String, Unsupported> {
        if operator != UnaryOperator::Negate {
            return Err(Unsupported::new(format!(
                "`{operator:?}` on complex numbers"
            )));
        }
        let parts = self.parts(part, value);
        let llvm = part_llvm(part);
        let negated = Parts {
            real: self.emit(&format!("fneg {llvm} {}", parts.real)),
            imaginary: self.emit(&format!("fneg {llvm} {}", parts.imaginary)),
        };
        Ok(self.complex_value(part, &negated))
    }

    /// `a op b` on two complex values whose parts are `part`.
    pub(crate) fn complex_binary(
        &mut self,
        operator: BinaryOperator,
        part: Float,
        a: &str,
        b: &str,
    ) -> Result<String, Unsupported> {
        let a = self.parts(part, a);
        let b = self.parts(part, b);
        let result = match operator {
            BinaryOperator::Add | BinaryOperator::Subtract => {
                let instruction = if operator == BinaryOperator::Add {
                    "fadd"
                } else {
                    "fsub"
                };
                Parts {
                    real: self.part_operation(instruction, part, &a.real, &b.real),
                    imaginary: self.part_operation(instruction, part, &a.imaginary, &b.imaginary),
                }
            }
            BinaryOperator::Multiply => self.complex_multiply(part, &a, &b),
            BinaryOperator::Divide => self.complex_divide(part, &a, &b),
            BinaryOperator::Equal | BinaryOperator::NotEqual => {
                let (condition, join) = if operator == BinaryOperator::Equal {
                    ("oeq", "and")
                } else {
                    ("une", "or")
                };
                let llvm = part_llvm(part);
                let real = self.emit(&format!("fcmp {condition} {llvm} {}, {}", a.real, b.real));
                let imaginary = self.emit(&format!(
                    "fcmp {condition} {llvm} {}, {}",
                    a.imaginary, b.imaginary
                ));
                return Ok(self.emit(&format!("{join} i1 {real}, {imaginary}")));
            }
            _ => {
                return Err(Unsupported::new(format!(
                    "`{operator:?}` on complex numbers"
                )));
            }
        };
        Ok(self.complex_value(part, &result))
    }

    /// `a * b`, as CPython's `_Py_c_prod` computes it.
    fn complex_multiply(&mut self, part: Float, a: &Parts, b: &Parts) -> Parts {
        let real_real = self.part_operation("fmul", part, &a.real, &b.real);
        let imaginary_imaginary = self.part_operation("fmul", part, &a.imaginary, &b.imaginary);
        let real_imaginary = self.part_operation("fmul", part, &a.real, &b.imaginary);
        let imaginary_real = self.part_operation("fmul", part, &a.imaginary, &b.real);
        Parts {
            real: self.part_operation("fsub", part, &real_real, &imaginary_imaginary),
            imaginary: self.part_operation("fadd", part, &real_imaginary, &imaginary_real),
        }
    }

    /// `a / b` by Smith's method, as CPython's `_Py_c_quot` computes it: when `|b.real|` is
    /// at least `|b.imag|`, divide through by `b.real`, otherwise by `b.imag`, which keeps the
    /// intermediate values from overflowing. When `b` is zero, each part of `a` is divided by
    /// `+0.0`, as `NumPy` does, where Python raises; when a part of `b` is NaN, both parts are
    /// NaN. Both ways are computed and the right one selected, which needs no branch and
    /// cannot trap, since float division never does.
    fn complex_divide(&mut self, part: Float, a: &Parts, b: &Parts) -> Parts {
        let llvm = part_llvm(part);
        let suffix = if part == Float::F32 { "f32" } else { "f64" };
        let fabs = format!("llvm.fabs.{suffix}");
        self.declare(&fabs, &format!("declare {llvm} @{fabs}({llvm})"));
        let real_size = self.emit(&format!("call {llvm} @{fabs}({llvm} {})", b.real));
        let imaginary_size = self.emit(&format!("call {llvm} @{fabs}({llvm} {})", b.imaginary));
        let op = |function: &mut Self, instruction: &str, x: &str, y: &str| {
            function.part_operation(instruction, part, x, y)
        };
        // `|b.real| >= |b.imag|`: ratio = b.imag / b.real.
        let ratio = op(self, "fdiv", &b.imaginary, &b.real);
        let scaled = op(self, "fmul", &b.imaginary, &ratio);
        let denominator = op(self, "fadd", &b.real, &scaled);
        let scaled = op(self, "fmul", &a.imaginary, &ratio);
        let sum = op(self, "fadd", &a.real, &scaled);
        let real_first = op(self, "fdiv", &sum, &denominator);
        let scaled = op(self, "fmul", &a.real, &ratio);
        let difference = op(self, "fsub", &a.imaginary, &scaled);
        let imaginary_first = op(self, "fdiv", &difference, &denominator);
        // `|b.imag| > |b.real|`: ratio = b.real / b.imag.
        let ratio = op(self, "fdiv", &b.real, &b.imaginary);
        let scaled = op(self, "fmul", &b.real, &ratio);
        let denominator = op(self, "fadd", &scaled, &b.imaginary);
        let scaled = op(self, "fmul", &a.real, &ratio);
        let sum = op(self, "fadd", &scaled, &a.imaginary);
        let real_second = op(self, "fdiv", &sum, &denominator);
        let scaled = op(self, "fmul", &a.imaginary, &ratio);
        let difference = op(self, "fsub", &scaled, &a.real);
        let imaginary_second = op(self, "fdiv", &difference, &denominator);
        // `b` is zero: IEEE 754 division of each part by `+0.0`.
        let real_zero = op(self, "fdiv", &a.real, &real_size);
        let imaginary_zero = op(self, "fdiv", &a.imaginary, &real_size);
        let first = self.emit(&format!("fcmp oge {llvm} {real_size}, {imaginary_size}"));
        let second = self.emit(&format!("fcmp oge {llvm} {imaginary_size}, {real_size}"));
        let zero = self.emit(&format!("fcmp oeq {llvm} {real_size}, 0.0"));
        let nan = float(f64::NAN, part == Float::F32);
        let mut pick = |zero_value: &str, first_value: &str, second_value: &str| {
            let large = self.emit(&format!(
                "select i1 {zero}, {llvm} {zero_value}, {llvm} {first_value}"
            ));
            let small = self.emit(&format!(
                "select i1 {second}, {llvm} {second_value}, {llvm} {nan}"
            ));
            self.emit(&format!(
                "select i1 {first}, {llvm} {large}, {llvm} {small}"
            ))
        };
        Parts {
            real: pick(&real_zero, &real_first, &real_second),
            imaginary: pick(&imaginary_zero, &imaginary_first, &imaginary_second),
        }
    }

    /// The complex number whose parts are `part` holding the number `value` of type `from`: a real number converted to the parts' type with no imaginary part, as
    /// Python's `complex(x)` gives, or a complex number's parts converted one by one.
    pub(crate) fn complex_convert(
        &mut self,
        value: &str,
        from: Type<'db>,
        part: Float,
    ) -> Result<String, Unsupported> {
        let part_type = Type::new(self.types.db, TypeKind::Primitive(Primitive::Float(part)));
        let parts = match self.complex_part(from) {
            Some(from_part) if from_part == part => return Ok(value.to_owned()),
            Some(from_part) => {
                let from_type = Type::new(
                    self.types.db,
                    TypeKind::Primitive(Primitive::Float(from_part)),
                );
                let parts = self.parts(from_part, value);
                Parts {
                    real: self.convert_value(&parts.real, from_type, part_type)?,
                    imaginary: self.convert_value(&parts.imaginary, from_type, part_type)?,
                }
            }
            None => Parts {
                real: self.convert_value(value, from, part_type)?,
                imaginary: float(0.0, part == Float::F32),
            },
        };
        Ok(self.complex_value(part, &parts))
    }

    /// Writes `x ** y` on two complex numbers whose parts are `part`, through the runtime's
    /// `baryon_rt_power_complex64` or `baryon_rt_power_complex128`, which writes the result to
    /// `address`, and goes to `target`.
    pub(crate) fn complex_power(
        &mut self,
        part: Float,
        values: &[String],
        address: &str,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let [base, exponent] = values else {
            return Err(Unsupported::new("`**` with other than two operands"));
        };
        let llvm = part_llvm(part);
        let base = self.parts(part, base);
        let exponent = self.parts(part, exponent);
        let function = if part == Float::F32 {
            "baryon_rt_power_complex64"
        } else {
            "baryon_rt_power_complex128"
        };
        self.declare(
            function,
            &format!("declare void @{function}(ptr, {llvm}, {llvm}, {llvm}, {llvm}) nounwind"),
        );
        self.line(&format!(
            "call void @{function}(ptr {address}, {llvm} {}, {llvm} {}, {llvm} {}, {llvm} {})",
            base.real, base.imaginary, exponent.real, exponent.imaginary
        ));
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }

    /// Prints the complex value `value` to `stream` as Python's `repr` writes it, through the
    /// runtime's `baryon_rt_print_complex64` or `baryon_rt_print_complex128`.
    pub(crate) fn print_complex(&mut self, stream: u8, part: Float, value: &str) {
        let llvm = part_llvm(part);
        let parts = self.parts(part, value);
        let function = if part == Float::F32 {
            "baryon_rt_print_complex64"
        } else {
            "baryon_rt_print_complex128"
        };
        self.declare(
            function,
            &format!("declare void @{function}(i32, {llvm}, {llvm}) nounwind"),
        );
        self.line(&format!(
            "call void @{function}(i32 {stream}, {llvm} {}, {llvm} {})",
            parts.real, parts.imaginary
        ));
    }
}

/// The LLVM type of one part: `float` or `double`.
fn part_llvm(part: Float) -> &'static str {
    if part == Float::F32 {
        "float"
    } else {
        "double"
    }
}

/// The LLVM type of a complex number whose parts are `part`.
pub(crate) fn structure(part: Float) -> String {
    let llvm = part_llvm(part);
    format!("{{ {llvm}, {llvm} }}")
}
