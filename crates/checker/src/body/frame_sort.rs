//! Sorting the rows of a frame (decision 0045, roadmap 3.13.5): `df.sort_values("price")`
//! and `df.sort_values(["symbol", "price"], ascending=[True, False])`.
//!
//! The result has the frame's type: the same columns, with the rows in another order. The
//! columns to sort by are string literals, checked against the frame's columns when
//! compiling, and `ascending` is one `bool` for every column or a list with one for each.

use super::calls::Arguments;
use super::{Body, relations};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// The call `node` of `sort_values` on a frame of type `base`, with `callee` the method's
    /// name: the frame's own type, after checking the columns named and `ascending`. `None`,
    /// with nothing checked, for any other method.
    pub(super) fn sort_method(
        &mut self,
        node: NodeId,
        base: Type<'db>,
        callee: &str,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        if callee != "sort_values" {
            return None;
        }
        let name = |body: &Self, keyword: NodeId| body.tree.text(keyword, body.text).to_owned();
        let (by, ascending) = match (
            arguments.positional.as_slice(),
            arguments.keywords.as_slice(),
        ) {
            (&[by], []) => (by, None),
            (&[by], &[(keyword, ascending)]) if name(self, keyword) == "ascending" => {
                (by, Some(ascending))
            }
            ([], &[(keyword, by)]) if name(self, keyword) == "by" => (by, None),
            ([], &[(first, by), (second, ascending)])
                if name(self, first) == "by" && name(self, second) == "ascending" =>
            {
                (by, Some(ascending))
            }
            _ => {
                self.untyped(arguments);
                return Some(self.lowerer.error(
                    node,
                    "`sort_values` takes the columns to sort by, then `ascending=`",
                    "wrong arguments",
                    Some(
                        "for example `df.sort_values(\"price\")` or \
                         `df.sort_values([\"symbol\", \"price\"], ascending=[True, False])`; \
                         the sort is always stable and missing values always come last",
                    ),
                ));
            }
        };
        let keys = if self.tree.kind(by) == SyntaxKind::List {
            self.column_names(by, base)
        } else {
            self.column_name(by, base).map(|name| vec![name])
        };
        let mut complete = keys.is_some();
        let keys = keys.unwrap_or_default();
        if complete && keys.is_empty() {
            self.lowerer.error(
                by,
                "`sort_values` needs at least one column to sort by",
                "no columns",
                None,
            );
            complete = false;
        }
        complete &= self.sortable(by, base, &keys);
        if let Some(ascending) = ascending {
            complete &= self.sort_directions(ascending, keys.len(), complete);
        }
        Some(if complete {
            base
        } else {
            self.lowerer.unknown()
        })
    }

    /// Whether every column of `keys`, columns of the frame of type `base`, has an order: a
    /// complex column has none, and is reported at `by`. A column that may hold `None` is
    /// sorted by its values, with each `None` last.
    fn sortable(&mut self, by: NodeId, base: Type<'db>, keys: &[String]) -> bool {
        let TypeKind::DataFrame { columns, .. } = base.kind(self.db) else {
            return false;
        };
        let mut sortable = true;
        for column in columns.iter().filter(|column| keys.contains(&column.name)) {
            let element = match column.element.kind(self.db) {
                &TypeKind::Optional(element) => element,
                _ => column.element,
            };
            if matches!(
                element.kind(self.db),
                TypeKind::Primitive(Primitive::Complex(_))
            ) {
                let message = format!(
                    "the column `{}` holds `{}`, which has no order to sort by",
                    column.name,
                    self.show(column.element)
                );
                self.lowerer.error(
                    by,
                    &message,
                    "complex numbers",
                    Some("sort by a column of real numbers worked out from it, such as each value's size"),
                );
                sortable = false;
            }
        }
        sortable
    }

    /// Checks `ascending`, the directions of a sort by `keys` columns: one `bool`, or a list
    /// written out with one `bool` for each column. The count is compared only when
    /// `counted`, that is when the columns were named without a mistake. Whether it is right.
    fn sort_directions(&mut self, ascending: NodeId, keys: usize, counted: bool) -> bool {
        let boolean = Type::new(self.db, TypeKind::Primitive(Primitive::Bool));
        let items: Vec<NodeId> = if self.tree.kind(ascending) == SyntaxKind::List {
            self.tree.children_with(ascending, Role::Item).collect()
        } else {
            vec![ascending]
        };
        let mut right = true;
        for &item in &items {
            let found = self.expression(item, Some(boolean));
            if relations::is_open(self.db, found) {
                right = false;
            } else if !matches!(found.kind(self.db), TypeKind::Primitive(Primitive::Bool)) {
                let message = format!(
                    "`ascending` is `True` or `False`, found `{}`",
                    self.show(found)
                );
                self.lowerer.error(item, &message, "not a `bool`", None);
                right = false;
            }
        }
        if self.tree.kind(ascending) == SyntaxKind::List && counted && items.len() != keys {
            let message = format!(
                "`ascending` lists {} for {keys}",
                count(items.len(), "direction"),
                keys = count(keys, "column")
            );
            self.lowerer.error(
                ascending,
                &message,
                "one for each column",
                Some("give one `bool` for every column, or one for each column sorted by"),
            );
            right = false;
        }
        right
    }
}

/// `number` things named `noun`, in words a message can use: "1 column", "2 columns".
fn count(number: usize, noun: &str) -> String {
    let plural = if number == 1 { "" } else { "s" };
    format!("{number} {noun}{plural}")
}
