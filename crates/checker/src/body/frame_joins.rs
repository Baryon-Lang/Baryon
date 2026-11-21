//! Joining two frames (decision 0045, roadmap 3.13.8): `left.merge(right, on="symbol")`.
//!
//! The result has one row for each pair of a row of `left` and a row of `right` whose key
//! columns hold the same values: an inner join. `how="left"`, `"right"` and `"outer"` also
//! keep the rows of one frame or both that have no partner, and the other frame's columns
//! then may hold `None`. Its columns are those of `left`, then those
//! of `right` that are not keys; two columns left with one name are told apart as pandas
//! tells them, with `_x` for the left one and `_y` for the right one, or with the two
//! `suffixes=`. The keys are string literals, checked against both frames when compiling,
//! and a key must have one type in both. `left_on=` and `right_on=` name a key that has two
//! names ([`frame_join_keys`](super::frame_join_keys)); both of its columns are kept.

use super::Body;
use super::calls::Arguments;
use syntax::NodeId;
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
        let how = named(self, "how");
        let written = ["on", "left_on", "right_on", "suffixes"].map(|name| named(self, name));
        let [on, left_on, right_on, suffixes] = written;
        let known = written.iter().flatten().count() + usize::from(how.is_some());
        // The keys are `on=`, or `left_on=` and `right_on=` together, never both forms.
        let keyed = match (on, left_on, right_on) {
            (Some(on), None, None) => Some((on, on)),
            (None, Some(left_on), Some(right_on)) => Some((left_on, right_on)),
            _ => None,
        };
        let (&[other], Some((left_on, right_on)), true) = (
            arguments.positional.as_slice(),
            keyed,
            known == arguments.keywords.len(),
        ) else {
            self.untyped(arguments);
            return Some(self.lowerer.error(
                node,
                "`merge` takes the other frame, then the columns to join on as `on=`",
                "wrong arguments",
                Some(
                    "for example `trades.merge(names, on=\"symbol\")` or \
                     `on=[\"symbol\", \"day\"]`; a key with two names is `left_on=` and \
                     `right_on=` together, in place of `on=`",
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
        let ends = self.join_suffixes(suffixes);
        let keys = self.join_keys([left_on, right_on], [base, right]);
        let (Some(kind), Some(ends), Some(keys)) = (kind, ends, keys) else {
            return Some(self.lowerer.unknown());
        };
        let mut complete = true;
        for (name, other) in &keys {
            if let Some(key) = left.iter().find(|column| column.name == *name) {
                complete &= self.joins(right_on, key, other, right);
            }
        }
        if !complete {
            return Some(self.lowerer.unknown());
        }
        Some(self.joined_columns(node, [left, found], &keys, kind, &ends))
    }

    /// The type of the frame a join of kind `kind` of frames with the columns `left` and
    /// `right`, the two of `sides`, on `keys` gives, for the call `node`: the columns of
    /// `left`, then those of `right`, without a key's right column when the key has one name
    /// in both frames. Each of `keys` is a key's name in the left frame and in the right. As
    /// in pandas, a column name the two frames share, a key with one name aside, gets the
    /// first of `ends` on the left and the second on the right, `_x` and `_y` unless
    /// `suffixes=` says otherwise; two columns still left with one name are an error. A
    /// column of a frame that may have no row for a row of the result may hold `None` there
    /// (decision 0045): the right frame's in a left join, the left frame's in a right join,
    /// and both in an outer join. A key with one name is the exception, since it is read
    /// from whichever frame has the row; a key with two names is two columns like any other.
    fn joined_columns(
        &mut self,
        node: NodeId,
        [left, right]: [&[Column<'db>]; 2],
        keys: &[(String, String)],
        kind: Join,
        ends: &[String; 2],
    ) -> Type<'db> {
        let one = |name: &String| {
            keys.iter()
                .any(|(first, second)| first == name && second == name)
        };
        let shared = |name: &String, others: &[Column<'db>]| {
            !one(name) && others.iter().any(|column| column.name == *name)
        };
        let gaps = |side: Join, column: &Column<'db>| {
            let optional = matches!(column.element.kind(self.db), TypeKind::Optional(_));
            if optional || one(&column.name) || !(kind == side || kind == Join::Outer) {
                column.element
            } else {
                Type::new(self.db, TypeKind::Optional(column.element))
            }
        };
        let mut columns: Vec<Column<'db>> = Vec::with_capacity(left.len() + right.len());
        for column in left {
            let suffix = if shared(&column.name, right) {
                ends[0].as_str()
            } else {
                ""
            };
            columns.push(Column {
                name: format!("{}{suffix}", column.name),
                // A right join may find no left row.
                element: gaps(Join::Right, column),
            });
        }
        for column in right.iter().filter(|column| !one(&column.name)) {
            let suffix = if shared(&column.name, left) {
                ends[1].as_str()
            } else {
                ""
            };
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
                    Some(
                        "give `merge` two different `suffixes=`, or rename one of them first, \
                         with `rename(columns={...})`",
                    ),
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

    /// Whether the key column `key` of the left frame can be joined with the column `other`
    /// of the right frame, of type `right`: there is such a column, of the same type, and
    /// the type has an order. A mistake is reported at `on`.
    fn joins(&mut self, on: NodeId, key: &Column<'db>, other: &str, right: Type<'db>) -> bool {
        let TypeKind::DataFrame { columns, .. } = right.kind(self.db) else {
            return false;
        };
        let Some(other) = columns.iter().find(|column| column.name == other) else {
            self.no_column(on, right, other);
            return false;
        };
        if other.element != key.element {
            let message = if other.name == key.name {
                format!(
                    "the column `{}` is `{}` in one frame and `{}` in the other",
                    key.name,
                    self.show(key.element),
                    self.show(other.element)
                )
            } else {
                format!(
                    "the column `{}` is `{}` and the column `{}` is `{}`",
                    key.name,
                    self.show(key.element),
                    other.name,
                    self.show(other.element)
                )
            };
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
