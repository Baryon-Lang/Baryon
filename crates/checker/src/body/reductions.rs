//! The built-ins that reduce a sequence to one number: `sum(values)`, `sum(values, start)`,
//! `min(values)` and `max(values)` of a list or a rank-1 array of numbers (roadmap 1.13.13).
//!
//! `min` and `max` of two or more numbers are the `builtins` module's. These take one sequence.

use super::calls::Arguments;
use super::{Body, numbers, relations};
use syntax::NodeId;
use types::{Type, TypeKind};

impl<'db> Body<'db> {
    /// The type of `name(values, …)`, where `name` is `sum`, `min` or `max` and `types` are the
    /// arguments' types, positional then keyword, or `None` when the call is not one of these
    /// reductions of a sequence and the caller should check it as before.
    ///
    /// `sum` gives the element type, or the common type of the elements and `start` when one
    /// is given, as `+` mixes them; `min` and `max` give the element type. A sequence of
    /// anything but numbers, or an array of rank 2 or more, is an error.
    pub(super) fn reduction(
        &mut self,
        node: NodeId,
        name: &str,
        arguments: &Arguments,
        types: &[Type<'db>],
    ) -> Option<Type<'db>> {
        let (&sequence, &ty) = (arguments.positional.first()?, types.first()?);
        let element = match ty.kind(self.db) {
            TypeKind::List(element) | TypeKind::Array { element, rank: 1 } => *element,
            TypeKind::Fixed { element, shape } if shape.rank() == 1 => *element,
            TypeKind::Array { .. } | TypeKind::Fixed { .. } => {
                let message = format!("`{name}` of an array of rank 2 or more");
                return Some(self.lowerer.error(
                    sequence,
                    &message,
                    "not rank 1",
                    Some(&format!("reduce every element with `a.{name}()`")),
                ));
            }
            _ if name == "sum" => {
                if relations::is_open(self.db, ty) {
                    return Some(self.lowerer.unknown());
                }
                let message = format!(
                    "`sum` adds a list or an array of numbers, found `{}`",
                    self.show(ty)
                );
                return Some(
                    self.lowerer
                        .error(sequence, &message, "not a sequence", None),
                );
            }
            _ => return None,
        };
        if name != "sum" && types.len() != 1 {
            return None;
        }
        let primitive = match element.kind(self.db) {
            TypeKind::Primitive(primitive) if numbers::is_number(*primitive) => *primitive,
            // Python orders `str`s and tuples too; lowering reports those as not built yet.
            _ if name != "sum" => return Some(element),
            _ => return Some(self.not_numbers(sequence, ty)),
        };
        if name != "sum" {
            if self.complex(element).is_some() {
                let message = format!("`{name}` is not defined on complex numbers");
                return Some(self.lowerer.error(
                    sequence,
                    &message,
                    "complex numbers",
                    Some("complex numbers have no order; compare their moduli with `abs(z)`"),
                ));
            }
            return Some(element);
        }
        Some(self.sum_start(node, (arguments, types), primitive, element))
    }

    /// The type of `sum(values, start)` whose elements are the number `primitive`, of type
    /// `element`: the common type of the elements and `start`, which is given second or as
    /// `start=`, as in Python. Without a start, the element type.
    fn sum_start(
        &mut self,
        node: NodeId,
        (arguments, types): (&Arguments, &[Type<'db>]),
        primitive: types::Primitive,
        element: Type<'db>,
    ) -> Type<'db> {
        for &(name, _) in &arguments.keywords {
            if self.tree.text(name, self.text) != "start" {
                let message = format!(
                    "`sum` has no parameter `{}`",
                    self.tree.text(name, self.text)
                );
                self.lowerer.error(
                    name,
                    &message,
                    "unknown parameter",
                    Some("`sum` takes the values, then `start=`"),
                );
            }
        }
        let starts = &types[1..];
        let [start] = starts else {
            if starts.is_empty() {
                return element;
            }
            return self.lowerer.error(
                node,
                "`sum` takes one sequence and at most one start",
                "wrong number of arguments",
                Some("write `sum(values)` or `sum(values, start=1.0)`"),
            );
        };
        let at = arguments
            .positional
            .get(1)
            .copied()
            .or_else(|| arguments.keywords.first().map(|&(_, value)| value))
            .unwrap_or(node);
        match start.kind(self.db) {
            TypeKind::Primitive(other) if numbers::is_number(*other) => {
                match numbers::common(primitive, *other) {
                    Some(common) => Type::new(self.db, TypeKind::Primitive(common)),
                    None => self.mixing_error(at, "sum", primitive, *other),
                }
            }
            _ if relations::is_open(self.db, *start) => self.lowerer.unknown(),
            _ => {
                let message = format!("`sum` starts from a number, found `{}`", self.show(*start));
                self.lowerer.error(at, &message, "not a number", None)
            }
        }
    }

    /// Reports `sum(values)` of the sequence `sequence`, of type `ty`, whose elements are not
    /// numbers.
    fn not_numbers(&mut self, sequence: NodeId, ty: Type<'db>) -> Type<'db> {
        let message = format!(
            "`sum` of `{}`, whose elements are not numbers",
            self.show(ty)
        );
        self.lowerer.error(sequence, &message, "not numbers", None)
    }
}
