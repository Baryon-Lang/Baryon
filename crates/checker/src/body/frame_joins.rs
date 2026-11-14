//! Joining two frames (decision 0045, roadmap 3.13.8): `left.merge(right, on="symbol")`.
//!
//! The result has one row for each pair of a row of `left` and a row of `right` whose key
//! columns hold the same values: an inner join. Its columns are those of `left`, then those
//! of `right` that are not keys; two columns left with one name are told apart as pandas
//! tells them, with `_x` for the left one and `_y` for the right one. The keys are string
//! literals, checked against both frames when compiling, and a key must have one type in
//! both.

use super::Body;
use super::calls::Arguments;
use syntax::{NodeId, SyntaxKind};
use types::{Column, Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// The call `node` of `merge` on a frame of type `base`, with `callee` the method's name:
    /// the joined frame's type. `None`, with nothing checked, for any other method.
    pub(super) fn merge_method(
        &mut self,
        node: NodeId,
        base: Type<'db>,
        callee: &str,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        if callee != "merge" {
            return None;
        }
        let named = |body: &Self, name: &str| {
            arguments
                .keywords
                .iter()
                .find(|&&(keyword, _)| body.tree.text(keyword, body.text) == name)
                .map(|&(_, value)| value)
        };
        let (on, how) = (named(self, "on"), named(self, "how"));
        let known = usize::from(on.is_some()) + usize::from(how.is_some());
        let (&[other], Some(on), true) = (
            arguments.positional.as_slice(),
            on,
            known == arguments.keywords.len(),
        ) else {
            self.untyped(arguments);
            return Some(self.lowerer.error(
                node,
                "`merge` takes the other frame, then the columns to join on as `on=`",
                "wrong arguments",
                Some(
                    "for example `trades.merge(names, on=\"symbol\")` or \
                     `on=[\"symbol\", \"day\"]`; the key columns have one name in both frames",
                ),
            ));
        };
        let right = self.expression(other, None);
        if self.has_missing_column(right) {
            return Some(self.missing_column_unbuilt(other, "`merge`"));
        }
        let (TypeKind::DataFrame { columns: left, .. }, TypeKind::DataFrame { columns: found, .. }) =
            (base.kind(self.db), right.kind(self.db))
        else {
            let message = format!(
                "`merge` joins two `DataFrame`s, found `{}`",
                self.show(right)
            );
            self.lowerer
                .error(other, &message, "not a `DataFrame`", None);
            return Some(self.lowerer.unknown());
        };
        let mut complete = self.inner_join(how);
        let keys = if self.tree.kind(on) == SyntaxKind::List {
            self.column_names(on, base)
        } else {
            self.column_name(on, base).map(|name| vec![name])
        };
        complete &= keys.is_some();
        let keys = keys.unwrap_or_default();
        if complete && keys.is_empty() {
            self.lowerer.error(
                on,
                "`merge` needs at least one column to join on",
                "no columns",
                None,
            );
            complete = false;
        }
        for key in left.iter().filter(|column| keys.contains(&column.name)) {
            complete &= self.joins(on, key, right);
        }
        if !complete {
            return Some(self.lowerer.unknown());
        }
        Some(self.joined_columns(node, left, found, &keys))
    }

    /// The type of the frame an inner join of frames with the columns `left` and `right` on
    /// `keys` gives, for the call `node`: the columns of `left`, then those of `right` that
    /// are no keys. As in pandas, a column the two frames share that is no key gets `_x` on
    /// the left and `_y` on the right; two columns still left with one name are an error.
    fn joined_columns(
        &mut self,
        node: NodeId,
        left: &[Column<'db>],
        right: &[Column<'db>],
        keys: &[String],
    ) -> Type<'db> {
        let shared = |name: &String, others: &[Column<'db>]| {
            !keys.contains(name) && others.iter().any(|column| column.name == *name)
        };
        let mut columns: Vec<Column<'db>> = Vec::with_capacity(left.len() + right.len());
        for column in left {
            let suffix = if shared(&column.name, right) {
                "_x"
            } else {
                ""
            };
            columns.push(Column {
                name: format!("{}{suffix}", column.name),
                element: column.element,
            });
        }
        for column in right.iter().filter(|column| !keys.contains(&column.name)) {
            let suffix = if shared(&column.name, left) { "_y" } else { "" };
            columns.push(Column {
                name: format!("{}{suffix}", column.name),
                element: column.element,
            });
        }
        for (position, column) in columns.iter().enumerate() {
            if columns[..position]
                .iter()
                .any(|earlier| earlier.name == column.name)
            {
                let message = format!(
                    "the joined frame would have two columns named `{}`",
                    column.name
                );
                return self.lowerer.error(
                    node,
                    &message,
                    "one name for two columns",
                    Some("rename one of them first, with `rename(columns={...})`"),
                );
            }
        }
        Type::new(self.db, TypeKind::DataFrame { columns, row: None })
    }

    /// Whether `how`, the `how=` of a `merge`, asks for the inner join, which it does when
    /// left out. The other joins are reported as not built yet.
    fn inner_join(&mut self, how: Option<NodeId>) -> bool {
        let Some(how) = how else {
            return true;
        };
        let written = self.literal_text(how).map(str::to_owned);
        if written.as_deref() == Some("inner") {
            return true;
        }
        self.expression(how, None);
        let (message, hint) = match written.as_deref() {
            Some("left" | "right" | "outer") => (
                "`merge` joins only with `how=\"inner\"` yet",
                "a left, right or outer join gives rows with missing values, which come with \
                 roadmap item 3.13.6",
            ),
            _ => (
                "`how` is `\"inner\"`, written as a string literal",
                "the result's type depends on it, so it must be known when compiling",
            ),
        };
        self.lowerer
            .error(how, message, "not an inner join", Some(hint));
        false
    }

    /// Whether the key column `key` of the left frame can be joined with the frame of type
    /// `right`: `right` has a column of that name and of the same type, and the type has an
    /// order. A mistake is reported at `on`.
    fn joins(&mut self, on: NodeId, key: &Column<'db>, right: Type<'db>) -> bool {
        let TypeKind::DataFrame { columns, .. } = right.kind(self.db) else {
            return false;
        };
        let Some(other) = columns.iter().find(|column| column.name == key.name) else {
            self.no_column(on, right, &key.name);
            return false;
        };
        if other.element != key.element {
            let message = format!(
                "the column `{}` is `{}` in one frame and `{}` in the other",
                key.name,
                self.show(key.element),
                self.show(other.element)
            );
            self.lowerer.error(
                on,
                &message,
                "two types for one key",
                Some("a key has one type in both frames; convert one column first, with `assign`"),
            );
            return false;
        }
        if matches!(
            key.element.kind(self.db),
            TypeKind::Primitive(Primitive::Complex(_))
        ) {
            let message = format!(
                "the column `{}` holds `{}`, which `merge` cannot join on",
                key.name,
                self.show(key.element)
            );
            self.lowerer.error(on, &message, "complex numbers", None);
            return false;
        }
        true
    }
}
