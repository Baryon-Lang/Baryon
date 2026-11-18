//! Joining two frames (decision 0045, roadmap 3.13.8): `left.merge(right, on="symbol")`.
//!
//! The result has one row for each pair of a row of `left` and a row of `right` whose key
//! columns hold the same values: an inner join. `how="left"`, `"right"` and `"outer"` also
//! keep the rows of one frame or both that have no partner, and the other frame's columns
//! then may hold `None`. Its columns are those of `left`, then those
//! of `right` that are not keys; two columns left with one name are told apart as pandas
//! tells them, with `_x` for the left one and `_y` for the right one. The keys are string
//! literals, checked against both frames when compiling, and a key must have one type in
//! both.

use super::Body;
use super::calls::Arguments;
use syntax::{NodeId, SyntaxKind};
use types::{Column, Primitive, Type, TypeKind};

/// The join a `merge` makes, which `how=` names.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Join {
    /// Rows with a partner only.
    Inner,
    /// Every row of the left frame, so the right frame's columns may hold `None`.
    Left,
    /// Every row of the right frame, so the left frame's columns may hold `None`.
    Right,
    /// Every row of both frames.
    Outer,
}

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
        let kind = self.join_kind(how);
        let mut complete = kind.is_some();
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
        let kind = kind.unwrap_or(Join::Inner);
        Some(self.joined_columns(node, [left, found], &keys, kind))
    }

    /// The type of the frame a join of kind `kind` of frames with the columns `left` and
    /// `right`, the two of `sides`, on `keys` gives, for the call `node`: the columns of
    /// `left`, then those of `right` that are no keys. As in pandas, a column the two frames
    /// share that is no key gets `_x` on the left and `_y` on the right; two columns still
    /// left with one name are an error. A column that is no key, of a frame that may have no
    /// row for a row of the result, may hold `None` there (decision 0045): the right frame's
    /// in a left join, the left frame's in a right join, and both in an outer join.
    fn joined_columns(
        &mut self,
        node: NodeId,
        [left, right]: [&[Column<'db>]; 2],
        keys: &[String],
        kind: Join,
    ) -> Type<'db> {
        let shared = |name: &String, others: &[Column<'db>]| {
            !keys.contains(name) && others.iter().any(|column| column.name == *name)
        };
        let gaps = |side: Join, column: &Column<'db>| {
            let optional = matches!(column.element.kind(self.db), TypeKind::Optional(_));
            if optional || keys.contains(&column.name) || !(kind == side || kind == Join::Outer) {
                column.element
            } else {
                Type::new(self.db, TypeKind::Optional(column.element))
            }
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
                // A right join may find no left row.
                element: gaps(Join::Right, column),
            });
        }
        for column in right.iter().filter(|column| !keys.contains(&column.name)) {
            let suffix = if shared(&column.name, left) { "_y" } else { "" };
            columns.push(Column {
                name: format!("{}{suffix}", column.name),
                // A left join may find no right row.
                element: gaps(Join::Left, column),
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

    /// The join `how`, the `how=` of a `merge`, asks for: the inner join when it is left
    /// out. `None` after a report when it is not one of the four names, written as a string
    /// literal, since the result's type depends on it.
    fn join_kind(&mut self, how: Option<NodeId>) -> Option<Join> {
        let Some(how) = how else {
            return Some(Join::Inner);
        };
        match self.literal_text(how) {
            Some("inner") => return Some(Join::Inner),
            Some("left") => return Some(Join::Left),
            Some("right") => return Some(Join::Right),
            Some("outer") => return Some(Join::Outer),
            _ => {}
        }
        self.expression(how, None);
        self.lowerer.error(
            how,
            "`how` is `\"inner\"`, `\"left\"`, `\"right\"` or `\"outer\"`, written as a string \
             literal",
            "not a join",
            Some("the result's type depends on it, so it must be known when compiling"),
        );
        None
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
