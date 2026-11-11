//! The statistics methods of an array (roadmap 3.16): `a.mean()`, `a.var(ddof=0)` and
//! `a.std(ddof=0)` on an `Array` or a `Fixed` array, of the whole array or along one axis, with
//! `NumPy`'s names and results.
//!
//! `a.mean()` takes real numbers, as `numpy.mean` does: the mean of floats keeps their type, and
//! the mean of integers is an `f64`. `a.var()` and `a.std()` take floats; `ddof`, the "delta
//! degrees of freedom" subtracted from the count, is an `int` keyword argument, so
//! `a.var(ddof=1)` is the sample variance. An axis, written as for `a.sum(axis=k)`, positional
//! or `axis=`, reduces along that axis only and takes floats; the result's type is
//! `a.sum(axis=k)`'s. Any other argument is an error.

use super::Body;
use super::calls::Arguments;
use syntax::NodeId;
use types::{Float, Integer, Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// `a.mean(...)`, `a.var(...)` or `a.std(...)`, the method `name` of `base`, an array whose
    /// elements are `element`, called with `arguments`: the type of the result.
    pub(super) fn statistic(
        &mut self,
        node: NodeId,
        (base, element): (Type<'db>, Type<'db>),
        name: &str,
        arguments: &Arguments,
    ) -> Type<'db> {
        let is_float = matches!(
            element.kind(self.db),
            TypeKind::Primitive(Primitive::Float(_))
        );
        let float = match element.kind(self.db) {
            TypeKind::Primitive(Primitive::Float(_)) => Some(element),
            TypeKind::Primitive(Primitive::Integer(_)) if name == "mean" => Some(Type::new(
                self.db,
                TypeKind::Primitive(Primitive::Float(Float::F64)),
            )),
            _ => None,
        };
        let Some(whole) = float else {
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
        let Some((axis, ddof)) = self.statistic_arguments(name, arguments) else {
            self.untyped(arguments);
            let message = if name == "mean" {
                "`mean` takes no arguments or one axis".to_owned()
            } else {
                format!("`{name}` takes an axis, `ddof`, both or neither")
            };
            let hint = format!(
                "write `a.{name}()` for the whole array, or `a.{name}(axis=0)` along the first \
                 axis{}",
                if name == "mean" {
                    ""
                } else {
                    "; `ddof=1` divides by n - 1"
                }
            );
            return self
                .lowerer
                .error(node, &message, "wrong arguments", Some(&hint));
        };
        let result = match axis {
            Some(_) if !is_float => {
                self.untyped(arguments);
                let message = format!(
                    "`{name}` along an axis of `{}` is not built",
                    self.show(base)
                );
                return self.lowerer.error(
                    node,
                    &message,
                    "not floats",
                    Some("along an axis, `mean` takes an array of floats"),
                );
            }
            Some(_) => self.axis_reduction(node, (base, element), name, axis, arguments),
            None => whole,
        };
        if let Some(ddof) = ddof {
            let int = Type::new(
                self.db,
                TypeKind::Primitive(Primitive::Integer(Integer::I64)),
            );
            let found = self.expression(ddof, Some(int));
            self.expect(ddof, int, found, "`ddof` is");
        }
        result
    }

    /// The axis and `ddof` arguments of the statistics method `name`, each `None` when not
    /// given: at most one positional argument, the axis, and the keywords `axis` and, except
    /// for `mean`, `ddof`. `None` for any other arguments.
    fn statistic_arguments(
        &self,
        name: &str,
        arguments: &Arguments,
    ) -> Option<(Option<NodeId>, Option<NodeId>)> {
        let mut axis = match arguments.positional.as_slice() {
            [] => None,
            [axis] => Some(*axis),
            _ => return None,
        };
        let mut ddof = None;
        for &(keyword, value) in &arguments.keywords {
            match self.tree.text(keyword, self.text) {
                "axis" if axis.is_none() => axis = Some(value),
                "ddof" if name != "mean" && ddof.is_none() => ddof = Some(value),
                _ => return None,
            }
        }
        Some((axis, ddof))
    }
}
