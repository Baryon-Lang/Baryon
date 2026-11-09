//! Tests of [`Builtin::Math`](mir::Builtin::Math): every [`MathFunction`] at `f64` and `f32`
//! gives the C math library's result, bit for bit, on arguments that cover each function's
//! ordinary values, its edges and NaN.

use super::{Database, build, end, module, parameter, primitive, run, span};
use mir::{Argument, BlockId, Body, Builtin, Callee, Local, MathFunction, Operand, TerminatorKind};
use std::fmt::Write as _;
use types::{Float, Mode, Primitive, Type};

/// Every function, with the C library function or expression that computes it for `double`
/// arguments `a`, `b` and `c`; `f32` uses the same name with `f` after it.
const FUNCTIONS: &[(MathFunction, &str)] = &[
    (MathFunction::Sqrt, "sqrt"),
    (MathFunction::Cbrt, "cbrt"),
    (MathFunction::Exp, "exp"),
    (MathFunction::Exp2, "exp2"),
    (MathFunction::ExpMinusOne, "expm1"),
    (MathFunction::Log, "log"),
    (MathFunction::Log2, "log2"),
    (MathFunction::Log10, "log10"),
    (MathFunction::LogOnePlus, "log1p"),
    (MathFunction::Sin, "sin"),
    (MathFunction::Cos, "cos"),
    (MathFunction::Tan, "tan"),
    (MathFunction::Asin, "asin"),
    (MathFunction::Acos, "acos"),
    (MathFunction::Atan, "atan"),
    (MathFunction::Atan2, "atan2"),
    (MathFunction::Sinh, "sinh"),
    (MathFunction::Cosh, "cosh"),
    (MathFunction::Tanh, "tanh"),
    (MathFunction::Asinh, "asinh"),
    (MathFunction::Acosh, "acosh"),
    (MathFunction::Atanh, "atanh"),
    (MathFunction::Erf, "erf"),
    (MathFunction::Erfc, "erfc"),
    (MathFunction::Gamma, "tgamma"),
    (MathFunction::LogGamma, "lgamma"),
    (MathFunction::Fabs, "fabs"),
    (MathFunction::Hypot, "hypot"),
    (MathFunction::CopySign, "copysign"),
    (MathFunction::TruncatedRemainder, "fmod"),
    (MathFunction::FusedMultiplyAdd, "fma"),
    (MathFunction::Power, "pow"),
    (MathFunction::Floor, "floor"),
    (MathFunction::Ceil, "ceil"),
    (MathFunction::Truncate, "trunc"),
    (MathFunction::RoundEven, "rint"),
];

/// `def f(a: T, …) -> T: return function(a, …)`, with as many parameters as `function` takes.
fn call(ty: Type<'_>, function: MathFunction) -> Body<'_> {
    let mut body = Body::new(ty, span());
    let arguments = ["a", "b", "c"][..function.arity()]
        .iter()
        .map(|name| {
            let local = body.push_local(ty, parameter(name, Mode::Borrow), span());
            Argument::Owned(Operand::Copy(local.into()))
        })
        .collect();
    body.push_block(end(TerminatorKind::Call {
        callee: Callee::Builtin(Builtin::Math(function)),
        arguments,
        destination: Local::RETURN.into(),
        target: BlockId::new(1),
        error: None,
    }));
    body.push_block(end(TerminatorKind::Return));
    body
}

/// The C program that calls each compiled function, `baryon_<name>` and `baryon_<name>f`, on
/// every combination of the test values and prints each one whose bits differ from the C
/// library's. Two NaNs count as equal, since their payloads may differ.
fn main_program() -> String {
    let mut declarations = String::new();
    let mut checks = String::new();
    for &(function, c) in FUNCTIONS {
        let arity = function.arity();
        for (ty, suffix) in [("double", ""), ("float", "f")] {
            let parameters = vec![ty; arity].join(", ");
            let name = format!("baryon_{c}{suffix}");
            let _ = writeln!(declarations, "{ty} {name}({parameters});");
            let arguments = ["x[i]", "x[j]", "x[k]"][..arity].join(", ");
            let library = if c == "lgamma" {
                format!("lgamma{suffix}_r({arguments}, &sign)")
            } else {
                format!("{c}{suffix}({arguments})")
            };
            let mut loops = String::new();
            for index in &["i", "j", "k"][..arity] {
                let _ = write!(loops, "for (int {index} = 0; {index} < N; {index}++) ");
            }
            let _ = write!(
                checks,
                "    {{ {ty} x[N]; for (int n = 0; n < N; n++) x[n] = ({ty})values[n] * one;\n      \
                 {loops}{{ {ty} got = {name}({arguments}), want = {library};\n        \
                 if (!same(got, want)) printf(\"{c}{suffix}: %a gives %a, not %a\\n\", \
                 (double)x[i], (double)got, (double)want); }} }}\n"
            );
        }
    }
    format!(
        "#define _GNU_SOURCE\n#include <math.h>\n#include <stdio.h>\n#include <string.h>\n\
         double lgamma_r(double, int *);\nfloat lgammaf_r(float, int *);\n{declarations}\
         static int same(double a, double b) {{ return (isnan(a) && isnan(b)) || \
         memcmp(&a, &b, sizeof a) == 0; }}\n\
         #define N 12\n\
         static const double values[N] = {{0.0, -0.0, 0.5, -0.5, 1.0, -1.0, 2.5, -3.75, 10.0, \
         1e-9, INFINITY, NAN}};\n\
         int main(int count, char **arguments) {{\n    (void)arguments;\n    \
         double one = count; int sign;\n{checks}    puts(\"done\");\n    return 0;\n}}\n"
    )
}

/// Each function at `f64` and `f32` gives the C library's bits on ordinary values, signed
/// zeros, values outside its domain, infinities and NaN, so lowering can rely on a
/// [`MathFunction`] meaning exactly the C function it is named after.
#[test]
fn every_math_function_matches_the_c_library() {
    let database = Database::default();
    let mut functions = Vec::new();
    for &(function, c) in FUNCTIONS {
        for (float, suffix) in [(Float::F64, ""), (Float::F32, "f")] {
            let ty = primitive(&database, Primitive::Float(float));
            functions.push((format!("baryon_{c}{suffix}"), call(ty, function)));
        }
    }
    let functions: Vec<(&str, Body<'_>)> = functions
        .iter()
        .map(|(name, body)| (name.as_str(), body.clone()))
        .collect();
    let text = module(&database, &functions);
    let executable = build("math", &text, &main_program());
    assert_eq!(run(&executable, &[]).unwrap(), "done\n");
}
