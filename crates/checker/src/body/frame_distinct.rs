//! Leaving out the rows of a frame that repeat, and counting them (decision 0045, roadmap
//! 3.13.9): `df.drop_duplicates()`, `df.drop_duplicates(subset=["symbol", "day"])` and
//! `df.value_counts("symbol")`.
//!
//! `drop_duplicates` gives the frame's type: the same columns, with fewer rows.
//! `value_counts` gives a frame of the columns compared, in the order named, and `count`, an
//! `i64`. The columns compared are every column, or the ones `subset` names as string
//! literals, checked against the frame's columns when compiling.

use super::Body;
use super::calls::Arguments;
use syntax::{NodeId, SyntaxKind};
use types::{Column, Integer, Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// The call `node` of `drop_duplicates` or `value_counts` on a frame of type `base`, with
    /// `callee` the method's name: the frame's own type, or the frame of counts, after
    /// checking the columns `subset` names. `None`, with nothing checked, for any other
    /// method.
    pub(super) fn distinct_method(
        &mut self,
        node: NodeId,
        base: Type<'db>,
        callee: &str,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        let counting = callee == "value_counts";
        if callee != "drop_duplicates" && !counting {
            return None;
        }
        let subset = match (
            arguments.positional.as_slice(),
            arguments.keywords.as_slice(),
        ) {
            ([], []) => None,
            (&[subset], []) => Some(subset),
            ([], &[(keyword, subset)]) if self.tree.text(keyword, self.text) == "subset" => {
                Some(subset)
            }
            _ => {
                self.untyped(arguments);
                let message =
                    format!("`{callee}` takes `subset=`, the columns to compare, or nothing");
                let hint = if counting {
                    "for example `df.value_counts(\"symbol\")` or \
                     `df.value_counts([\"symbol\", \"day\"])`"
                } else {
                    "for example `df.drop_duplicates()` or \
                     `df.drop_duplicates(subset=[\"symbol\", \"day\"])`; the first of the rows \
                     that repeat is the one kept"
                };
                return Some(
                    self.lowerer
                        .error(node, &message, "wrong arguments", Some(hint)),
                );
            }
        };
        let keys = match subset {
            None => None,
            Some(subset) if self.tree.kind(subset) == SyntaxKind::List => {
                Some((subset, self.column_names(subset, base)))
            }
            Some(subset) => Some((
                subset,
                self.column_name(subset, base).map(|name| vec![name]),
            )),
        };
        let mut complete = true;
        if let Some((subset, names)) = &keys {
            complete = names.is_some();
            if names.as_ref().is_some_and(Vec::is_empty) {
                self.lowerer.error(
                    *subset,
                    "`subset` needs at least one column to compare",
                    "no columns",
                    Some("leave `subset` out to compare every column"),
                );
                complete = false;
            }
        }
        let names = keys.as_ref().and_then(|(_, names)| names.as_deref());
        let at = keys.as_ref().map_or(node, |(subset, _)| *subset);
        complete &= self.distinct_columns(at, (callee, base), names);
        Some(match (complete, counting) {
            (false, _) => self.lowerer.unknown(),
            (true, false) => base,
            (true, true) => self.counts_frame(at, base, names),
        })
    }

    /// The type `value_counts` gives for a frame of type `base` counted by the columns
    /// `names`, in that order, or by all of them: those columns, then `count`, an `i64`. A
    /// column counted that is itself called `count` is an error, reported at `at`, since the
    /// result would have the name twice.
    fn counts_frame(&mut self, at: NodeId, base: Type<'db>, names: Option<&[String]>) -> Type<'db> {
        let TypeKind::DataFrame { columns, .. } = base.kind(self.db) else {
            return self.lowerer.unknown();
        };
        let mut result: Vec<Column<'db>> = match names {
            Some(names) => names
                .iter()
                .filter_map(|name| columns.iter().find(|column| column.name == *name))
                .cloned()
                .collect(),
            None => columns.clone(),
        };
        if result.iter().any(|column| column.name == "count") {
            return self.lowerer.error(
                at,
                "`value_counts` cannot count by a column named `count`",
                "the result would have two columns named `count`",
                Some("rename it first: `df.rename(columns={\"count\": \"n\"})`"),
            );
        }
        let int = Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::I64)),
        );
        result.push(Column {
            name: "count".to_owned(),
            element: int,
        });
        self.frame_of_columns(result)
    }

    /// Whether the columns of the frame of type `base` that `names` lists, or all of them
    /// when it is `None`, can be compared for rows that repeat by the method `callee`: a
    /// complex column cannot yet, and is reported at `at`.
    fn distinct_columns(
        &mut self,
        at: NodeId,
        (callee, base): (&str, Type<'db>),
        names: Option<&[String]>,
    ) -> bool {
        let TypeKind::DataFrame { columns, .. } = base.kind(self.db) else {
            return false;
        };
        let mut comparable = true;
        for column in columns
            .iter()
            .filter(|column| names.is_none_or(|names| names.contains(&column.name)))
        {
            if matches!(
                column.element.kind(self.db),
                TypeKind::Primitive(Primitive::Complex(_))
            ) {
                let message = format!(
                    "`{callee}` cannot compare the column `{}`, which holds `{}`, yet",
                    column.name,
                    self.show(column.element)
                );
                self.lowerer.error(
                    at,
                    &message,
                    "complex numbers",
                    Some("name the columns of real numbers to compare with `subset=`"),
                );
                comparable = false;
            }
        }
        comparable
    }
}
