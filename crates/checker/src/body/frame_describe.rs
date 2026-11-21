//! Summing up the columns of a frame (decision 0045, roadmap 3.13.9): `df.describe()`, and
//! `df.describe(percentiles=[0.1, 0.9])` to choose the percentiles shown in place of the
//! quartiles (roadmap 3.13.16).
//!
//! pandas gives a frame whose row index names eight statistics. A Baryon frame has no row
//! index, so the names are the first column, `statistic`, a column of `str`, as in Polars.
//! After it comes one `f64` column for each column of whole or real numbers, under the
//! column's own name. Columns of `bool`, `str` and complex numbers are left out, as pandas
//! leaves out what is not a number.

use super::calls::Arguments;
use super::{Body, relations};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Column, Float, Primitive, Type, TypeKind};

/// The name of the column that holds the statistics' names.
const STATISTIC: &str = "statistic";

impl<'db> Body<'db> {
    /// The call `node` of `describe` on a frame of type `base`: the frame of `statistic` and
    /// one `f64` column for each column of numbers. Arguments other than `percentiles`, a
    /// frame with no column of numbers, and a column of numbers named `statistic` are errors.
    /// `None`, with nothing checked, for any other method.
    pub(super) fn describe_method(
        &mut self,
        node: NodeId,
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
        let percentiles = match (
            arguments.positional.as_slice(),
            arguments.keywords.as_slice(),
        ) {
            ([], []) => None,
            (&[percentiles], []) => Some(percentiles),
            ([], &[(keyword, percentiles)])
                if self.tree.text(keyword, self.text) == "percentiles" =>
            {
                Some(percentiles)
            }
            _ => {
                self.untyped(arguments);
                return Some(self.lowerer.error(
                    node,
                    "`describe` takes `percentiles=`, or nothing",
                    "wrong arguments",
                    Some(
                        "it describes every column of numbers; choose columns first with \
                         `df[[\"price\", \"volume\"]].describe()`",
                    ),
                ));
            }
        };
        if percentiles.is_some_and(|percentiles| !self.percentiles(percentiles)) {
            return Some(self.lowerer.unknown());
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

    /// Whether `node`, the `percentiles=` of a `describe`, is right: a list written out of
    /// numbers from 0 to 1, none twice, as pandas asks. They are literals so that a mistake
    /// is found when compiling, where pandas raises `ValueError`. The list is then checked
    /// as a `list[f64]`, which lowering passes to the library. A mistake is reported.
    fn percentiles(&mut self, node: NodeId) -> bool {
        let items: Vec<NodeId> = self.tree.children_with(node, Role::Item).collect();
        let number = |body: &Self, item: NodeId| {
            matches!(
                body.tree.kind(item),
                SyntaxKind::Integer | SyntaxKind::Float
            )
            .then(|| {
                body.tree
                    .text(item, body.text)
                    .replace('_', "")
                    .parse::<f64>()
                    .ok()
            })
            .flatten()
        };
        let parts: Vec<Option<f64>> = items.iter().map(|&item| number(self, item)).collect();
        if self.tree.kind(node) != SyntaxKind::List || parts.contains(&None) {
            self.expression(node, None);
            self.lowerer.error(
                node,
                "`percentiles` is a list of numbers from 0 to 1, written out",
                "not a list of numbers",
                Some("for example `df.describe(percentiles=[0.1, 0.5, 0.9])`"),
            );
            return false;
        }
        let parts: Vec<f64> = parts.into_iter().flatten().collect();
        let mut right = true;
        for (index, (&item, &part)) in items.iter().zip(&parts).enumerate() {
            let text = self.tree.text(item, self.text);
            if !(0.0..=1.0).contains(&part) {
                let message = format!("a percentile is from 0 to 1, found `{text}`");
                let hint = "a percentile is a part of 1: the 90th is `0.9`";
                self.lowerer
                    .error(item, &message, "outside 0 to 1", Some(hint));
                right = false;
            } else if parts
                .iter()
                .take(index)
                .any(|earlier| earlier.to_bits() == part.to_bits())
            {
                let message = format!("`percentiles` lists `{text}` twice");
                self.lowerer.error(item, &message, "listed before", None);
                right = false;
            }
        }
        let float = Type::new(self.db, TypeKind::Primitive(Primitive::Float(Float::F64)));
        let list = Type::new(self.db, TypeKind::List(float));
        let found = self.expression(node, Some(list));
        right && !relations::is_open(self.db, found)
    }
}
