//! Grouping the rows of a frame (decision 0045, roadmap 3.13.7):
//! `df.groupby("symbol").agg(total=("volume", "sum"))`.
//!
//! `groupby` and `agg` are checked together, as one expression: the rows with one value of
//! the key columns form a group, and each argument of `agg` names a new column, the column
//! it reads and how that column's values in a group become one. The result is a frame of the
//! key columns followed by the new ones, with one row for each group. Every name is a
//! string literal, so the result's type is known when compiling. A column of `str` has the
//! reductions that need no arithmetic: `count`, `first`, `last`, `min` and `max`.
//!
//! The reductions written as methods, `df.groupby("symbol").sum()`, are the
//! [`frame_group_methods`](super::frame_group_methods) module's, which shares the rules here.
//! `agg` with a dict or with one name is the [`frame_agg_forms`](super::frame_agg_forms)
//! module's.

use super::Body;
use super::calls::Arguments;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Call, Callee, Column, Float, Integer, Key, Primitive, Type, TypeKind};

/// The ways `agg` makes one value of a group's values, as pandas names them.
pub(super) const REDUCTIONS: [&str; 12] = [
    "sum", "min", "max", "first", "last", "count", "mean", "var", "std", "median", "size",
    "nunique",
];

impl<'db> Body<'db> {
    /// The frame `df` when `function`, the callee of a call, is `df.groupby(...).agg`, read
    /// from the syntax alone. `None` for any other callee.
    pub(super) fn grouped_frame(&self, function: NodeId) -> Option<NodeId> {
        let named = |node: NodeId, name: &str| {
            self.tree.kind(node) == SyntaxKind::Attribute
                && self
                    .tree
                    .child(node, Role::Attribute)
                    .is_some_and(|attribute| self.tree.text(attribute, self.text) == name)
        };
        let grouping = self.tree.child(function, Role::Value)?;
        let groupby = self.tree.child(grouping, Role::Function)?;
        (named(function, "agg")
            && self.tree.kind(grouping) == SyntaxKind::Call
            && named(groupby, "groupby"))
        .then(|| self.tree.child(groupby, Role::Value))?
    }

    /// The call `node`, `frame.groupby(...).agg(...)` with `arguments` those of `agg`: the
    /// frame of one row for each group. `None`, with only `frame` typed, when `frame` is not
    /// a `DataFrame`, so the call is some other type's methods.
    pub(super) fn frame_agg(
        &mut self,
        node: NodeId,
        frame: NodeId,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        let base = self.expression(frame, None);
        let TypeKind::DataFrame { .. } = base.kind(self.db) else {
            return None;
        };
        let grouping = self
            .tree
            .child(node, Role::Function)
            .and_then(|function| self.tree.child(function, Role::Value));
        let Some((keys, mut result, mut complete)) = self.grouping_keys(node, grouping, base)
        else {
            self.untyped(arguments);
            return Some(self.lowerer.unknown());
        };
        let form = match (
            arguments.positional.as_slice(),
            arguments.keywords.is_empty(),
        ) {
            (&[form], true) => self.agg_form(form, base, &keys, &mut result),
            _ => None,
        };
        if let Some(added) = form {
            complete &= added;
        } else if !arguments.positional.is_empty() || arguments.keywords.is_empty() {
            self.untyped(arguments);
            self.lowerer.error(
                node,
                "`agg` takes each new column by name, as a pair of a column and a reduction",
                "wrong arguments",
                Some(
                    "for example `.agg(total=(\"volume\", \"sum\"), \
                     mean_price=(\"price\", \"mean\"))`",
                ),
            );
            return Some(self.lowerer.unknown());
        }
        for &(name, value) in &arguments.keywords {
            let name = self.tree.text(name, self.text).to_owned();
            let Some(element) = self.group_reduction(value, base, &keys) else {
                complete = false;
                continue;
            };
            if result.iter().any(|column| column.name == name) {
                let message = format!("the result would have two columns named `{name}`");
                self.lowerer
                    .error(value, &message, "one name for two columns", None);
                complete = false;
                continue;
            }
            result.push(Column { name, element });
        }
        if !complete {
            return Some(self.lowerer.unknown());
        }
        let call = Call {
            callee: Callee::Builtin("DataFrame.agg".to_owned()),
            type_arguments: Vec::new(),
            arguments: vec![Some(Key(frame))],
        };
        self.typed.calls.insert(Key(node), call);
        Some(self.frame_of_columns(result))
    }

    /// The key columns of the grouping `grouping`, the call `frame.groupby(...)`, on a frame
    /// of type `base`: their names as written, the columns in that order, which start the
    /// result, and whether every one was a column that can be grouped by. `None` after
    /// reporting, at `grouping` or at `node` when there is none, that `groupby` was not
    /// given its columns.
    pub(super) fn grouping_keys(
        &mut self,
        node: NodeId,
        grouping: Option<NodeId>,
        base: Type<'db>,
    ) -> Option<(Vec<String>, Vec<Column<'db>>, bool)> {
        let TypeKind::DataFrame { columns, .. } = base.kind(self.db) else {
            return None;
        };
        let written = grouping.map(|grouping| self.arguments(grouping));
        let by = match written.as_ref().map(|written| self.grouping_by(written)) {
            Some(Ok(by)) => by,
            // A keyword's own mistake is already reported.
            Some(Err(())) => return None,
            None => None,
        };
        let Some(by) = by else {
            self.lowerer.error(
                grouping.unwrap_or(node),
                "`groupby` takes the columns whose values make the groups",
                "wrong arguments",
                Some("for example `df.groupby(\"symbol\")` or `df.groupby([\"symbol\", \"day\"])`"),
            );
            return None;
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
                "`groupby` needs at least one column",
                "no columns",
                None,
            );
            complete = false;
        }
        let mut result: Vec<Column<'db>> = columns
            .iter()
            .filter(|column| keys.contains(&column.name))
            .cloned()
            .collect();
        result.sort_by_key(|column| keys.iter().position(|key| *key == column.name));
        for column in &result {
            complete &= self.orderable(by, column, "group by");
        }
        Some((keys, result, complete))
    }

    /// The argument of `groupby` that names the key columns, from `written`, its arguments:
    /// the one positional argument or `by=`. `as_index=False` and `sort=`, each `True` or
    /// `False` written out, may follow (decision 0045's amendment, point 3); they change
    /// no type, and lowering reads `sort` from the syntax. `Ok(None)`, with every argument
    /// marked untyped, when the arguments are not these, and `Err` after reporting
    /// `as_index=True`, which asks for an index a frame does not have, or a flag that is
    /// not written out, or `dropna=`, which is not built yet.
    fn grouping_by(&mut self, written: &Arguments) -> Result<Option<NodeId>, ()> {
        let mut by = match written.positional.as_slice() {
            [] => None,
            &[by] => Some(by),
            _ => {
                self.untyped(written);
                return Ok(None);
            }
        };
        let mut seen: Vec<&str> = Vec::new();
        for &(name, value) in &written.keywords {
            let keyword = self.tree.text(name, self.text);
            let flag = match self.tree.text(value, self.text) {
                "True" => Some(true),
                "False" => Some(false),
                _ => None,
            };
            if keyword == "dropna" {
                self.untyped(written);
                self.lowerer.error(
                    name,
                    "`groupby` does not take `dropna=` yet",
                    "not built yet",
                    Some("a row whose key is missing is in no group"),
                );
                return Err(());
            }
            let known = match keyword {
                "by" if by.is_none() => {
                    by = Some(value);
                    true
                }
                "as_index" | "sort" => !seen.contains(&keyword),
                _ => false,
            };
            if !known {
                self.untyped(written);
                return Ok(None);
            }
            seen.push(keyword);
            if keyword == "by" {
                continue;
            }
            self.expression(value, None);
            if flag.is_none() {
                let message = format!("`{keyword}` of `groupby` is `True` or `False`, written out");
                self.lowerer
                    .error(value, &message, "not `True` or `False`", None);
            } else if keyword == "as_index" && flag == Some(true) {
                self.lowerer.error(
                    value,
                    "a `DataFrame` has no index for `as_index=True` to make of the keys",
                    "no index",
                    Some("leave it out or write `as_index=False`: keys are columns of the result"),
                );
            } else {
                continue;
            }
            self.untyped(written);
            return Err(());
        }
        Ok(by)
    }

    /// The element type of the column one argument of `agg` gives, `value` being
    /// `("column", "reduction")` on a frame of type `base` grouped by `keys`. `None` after an
    /// error.
    fn group_reduction(
        &mut self,
        value: NodeId,
        base: Type<'db>,
        keys: &[String],
    ) -> Option<Type<'db>> {
        let items: Vec<NodeId> = self.tree.children_with(value, Role::Item).collect();
        let (SyntaxKind::Tuple, &[column, how]) = (self.tree.kind(value), items.as_slice()) else {
            self.expression(value, None);
            self.lowerer.error(
                value,
                "a column of `agg` is a pair: the column to read and how to reduce it",
                "not a pair",
                Some("for example `total=(\"volume\", \"sum\")`"),
            );
            return None;
        };
        let name = self.column_name(column, base);
        let how = self.agg_reduction(how)?;
        let name = name?;
        if keys.contains(&name) {
            let message = format!("`{name}` is a column the rows are grouped by");
            self.lowerer.error(
                column,
                &message,
                "a key, not a value",
                Some("each group has one value of it, which the result already holds"),
            );
            return None;
        }
        let TypeKind::DataFrame { columns, .. } = base.kind(self.db) else {
            return None;
        };
        let stored = columns.iter().find(|read| read.name == name)?;
        self.reduced_element(column, stored, &how)
    }

    /// The element type of the column that reducing `stored`, a column of the frame, the
    /// way `how` names gives for each group. `None` after reporting at `column`, the node
    /// that names it, a column or a reduction that cannot be.
    pub(super) fn reduced_element(
        &mut self,
        column: NodeId,
        stored: &Column<'db>,
        how: &str,
    ) -> Option<Type<'db>> {
        // A column that may hold `None` is reduced by the values that are there, as the
        // column of those values would be. A group may have none: its `sum` and `count`
        // are 0, its mean, spread and median NaN, as for a group of NaN, and the reductions
        // that pick one of the values give `None`.
        let optional = match stored.element.kind(self.db) {
            TypeKind::Optional(element) => Some(*element),
            _ => None,
        };
        let read = &Column {
            name: stored.name.clone(),
            element: optional.unwrap_or(stored.element),
        };
        let whole = Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::I64)),
        );
        // `size` counts a group's rows, whatever the column holds, and `nunique` its
        // different values, `None` and NaN left out, as pandas counts them (roadmap
        // 3.13.17).
        if how == "size" {
            return Some(whole);
        }
        if how == "nunique" {
            return self.orderable(column, read, "count").then_some(whole);
        }
        if matches!(read.element.kind(self.db), TypeKind::Str) {
            let reduced = self.text_reduction(column, read, how)?;
            // The text picked from a group whose texts are all `None` is `None`.
            if optional.is_some() && matches!(reduced.kind(self.db), TypeKind::Str) {
                return Some(stored.element);
            }
            return Some(reduced);
        }
        if !self.orderable(column, read, "reduce") {
            return None;
        }
        if optional.is_some() && matches!(how, "min" | "max" | "first" | "last") {
            return Some(stored.element);
        }
        let primitive = |primitive| Type::new(self.db, TypeKind::Primitive(primitive));
        let single = matches!(
            read.element.kind(self.db),
            TypeKind::Primitive(Primitive::Float(Float::F32))
        );
        let float = matches!(
            read.element.kind(self.db),
            TypeKind::Primitive(Primitive::Float(_))
        );
        Some(match how {
            "count" => primitive(Primitive::Integer(Integer::I64)),
            "sum" if !float => primitive(Primitive::Integer(Integer::I64)),
            "sum" | "min" | "max" | "first" | "last" => read.element,
            _ if single => read.element,
            _ => primitive(Primitive::Float(Float::F64)),
        })
    }

    /// The element type of the column that reducing `read`, a column of `str`, the way `how`
    /// names gives: `i64` for `count`, and `str` for `first`, `last`, `min` and `max`, which
    /// pick one of the group's texts, the smallest and largest by `<`. Text has no sum, mean
    /// or spread here, so the rest are reported at `node`, where the column is named;
    /// pandas's `sum`, which joins the texts, is not built.
    fn text_reduction(&mut self, node: NodeId, read: &Column<'db>, how: &str) -> Option<Type<'db>> {
        match how {
            "count" => Some(Type::new(
                self.db,
                TypeKind::Primitive(Primitive::Integer(Integer::I64)),
            )),
            "first" | "last" | "min" | "max" => Some(read.element),
            _ => {
                let message = format!(
                    "`agg` cannot reduce `{}`, a column of `str`, with `{how}`",
                    read.name
                );
                self.lowerer.error(
                    node,
                    &message,
                    "a column of `str`",
                    Some(
                        "a column of `str` has `count`, `size`, `nunique`, `first`, `last`, \
                         `min` and `max`",
                    ),
                );
                None
            }
        }
    }

    /// Whether `column` holds values with an order, which grouping and reducing need: a
    /// complex column has none, and is reported at `node` as something `agg` cannot `verb`.
    fn orderable(&mut self, node: NodeId, column: &Column<'db>, verb: &str) -> bool {
        let element = match column.element.kind(self.db) {
            TypeKind::Optional(element) => *element,
            _ => column.element,
        };
        if !matches!(
            element.kind(self.db),
            TypeKind::Primitive(Primitive::Complex(_))
        ) {
            return true;
        }
        let message = format!(
            "the column `{}` holds `{}`, which `groupby` cannot {verb}",
            column.name,
            self.show(column.element)
        );
        self.lowerer.error(node, &message, "complex numbers", None);
        false
    }
}
