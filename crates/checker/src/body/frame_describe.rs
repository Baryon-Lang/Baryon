//! Summing up the columns of a frame (decision 0045, roadmap 3.13.9): `df.describe()`.
//!
//! pandas gives a frame whose row index names eight statistics. A Baryon frame has no row
//! index, so the names are the first column, `statistic`, a column of `str`, as in Polars.
//! After it comes one `f64` column for each column of whole or real numbers, under the
//! column's own name. Columns of `bool`, `str` and complex numbers are left out, as pandas
//! leaves out what is not a number.

use super::Body;
use super::calls::Arguments;
use types::{Column, Float, Primitive, Type, TypeKind};

/// The name of the column that holds the statistics' names.
const STATISTIC: &str = "statistic";

impl<'db> Body<'db> {
    /// The call `node` of `describe` on a frame of type `base`: the frame of `statistic` and
    /// one `f64` column for each column of numbers. Arguments, a frame with no column of
    /// numbers, and a column of numbers named `statistic` are errors. `None`, with nothing
    /// checked, for any other method.
    pub(super) fn describe_method(
        &mut self,
        node: syntax::NodeId,
        base: Type<'db>,
        callee: &str,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        if callee != "describe" {
            return None;
        }
        let TypeKind::DataFrame { columns, .. } = base.kind(self.db) else {
            return None;
        };
        if !arguments.positional.is_empty() || !arguments.keywords.is_empty() {
            self.untyped(arguments);
            return Some(self.lowerer.error(
                node,
                "`describe` takes no arguments",
                "wrong arguments",
                Some(
                    "it describes every column of numbers; choose columns first with \
                     `df[[\"price\", \"volume\"]].describe()`",
                ),
            ));
        }
        let float = Type::new(self.db, TypeKind::Primitive(Primitive::Float(Float::F64)));
        let mut result = vec![Column {
            name: STATISTIC.to_owned(),
            element: Type::new(self.db, TypeKind::Str),
        }];
        for column in columns {
            // A column of numbers that may hold `None` is described by the numbers there.
            let element = match column.element.kind(self.db) {
                TypeKind::Optional(element) => *element,
                _ => column.element,
            };
            if matches!(
                element.kind(self.db),
                TypeKind::Primitive(Primitive::Integer(_) | Primitive::Float(_))
            ) {
                result.push(Column {
                    name: column.name.clone(),
                    element: float,
                });
            }
        }
        if result.len() == 1 {
            let message = format!("`{}` has no column of numbers to describe", self.show(base));
            return Some(self.lowerer.error(
                node,
                &message,
                "no column of numbers",
                Some("`describe` reads columns of whole and real numbers"),
            ));
        }
        if result[1..].iter().any(|column| column.name == STATISTIC) {
            return Some(self.lowerer.error(
                node,
                "`describe` cannot describe a column named `statistic`",
                "the result would have two columns named `statistic`",
                Some("rename it first: `df.rename(columns={\"statistic\": \"value\"})`"),
            ));
        }
        Some(self.frame_of_columns(result))
    }
}
