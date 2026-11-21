//! The other forms of `agg` on a grouping (decision 0045's amendment of 2026-11-19, point 4;
//! roadmap 3.13.24): `df.groupby("k").agg({"v": "sum", "n": "max"})`, which reduces each
//! column named its own way and keeps the columns' names, and `df.groupby("k").agg("sum")`,
//! which is `df.groupby("k").sum()`.
//!
//! Both are checked with the rules of the reductions written as methods
//! ([`frame_group_methods`](super::frame_group_methods)). `agg(["sum", "mean"])` stays a
//! compile error: its result has two levels of column names, which a frame does not have.

use super::Body;
use super::frame_group_methods::{Mode, Text};
use super::frame_groups::REDUCTIONS;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Column, Integer, Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// Adds to `result`, the columns of the frame a grouping of the frame of type `base` by
    /// `keys` gives, the columns that `form`, the one positional argument of `agg`, asks
    /// for. `None` when `form` is neither a dict, a string nor a list, so the caller
    /// reports what `agg` takes; otherwise whether every column was added, false after a
    /// report.
    pub(super) fn agg_form(
        &mut self,
        form: NodeId,
        base: Type<'db>,
        keys: &[String],
        result: &mut Vec<Column<'db>>,
    ) -> Option<bool> {
        let mode = Mode {
            every: false,
            listed: false,
            text: Text::Reduced,
        };
        match self.tree.kind(form) {
            SyntaxKind::Dictionary => {
                let entries: Vec<NodeId> = self.tree.children_with(form, Role::Item).collect();
                let mut complete = !entries.is_empty();
                if entries.is_empty() {
                    self.lowerer
                        .error(form, "`agg` needs at least one column", "no columns", None);
                }
                for entry in entries {
                    complete &= self.agg_entry(entry, base, keys, result, mode);
                }
                Some(complete)
            }
            SyntaxKind::StringLiteral => {
                let Some(how) = self.agg_reduction(form) else {
                    return Some(false);
                };
                if how == "size" {
                    return Some(self.added(result, "size", self.whole(), form));
                }
                let TypeKind::DataFrame { columns, .. } = base.kind(self.db) else {
                    return Some(false);
                };
                let read = columns
                    .iter()
                    .filter(|column| !keys.contains(&column.name))
                    .map(|column| (column.name.clone(), form))
                    .collect();
                let every = Mode {
                    every: true,
                    listed: true,
                    text: Text::Reduced,
                };
                Some(self.reduce_columns(base, keys, result, &how, read, every))
            }
            SyntaxKind::List => {
                self.expression(form, None);
                self.lowerer.error(
                    form,
                    "`agg` does not take a list of reductions",
                    "two levels of column names",
                    Some(
                        "pandas names each new column by a column and a reduction; here, name \
                         each: `.agg(total=(\"volume\", \"sum\"), mean=(\"volume\", \"mean\"))`",
                    ),
                );
                Some(false)
            }
            _ => None,
        }
    }

    /// Adds to `result` the column one entry of the dict of `agg` asks for, `"v": "sum"`:
    /// the column `v` reduced by `sum`, under the name `v`. False after a report.
    fn agg_entry(
        &mut self,
        entry: NodeId,
        base: Type<'db>,
        keys: &[String],
        result: &mut Vec<Column<'db>>,
        mode: Mode,
    ) -> bool {
        let key = self.tree.child(entry, Role::Key);
        let value = self.tree.child(entry, Role::Value);
        let (SyntaxKind::DictionaryEntry, Some(key), Some(value)) =
            (self.tree.kind(entry), key, value)
        else {
            self.lowerer.error(
                entry,
                "the dict of `agg` is written out, each column with its reduction",
                "not a column and a reduction",
                Some("for example `.agg({\"volume\": \"sum\", \"price\": \"mean\"})`"),
            );
            return false;
        };
        let name = self.column_name(key, base);
        let Some(how) = self.agg_reduction(value) else {
            return false;
        };
        let Some(name) = name else {
            return false;
        };
        if how == "size" && !keys.contains(&name) {
            return self.added(result, &name, self.whole(), key);
        }
        self.reduce_columns(base, keys, result, &how, vec![(name, key)], mode)
    }

    /// The reduction that `node` names, a string literal that is one of `agg`'s. `None`
    /// after reporting anything else.
    pub(super) fn agg_reduction(&mut self, node: NodeId) -> Option<String> {
        let Some(how) = self.literal_text(node).map(str::to_owned) else {
            self.expression(node, None);
            let hint = format!("one of `{}`", REDUCTIONS.join("`, `"));
            self.lowerer.error(
                node,
                "a reduction of `agg` is written as a string literal",
                "not a string literal",
                Some(&hint),
            );
            return None;
        };
        if !REDUCTIONS.contains(&how.as_str()) {
            let message = format!("`agg` has no reduction `{how}`");
            let hint = format!("it has `{}`", REDUCTIONS.join("`, `"));
            self.lowerer
                .error(node, &message, "not a reduction", Some(&hint));
            return None;
        }
        Some(how)
    }

    /// The type `i64`, which a count is.
    fn whole(&self) -> Type<'db> {
        Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::I64)),
        )
    }
}
