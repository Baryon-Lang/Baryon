//! Leaving out the rows of a frame that repeat, and counting them (decision 0045, roadmap
//! 3.13.9): `df.drop_duplicates()`, `df.drop_duplicates(subset=["symbol", "day"])` and
//! `df.value_counts("symbol")`. Their other arguments are checked in the
//! `frame_distinct_options` module.
//!
//! `drop_duplicates` gives the frame's type: the same columns, with fewer rows.
//! `value_counts` gives a frame of the columns compared, in the order named, and `count`, an
//! `i64`, or with `normalize=True`, `proportion`, an `f64`. The columns compared are every
//! column, or the ones `subset` names as string literals, checked against the frame's
//! columns when compiling.

use super::Body;
use super::calls::Arguments;
use syntax::{NodeId, SyntaxKind};
use types::{Column, Float, Integer, Primitive, Type, TypeKind};

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
        let Some((subset, options)) = self.distinct_arguments(callee, arguments) else {
            self.untyped(arguments);
            let (message, hint) = if counting {
                (
                    "`value_counts` takes `subset=`, the columns to count by, then \
                     `normalize=`, `sort=`, `ascending=` and `dropna=`",
                    "for example `df.value_counts(\"symbol\")` or \
                     `df.value_counts([\"symbol\", \"day\"], normalize=True)`",
                )
            } else {
                (
                    "`drop_duplicates` takes `subset=`, the columns to compare, and `keep=`",
                    "for example `df.drop_duplicates()` or \
                     `df.drop_duplicates(subset=[\"symbol\", \"day\"], keep=\"last\")`; the \
                     first of the rows that repeat is kept unless `keep=` says otherwise",
                )
            };
            return Some(
                self.lowerer
                    .error(node, message, "wrong arguments", Some(hint)),
            );
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
        let mut complete = options.is_some();
        if let Some((subset, names)) = &keys {
            complete &= names.is_some();
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
            (true, true) => self.counts_frame(at, base, names, options == Some(true)),
        })
    }

    /// The type `value_counts` gives for a frame of type `base` counted by the columns
    /// `names`, in that order, or by all of them: those columns, then `count`, an `i64`, or
    /// when `normalize`, `proportion`, an `f64`, as pandas names it. A column counted that
    /// itself has the last column's name is an error, reported at `at`, since the result
    /// would have the name twice.
    fn counts_frame(
        &mut self,
        at: NodeId,
        base: Type<'db>,
        names: Option<&[String]>,
        normalize: bool,
    ) -> Type<'db> {
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
        let (name, element) = if normalize {
            (
                "proportion",
                TypeKind::Primitive(Primitive::Float(Float::F64)),
            )
        } else {
            (
                "count",
                TypeKind::Primitive(Primitive::Integer(Integer::I64)),
            )
        };
        if result.iter().any(|column| column.name == name) {
            let message = format!("`value_counts` cannot count by a column named `{name}`");
            let label = format!("the result would have two columns named `{name}`");
            let hint = format!("rename it first: `df.rename(columns={{\"{name}\": \"n\"}})`");
            return self.lowerer.error(at, &message, &label, Some(&hint));
        }
        result.push(Column {
            name: name.to_owned(),
            element: Type::new(self.db, element),
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
