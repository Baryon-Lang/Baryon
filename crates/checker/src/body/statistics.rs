//! The statistics methods of an array (roadmap 3.16): `a.mean()`, `a.var(ddof=0)` and
//! `a.std(ddof=0)` on an `Array` or a `Fixed` array, with `NumPy`'s names and results.
//!
//! `a.mean()` takes real numbers, as `numpy.mean` does: the mean of floats keeps their type, and
//! the mean of integers is an `f64`. `a.var()` and `a.std()` take floats; `ddof`, the "delta
//! degrees of freedom" subtracted from the count, is an `int` keyword argument, so
//! `a.var(ddof=1)` is the sample variance. Means and variances along an axis are not built yet:
//! any other argument is an error that says so.

use super::Body;
use super::calls::Arguments;
use syntax::NodeId;
use types::{Float, Integer, Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// `a.mean()`, `a.var(...)` or `a.std(...)`, the method `name` of `base`, an array whose
    /// elements are `element`, called with `arguments`: the type of the result.
    pub(super) fn statistic(
        &mut self,
        node: NodeId,
        (base, element): (Type<'db>, Type<'db>),
        name: &str,
        arguments: &Arguments,
    ) -> Type<'db> {
        let float = match element.kind(self.db) {
            TypeKind::Primitive(Primitive::Float(_)) => Some(element),
            TypeKind::Primitive(Primitive::Integer(_)) if name == "mean" => Some(Type::new(
                self.db,
                TypeKind::Primitive(Primitive::Float(Float::F64)),
            )),
            _ => None,
        };
        let Some(result) = float else {
            self.untyped(arguments);
            let message = format!("`{name}` of `{}` is not built", self.show(base));
            let (label, hint) = if name == "mean" {
                (
                    "not real numbers",
                    "`mean` takes an array of integers or floats",
                )
            } else {
                ("not floats", "`var` and `std` take an array of floats")
            };
            return self.lowerer.error(node, &message, label, Some(hint));
        };
        let ddof = match (
            arguments.positional.as_slice(),
            arguments.keywords.as_slice(),
        ) {
            ([], []) => return result,
            ([], [(keyword, value)])
                if name != "mean" && self.tree.text(*keyword, self.text) == "ddof" =>
            {
                *value
            }
            _ => {
                self.untyped(arguments);
                let message = if name == "mean" {
                    "`mean` takes no arguments".to_owned()
                } else {
                    format!("`{name}` takes no arguments or `ddof`")
                };
                let hint = format!(
                    "write `a.{name}()` for the whole array{}; along an axis is not built yet",
                    if name == "mean" {
                        ""
                    } else {
                        ", or `ddof=1` to divide by n - 1"
                    }
                );
                return self
                    .lowerer
                    .error(node, &message, "wrong arguments", Some(&hint));
            }
        };
        let int = Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::I64)),
        );
        let found = self.expression(ddof, Some(int));
        self.expect(ddof, int, found, "`ddof` is");
        result
    }
}
