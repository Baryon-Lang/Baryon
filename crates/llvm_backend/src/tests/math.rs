//! Mathematical functions: a [`Builtin::Math`] call becomes a call of the C library's function,
//! with the result C gives.

use super::{Database, build, end, module, parameter, primitive, run, span};
use mir::{Argument, BlockId, Body, Builtin, Callee, Local, MathFunction, Operand, TerminatorKind};
use types::{Float, Mode, Primitive};

/// `def f(x: T, y: T) -> T: return math.hypot(x, y)`, for the float type `float`.
fn hypot(database: &Database, float: Float) -> Body<'_> {
    let ty = primitive(database, Primitive::Float(float));
    let mut body = Body::new(ty, span());
    let x = body.push_local(ty, parameter("x", Mode::Borrow), span());
    let y = body.push_local(ty, parameter("y", Mode::Borrow), span());
    body.push_block(end(TerminatorKind::Call {
        callee: Callee::Builtin(Builtin::Math(MathFunction::Hypot)),
        arguments: vec![
            Argument::Owned(Operand::Copy(x.into())),
            Argument::Owned(Operand::Copy(y.into())),
        ],
        destination: Local::RETURN.into(),
        target: BlockId::new(1),
        error: None,
    }));
    body.push_block(end(TerminatorKind::Return));
    body
}

/// `hypot` gives the length without overflowing where squaring would, and an infinity for an
/// infinite argument even beside NaN, in both widths.
#[test]
fn hypot_calls_the_c_library() {
    let database = Database::default();
    let text = module(
        &database,
        &[
            ("hypot64", hypot(&database, Float::F64)),
            ("hypot32", hypot(&database, Float::F32)),
        ],
    );
    assert!(text.contains("declare double @hypot(double, double)"));
    assert!(text.contains("declare float @hypotf(float, float)"));
    let main = r#"
        #include <math.h>
        #include <stdio.h>
        double hypot64(double, double);
        float hypot32(float, float);
        int main(int count, char **arguments) {
            (void)arguments;
            double one = count;
            printf(
                "%.17g %.17g %g %.9g\n", hypot64(3.0 * one, 4.0), hypot64(1e300 * one, 1e300),
                hypot64(INFINITY * one, NAN), (double)hypot32(3.0f * (float)one, 4.0f)
            );
            return 0;
        }
    "#;
    let executable = build("hypot", &text, main);
    assert_eq!(
        run(&executable, &[]).unwrap(),
        "5 1.4142135623730952e+300 inf 5\n"
    );
}
