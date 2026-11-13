//! Grouping the rows of a frame (decision 0045, roadmap 3.13.7):
//! `df.groupby("symbol").agg(total=("volume", "sum"))`.
//!
//! `groupby` and `agg` are checked together, as one expression: the rows with one value of
//! the key columns form a group, and each argument of `agg` names a new column, the column
//! it reads and how that column's values in a group become one. The result is a frame of the
//! key columns followed by the new ones, with one row for each group. Every name is a
//! string literal, so the result's type is known when compiling. A column of `str` has the
//! reductions that need no arithmetic: `count`, `first`, `last`, `min` and `max`.

use super::Body;
use super::calls::Arguments;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Call, Callee, Column, Float, Integer, Key, Primitive, Type, TypeKind};

/// The ways `agg` makes one value of a group's values, as pandas names them.
const REDUCTIONS: [&str; 10] = [
    "sum", "min", "max", "first", "last", "count", "mean", "var", "std", "median",
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
        let TypeKind::DataFrame { columns, .. } = base.kind(self.db) else {
            return None;
        };
        let grouping = self
            .tree
            .child(node, Role::Function)
            .and_then(|function| self.tree.child(function, Role::Value));
        let keys = match grouping.map(|grouping| self.arguments(grouping)) {
            Some(Arguments {
                positional,
                keywords,
            }) => match (positional.as_slice(), keywords.as_slice()) {
                (&[by], []) => Some(by),
                ([], &[(name, by)]) if self.tree.text(name, self.text) == "by" => Some(by),
                _ => {
                    self.untyped(&Arguments {
                        positional,
                        keywords,
                    });
                    None
                }
            },
            None => None,
        };
        let Some(by) = keys else {
            self.untyped(arguments);
            return Some(self.lowerer.error(
                grouping.unwrap_or(node),
                "`groupby` takes the columns whose values make the groups",
                "wrong arguments",
                Some("for example `df.groupby(\"symbol\")` or `df.groupby([\"symbol\", \"day\"])`"),
            ));
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
        if !arguments.positional.is_empty() || arguments.keywords.is_empty() {
            self.untyped(arguments);
            self.lowerer.error(
                node,
                "`agg` takes each new column by name, as a pair of a column and a reduction",
                "wrong arguments",
                Some("for example `.agg(total=(\"volume\", \"sum\"), mean_price=(\"price\", \"mean\"))`"),
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
        let Some(how) = self.literal_text(how).map(str::to_owned) else {
            self.expression(how, None);
            let hint = format!("one of `{}`", REDUCTIONS.join("`, `"));
            self.lowerer.error(
                how,
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
                .error(items[1], &message, "not a reduction", Some(&hint));
            return None;
        }
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
        let read = columns.iter().find(|read| read.name == name)?;
        if matches!(read.element.kind(self.db), TypeKind::Str) {
            return self.text_reduction(column, read, &how);
        }
        if !self.orderable(column, read, "reduce") {
            return None;
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
        Some(match how.as_str() {
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
                    Some("a column of `str` has `count`, `first`, `last`, `min` and `max`"),
                );
                None
            }
        }
    }

    /// Whether `column` holds values with an order, which grouping and reducing need: a
    /// complex column has none, and is reported at `node` as something `agg` cannot `verb`.
    fn orderable(&mut self, node: NodeId, column: &Column<'db>, verb: &str) -> bool {
        if !matches!(
            column.element.kind(self.db),
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
