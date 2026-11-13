//! Leaving out the rows of a frame that repeat (decision 0045, roadmap 3.13.9):
//! `df.drop_duplicates()` and `df.drop_duplicates(subset=["symbol", "day"])`.
//!
//! The result has the frame's type: the same columns, with fewer rows. The columns compared
//! are every column, or the ones `subset` names as string literals, checked against the
//! frame's columns when compiling.

use super::Body;
use super::calls::Arguments;
use syntax::{NodeId, SyntaxKind};
use types::{Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// The call `node` of `drop_duplicates` on a frame of type `base`, with `callee` the
    /// method's name: the frame's own type, after checking the columns `subset` names.
    /// `None`, with nothing checked, for any other method.
    pub(super) fn distinct_method(
        &mut self,
        node: NodeId,
        base: Type<'db>,
        callee: &str,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        if callee != "drop_duplicates" {
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
                return Some(self.lowerer.error(
                    node,
                    "`drop_duplicates` takes `subset=`, the columns to compare, or nothing",
                    "wrong arguments",
                    Some(
                        "for example `df.drop_duplicates()` or \
                         `df.drop_duplicates(subset=[\"symbol\", \"day\"])`; the first of the \
                         rows that repeat is the one kept",
                    ),
                ));
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
        complete &= self.distinct_columns(at, base, names);
        Some(if complete {
            base
        } else {
            self.lowerer.unknown()
        })
    }

    /// Whether the columns of the frame of type `base` that `names` lists, or all of them
    /// when it is `None`, can be compared for rows that repeat: a complex column cannot
    /// yet, and is reported at `at`.
    fn distinct_columns(&mut self, at: NodeId, base: Type<'db>, names: Option<&[String]>) -> bool {
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
                    "`drop_duplicates` cannot compare the column `{}`, which holds `{}`, yet",
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
