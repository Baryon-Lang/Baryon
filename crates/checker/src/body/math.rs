//! Calls to the functions of the standard `math` module that the compiler builds today: `sqrt`,
//! `exp`, `log`, `sin`, `cos` and `pow` (language reference, "Math functions" in the arrays
//! chapter).
//!
//! Each takes real numbers and returns a float. An `f32` or `f64` argument keeps its type, so
//! `sqrt(x)` on an `f32` is an `f32`, as `numpy.sqrt` gives. An integer argument converts to
//! `f64`, as Python's `math.sqrt(2)` gives `1.4142135623730951`. `pow(x, y)` mixes its two
//! arguments as `x ** y` does, except that two integers give an `f64`.
//!
//! The constants `pi`, `e`, `tau`, `inf` and `nan` are `f64`s with Python's values.
//!
//! Any other member of `math` stays unchecked until the standard library exists (roadmap 1.13).

use super::Body;
use super::calls::Arguments;
use super::numbers;
use syntax::NodeId;
use types::{Float, Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// A call of `math.<member>`, or of `member` imported from `math`. `None` when the member is
    /// not one this module checks, so the caller types it as unknown.
    pub(super) fn math(
        &mut self,
        node: NodeId,
        member: &str,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        let arity = match member {
            "sqrt" | "exp" | "log" | "sin" | "cos" => 1,
            "pow" => 2,
            _ => return None,
        };
        // `pow(x, 0.5)` types its arguments as `x ** 0.5` does: a number literal takes the
        // other argument's type, so the call stays `f32` when `x` is.
        let types: Vec<Type<'db>> = match (arguments.positional.as_slice(), arity) {
            (&[base, exponent], 2) if arguments.keywords.is_empty() => {
                let (base, exponent) = self.operands(base, exponent);
                vec![base, exponent]
            }
            _ => arguments
                .positional
                .iter()
                .chain(arguments.keywords.iter().map(|(_, value)| value))
                .map(|&argument| self.expression(argument, None))
                .collect(),
        };
        if !arguments.keywords.is_empty() || types.len() != arity {
            let message = match arity {
                1 => format!("`{member}` takes one number"),
                _ => format!("`{member}` takes two numbers"),
            };
            let found = format!(
                "found {}",
                count(types.len(), !arguments.keywords.is_empty())
            );
            return Some(self.lowerer.error(node, &message, &found, None));
        }
        let mut result: Option<Primitive> = None;
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
            result = match result {
                None => Some(primitive),
                Some(before) => match numbers::common(before, primitive) {
                    Some(common) => Some(common),
                    None => return Some(self.mixing_error(node, member, before, primitive)),
                },
            };
        }
        let float = match result {
            Some(Primitive::Float(float)) => float,
            _ => Float::F64,
        };
        Some(Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Float(float)),
        ))
    }
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
