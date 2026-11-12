//! Choosing and changing the columns of a frame (decision 0045, roadmap 3.13.4):
//! `df[["a", "b"]]`, `df.assign(name=values)`, `df.drop(columns=[...])` and
//! `df.rename(columns={...})`.
//!
//! Each gives a frame with other columns than the one it starts from, so the compiler works
//! out the result's type here: its columns are listed, and no struct names its rows. Every
//! column name is a string literal, checked against the frame's columns when compiling.
//!
//! `df[["a", "b"]]` reads the frame and copies the columns it names. `assign`, `drop` and
//! `rename` take the frame: its columns move into the result, at no cost, and the frame's
//! old name is gone, as with any moved value.

use super::calls::Arguments;
use super::{Body, relations};
use crate::frames::{COLUMN_TYPES, is_column};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Column, Type, TypeKind};

impl<'db> Body<'db> {
    /// `df[["a", "b"]]`, with `index` the list: a new frame of copies of the columns named,
    /// in the order written. `None` when `index` is not a list.
    pub(super) fn frame_select(&mut self, index: NodeId, base: Type<'db>) -> Option<Type<'db>> {
        if self.tree.kind(index) != SyntaxKind::List {
            return None;
        }
        let TypeKind::DataFrame { columns, .. } = base.kind(self.db) else {
            return None;
        };
        let Some(names) = self.column_names(index, base) else {
            return Some(self.lowerer.unknown());
        };
        if names.is_empty() {
            return Some(self.lowerer.error(
                index,
                "a `DataFrame` needs at least one column",
                "no columns",
                None,
            ));
        }
        let picked = names
            .iter()
            .filter_map(|name| columns.iter().find(|column| column.name == *name))
            .cloned()
            .collect();
        Some(self.frame_of_columns(picked))
    }

    /// The call `node` of `assign`, `drop` or `rename` on a frame of type `base`, with
    /// `callee` the method's name: the frame the call gives, after moving the frame it is
    /// called on. `None`, with nothing checked, for any other method.
    pub(super) fn schema_method(
        &mut self,
        node: NodeId,
        base: Type<'db>,
        callee: &str,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        let TypeKind::DataFrame { columns, .. } = base.kind(self.db) else {
            return None;
        };
        let result = match callee {
            "assign" => self.frame_assign(node, columns, arguments),
            "drop" => self.frame_drop(node, base, arguments),
            "rename" => self.frame_rename(node, base, arguments),
            _ => return None,
        };
        // The arguments are checked first, since `df.assign(x=df.a + df.b)` reads the frame
        // that the call then takes. A call with a mistake takes nothing, so the mistake is
        // not followed by reports that the frame was moved.
        let receiver = self
            .tree
            .child(node, Role::Function)
            .and_then(|function| self.tree.child(function, Role::Value));
        let Some(columns) = result else {
            return Some(self.lowerer.unknown());
        };
        if let Some(receiver) = receiver {
            self.consume(receiver, base);
        }
        Some(self.frame_of_columns(columns))
    }

    /// The columns `df.assign(name=values, ...)` gives: those of the frame, `columns`, with a
    /// column of an existing name replaced where it stands and each new one added at the
    /// end, in the order written, as in pandas. Each value is an array of rank 1, which
    /// moves in. `None` after an error.
    fn frame_assign(
        &mut self,
        node: NodeId,
        columns: &[Column<'db>],
        arguments: &Arguments,
    ) -> Option<Vec<Column<'db>>> {
        if !arguments.positional.is_empty() || arguments.keywords.is_empty() {
            self.untyped(arguments);
            self.lowerer.error(
                node,
                "`assign` takes each new column by name",
                "wrong arguments",
                Some("for example `df.assign(notional=df.price * df.volume)`"),
            );
            return None;
        }
        let mut result = columns.to_vec();
        let mut complete = true;
        for &(name, value) in &arguments.keywords {
            let name = self.tree.text(name, self.text).to_owned();
            let found = self.expression(value, None);
            self.consume(value, found);
            let element = match *found.kind(self.db) {
                TypeKind::Array { element, rank: 1 } if is_column(self.db, element) => element,
                _ if relations::is_open(self.db, found) => {
                    complete = false;
                    continue;
                }
                TypeKind::Array { element, rank: 1 } => {
                    let message = format!(
                        "a column of a `DataFrame` cannot hold `{}` yet",
                        self.show(element)
                    );
                    self.lowerer
                        .error(value, &message, "not a column yet", Some(COLUMN_TYPES));
                    complete = false;
                    continue;
                }
                _ => {
                    let message = format!(
                        "a column is an array of rank 1, found `{}`",
                        self.show(found)
                    );
                    self.lowerer.error(
                        value,
                        &message,
                        "not a column",
                        Some(
                            "to give every row one value, build the array: \
                             `Array.fill(len(df), value)`",
                        ),
                    );
                    complete = false;
                    continue;
                }
            };
            match result.iter_mut().find(|column| column.name == name) {
                Some(column) => column.element = element,
                None => result.push(Column { name, element }),
            }
        }
        complete.then_some(result)
    }

    /// The columns `df.drop(columns=[...])` gives: those of the frame of type `base` that
    /// the list, or the one name, does not name. `None` after an error.
    fn frame_drop(
        &mut self,
        node: NodeId,
        base: Type<'db>,
        arguments: &Arguments,
    ) -> Option<Vec<Column<'db>>> {
        let TypeKind::DataFrame { columns, .. } = base.kind(self.db) else {
            return None;
        };
        let Some(given) = self.columns_argument(arguments) else {
            self.lowerer.error(
                node,
                "`drop` takes the columns to leave out as `columns=`",
                "wrong arguments",
                Some("for example `df.drop(columns=[\"volume\"])`"),
            );
            return None;
        };
        let names = if self.tree.kind(given) == SyntaxKind::List {
            self.column_names(given, base)?
        } else {
            let name = self.column_name(given, base)?;
            vec![name]
        };
        let kept: Vec<Column<'db>> = columns
            .iter()
            .filter(|column| !names.contains(&column.name))
            .cloned()
            .collect();
        if kept.is_empty() {
            self.lowerer.error(
                given,
                "`drop` would leave the `DataFrame` with no columns",
                "every column",
                None,
            );
            return None;
        }
        Some(kept)
    }

    /// The columns `df.rename(columns={"old": "new"})` gives: those of the frame of type
    /// `base`, each under its new name where the dict gives one. `None` after an error.
    fn frame_rename(
        &mut self,
        node: NodeId,
        base: Type<'db>,
        arguments: &Arguments,
    ) -> Option<Vec<Column<'db>>> {
        let TypeKind::DataFrame { columns, .. } = base.kind(self.db) else {
            return None;
        };
        let table = self
            .columns_argument(arguments)
            .filter(|&given| self.tree.kind(given) == SyntaxKind::Dictionary);
        let Some(table) = table else {
            self.lowerer.error(
                node,
                "`rename` takes the new names as `columns=`, a dict from old name to new",
                "wrong arguments",
                Some("for example `df.rename(columns={\"price\": \"cost\"})`"),
            );
            return None;
        };
        let mut result = columns.clone();
        let mut complete = true;
        let entries: Vec<NodeId> = self.tree.children_with(table, Role::Item).collect();
        for entry in entries {
            let key = self.tree.child(entry, Role::Key);
            let value = self.tree.child(entry, Role::Value);
            let (Some(key), Some(value)) = (key, value) else {
                self.lowerer.error(
                    entry,
                    "`**` cannot give the new names of columns",
                    "unpacks a dict",
                    None,
                );
                complete = false;
                continue;
            };
            let old = self.column_name(key, base);
            let new = self.written_name(value);
            let (Some(old), Some(new)) = (old, new) else {
                complete = false;
                continue;
            };
            if let Some(position) = columns.iter().position(|column| column.name == old) {
                result[position].name = new;
            }
        }
        for (position, column) in result.iter().enumerate() {
            if result[..position]
                .iter()
                .any(|earlier| earlier.name == column.name)
            {
                let message = format!("`rename` would give two columns the name `{}`", column.name);
                self.lowerer
                    .error(table, &message, "one name for two columns", None);
                complete = false;
            }
        }
        complete.then_some(result)
    }

    /// The value of the one argument of `df.drop` or `df.rename`, which is given as
    /// `columns=`; `None`, after typing whatever was passed, for any other arguments.
    fn columns_argument(&mut self, arguments: &Arguments) -> Option<NodeId> {
        match (
            arguments.positional.as_slice(),
            arguments.keywords.as_slice(),
        ) {
            ([], &[(name, value)]) if self.tree.text(name, self.text) == "columns" => Some(value),
            _ => {
                self.untyped(arguments);
                None
            }
        }
    }

    /// The column names the list `list` writes, each a string literal that names a column of
    /// the frame of type `base`, none of them twice. `None` after an error.
    fn column_names(&mut self, list: NodeId, base: Type<'db>) -> Option<Vec<String>> {
        let items: Vec<NodeId> = self.tree.children_with(list, Role::Item).collect();
        let mut names: Vec<String> = Vec::with_capacity(items.len());
        let mut complete = true;
        for item in items {
            match self.column_name(item, base) {
                Some(name) if names.contains(&name) => {
                    let message = format!("the column `{name}` is named twice");
                    self.lowerer.error(item, &message, "a second time", None);
                    complete = false;
                }
                Some(name) => names.push(name),
                None => complete = false,
            }
        }
        complete.then_some(names)
    }

    /// The name the string literal `node` writes, which must be a column of the frame of
    /// type `base`. `None` after an error.
    fn column_name(&mut self, node: NodeId, base: Type<'db>) -> Option<String> {
        let name = self.written_name(node)?;
        let TypeKind::DataFrame { columns, .. } = base.kind(self.db) else {
            return None;
        };
        if columns.iter().any(|column| column.name == name) {
            return Some(name);
        }
        self.no_column(node, base, &name);
        None
    }

    /// The text of the string literal `node`, a column's name. `None`, after an error, when
    /// `node` is anything else, since a frame's type lists its columns.
    fn written_name(&mut self, node: NodeId) -> Option<String> {
        if let Some(name) = self.literal_text(node) {
            return Some(name.to_owned());
        }
        self.expression(node, None);
        self.lowerer.error(
            node,
            "a column's name is written as a string literal",
            "not a string literal",
            Some(
                "a `DataFrame`'s type lists its columns, so each name must be known when \
                 compiling",
            ),
        );
        None
    }

    /// The type of a frame with the columns `columns`, which no struct names.
    fn frame_of_columns(&self, columns: Vec<Column<'db>>) -> Type<'db> {
        Type::new(self.db, TypeKind::DataFrame { columns, row: None })
    }
}
