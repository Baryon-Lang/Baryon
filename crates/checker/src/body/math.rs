//! Calls to the functions of the standard `math` module for real numbers (language reference,
//! "Math functions" in the arrays chapter; roadmap 1.13.1).
//!
//! Most take real numbers and return a float. An `f32` or `f64` argument keeps its type, so
//! `sqrt(x)` on an `f32` is an `f32`, as `numpy.sqrt` gives. An integer argument converts to
//! `f64`, as Python's `math.sqrt(2)` gives `1.4142135623730951`. A function of two or three
//! numbers, such as `pow(x, y)` or `fma(x, y, z)`, mixes them as `x ** y` does, except that
//! integers alone give an `f64`. `floor`, `ceil` and `trunc` return an `int`, as in Python, and
//! `isnan`, `isinf` and `isfinite` a `bool`.
//!
//! The constants `pi`, `e`, `tau`, `inf` and `nan` are `f64`s with Python's values.
//!
//! The functions of integers alone, such as `gcd`, and those of sequences, such as `fsum`, are
//! not checked yet; a call of one stays unchecked.

use super::Body;
use super::calls::Arguments;
use super::numbers;
use syntax::NodeId;
use types::{Float, Integer, Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// A call of `math.<member>`, or of `member` imported from `math`. `None` when the member is
    /// not one this module checks, so the caller types it as unknown.
    pub(super) fn math(
        &mut self,
        node: NodeId,
        member: &str,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        let (arities, result) = signature(member)?;
        // `pow(x, 0.5)` types its arguments as `x ** 0.5` does: a number literal takes the
        // other argument's type, so the call stays `f32` when `x` is.
        let types: Vec<Type<'db>> = match *arguments.positional.as_slice() {
            [base, exponent] if arguments.keywords.is_empty() => {
                let (base, exponent) = self.operands(base, exponent);
                vec![base, exponent]
            }
            [first, ref rest @ ..] if arguments.keywords.is_empty() => {
                let first = self.expression(first, None);
                let context = self.number_context(first);
                let mut types = vec![first];
                types.extend(
                    rest.iter()
                        .map(|&argument| self.expression(argument, context)),
                );
                types
            }
            _ => arguments
                .positional
                .iter()
                .chain(arguments.keywords.iter().map(|(_, value)| value))
                .map(|&argument| self.expression(argument, None))
                .collect(),
        };
        if !arguments.keywords.is_empty() || !arities.contains(&types.len()) {
            let message = match arities {
                [1] => format!("`{member}` takes one number"),
                [1, 2] => format!("`{member}` takes one or two numbers"),
                [2] => format!("`{member}` takes two numbers"),
                _ => format!("`{member}` takes three numbers"),
            };
            let found = format!(
                "found {}",
                count(types.len(), !arguments.keywords.is_empty())
            );
            return Some(self.lowerer.error(node, &message, &found, None));
        }
        let mut mixed: Option<Primitive> = None;
        for (&ty, &argument) in types.iter().zip(&arguments.positional) {
            let primitive = match ty.kind(self.db) {
                TypeKind::Unknown => return Some(ty),
                &TypeKind::Primitive(primitive @ (Primitive::Integer(_) | Primitive::Float(_))) => {
                    primitive
                }
                _ => {
                    let message = format!(
                        "`{member}` takes a real number, not `{}`",
                        ty.display(self.db)
                    );
                    return Some(self.lowerer.error(
                        argument,
                        &message,
                        "not an integer or a float",
                        None,
                    ));
                }
            };
            mixed = match mixed {
                None => Some(primitive),
                Some(before) => match numbers::common(before, primitive) {
                    Some(common) => Some(common),
                    None => return Some(self.mixing_error(node, member, before, primitive)),
                },
            };
        }
        let primitive = match (result, mixed) {
            (Returns::Integer, _) => Primitive::Integer(Integer::I64),
            (Returns::Bool, _) => Primitive::Bool,
            (Returns::Float, Some(Primitive::Float(float))) => Primitive::Float(float),
            (Returns::Float, _) => Primitive::Float(Float::F64),
        };
        Some(Type::new(self.db, TypeKind::Primitive(primitive)))
    }
}

/// What a `math` function returns, before number mixing picks the float type.
#[derive(Clone, Copy)]
enum Returns {
    /// A float: the arguments' float type, or `f64` when every argument is an integer.
    Float,
    /// An `int`, for `floor`, `ceil` and `trunc`, as Python's are.
    Integer,
    /// A `bool`, for `isnan`, `isinf` and `isfinite`.
    Bool,
}

/// The numbers of arguments `member` of `math` takes and what it returns, or `None` when it is
/// not a function this module checks.
fn signature(member: &str) -> Option<(&'static [usize], Returns)> {
    Some(match member {
        "sqrt" | "cbrt" | "exp" | "exp2" | "expm1" | "log2" | "log10" | "log1p" | "sin" | "cos"
        | "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh" | "tanh" | "asinh" | "acosh"
        | "atanh" | "erf" | "erfc" | "gamma" | "lgamma" | "fabs" | "degrees" | "radians" => {
            (&[1], Returns::Float)
        }
        "log" => (&[1, 2], Returns::Float),
        "pow" | "atan2" | "hypot" | "copysign" | "fmod" => (&[2], Returns::Float),
        "fma" => (&[3], Returns::Float),
        "floor" | "ceil" | "trunc" => (&[1], Returns::Integer),
        "isnan" | "isinf" | "isfinite" => (&[1], Returns::Bool),
        _ => return None,
    })
}

/// Whether `member` is one of the constants of `math` the compiler builds: `pi`, `e`, `tau`,
/// `inf` and `nan`, each an `f64` with Python's value.
pub(super) fn is_constant(member: &str) -> bool {
    matches!(member, "pi" | "e" | "tau" | "inf" | "nan")
}

/// "1 argument", "2 arguments", or "a keyword argument" when any argument was given by keyword,
/// which `math` functions do not take.
fn count(arguments: usize, keyword: bool) -> String {
    match (keyword, arguments) {
        (true, _) => "a keyword argument".to_owned(),
        (false, 1) => "1 argument".to_owned(),
        (false, count) => format!("{count} arguments"),
    }
}
