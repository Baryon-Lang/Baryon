//! Frames in a function body (decision 0045): building a `DataFrame` from its columns, and
//! reading a column, `df.price` or `df["price"]`, and the attributes `shape` and `columns`.
//!
//! A frame's type lists its columns, so every column name here is checked when compiling: a
//! name the frame does not have is an error that lists the names it has.

use super::calls::Arguments;
use super::{Body, relations};
use crate::frames::{COLUMN_TYPES, is_column};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Column, Integer, Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// `DataFrame({"price": prices, "volume": volumes})`: a frame whose columns are the arrays,
    /// which move into it, named by the keys, in the order written. The names are string
    /// literals, since they become part of the frame's type. When the context expects a frame
    /// with these columns, such as `trades: DataFrame[Trade] = DataFrame({...})`, the result
    /// is that type, so its rows are that struct.
    pub(super) fn frame_new(
        &mut self,
        node: NodeId,
        arguments: &Arguments,
        expected: Option<Type<'db>>,
    ) -> Type<'db> {
        let wanted = match expected.map(|ty| ty.kind(self.db)) {
            Some(TypeKind::DataFrame { columns, .. }) => columns.clone(),
            _ => Vec::new(),
        };
        let table = match (arguments.positional.as_slice(), arguments.keywords.len()) {
            (&[table], 0) if self.tree.kind(table) == SyntaxKind::Dictionary => table,
            _ => {
                self.untyped(arguments);
                return self.lowerer.error(
                    node,
                    "`DataFrame(...)` takes a dict of its columns",
                    "wrong arguments",
                    Some("for example `DataFrame({\"price\": prices, \"volume\": volumes})`"),
                );
            }
        };
        let mut columns: Vec<Column<'db>> = Vec::new();
        let mut complete = true;
        let entries: Vec<NodeId> = self.tree.children_with(table, Role::Item).collect();
        for entry in entries {
            let column = self.frame_entry(entry, &wanted);
            complete &= column.is_some();
            let Some(column) = column else { continue };
            if columns.iter().any(|earlier| earlier.name == column.name) {
                let message = format!("the column `{}` is given twice", column.name);
                self.lowerer.error(entry, &message, "a second time", None);
                complete = false;
            } else {
                columns.push(column);
            }
        }
        if !complete {
            return self.lowerer.unknown();
        }
        if columns.is_empty() {
            return self.lowerer.error(
                table,
                "a `DataFrame` needs at least one column",
                "no columns",
                None,
            );
        }
        match expected {
            Some(expected) if wanted == columns => expected,
            _ => Type::new(self.db, TypeKind::DataFrame { columns, row: None }),
        }
    }

    /// One `"name": values` entry of the dict a frame is built from: the column it gives, or
    /// `None` after an error. `wanted` holds the columns the context expects, which give a
    /// literal such as `Array([1, 2])` its element type.
    fn frame_entry(&mut self, entry: NodeId, wanted: &[Column<'db>]) -> Option<Column<'db>> {
        let key = self.tree.child(entry, Role::Key);
        let value = self.tree.child(entry, Role::Value);
        let (Some(key), Some(value)) = (key, value) else {
            for child in self.tree.children(entry).collect::<Vec<_>>() {
                self.expression(child, None);
            }
            self.lowerer.error(
                entry,
                "`**` cannot give the columns of a `DataFrame`",
                "unpacks a dict",
                Some("write each column: `DataFrame({\"price\": prices})`"),
            );
            return None;
        };
        let name = self.literal_text(key).map(str::to_owned);
        if name.is_none() {
            self.expression(key, None);
            self.lowerer.error(
                key,
                "a column's name is written as a string literal",
                "not a string literal",
                Some(
                    "a `DataFrame`'s type lists its columns, so each name must be known when \
                     compiling",
                ),
            );
        }
        let context = name.as_ref().and_then(|name| {
            let column = wanted.iter().find(|column| column.name == *name)?;
            Some(self.column_type(column))
        });
        let found = self.expression(value, context);
        self.consume(value, found);
        let element = match *found.kind(self.db) {
            TypeKind::Array { element, rank: 1 } if is_column(self.db, element) => element,
            _ if relations::is_open(self.db, found) => return None,
            TypeKind::Array { element, rank: 1 } => {
                let message = format!(
                    "a column of a `DataFrame` cannot hold `{}` yet",
                    self.show(element)
                );
                self.lowerer
                    .error(value, &message, "not a column yet", Some(COLUMN_TYPES));
                return None;
            }
            _ => {
                let message = format!(
                    "a column is an array of rank 1, found `{}`",
                    self.show(found)
                );
                self.lowerer.error(value, &message, "not a column", None);
                return None;
            }
        };
        name.map(|name| Column { name, element })
    }

    /// `df.name` on a frame: the attribute `shape` or `columns`, or else the column of that
    /// name, as in pandas, where an attribute hides a column of the same name and
    /// `df["shape"]` still reaches it. `None` when `base` is not a frame.
    pub(super) fn frame_attribute(
        &mut self,
        node: NodeId,
        base: Type<'db>,
        name: &str,
    ) -> Option<Type<'db>> {
        let TypeKind::DataFrame { columns, .. } = base.kind(self.db) else {
            return None;
        };
        let int = Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::I64)),
        );
        Some(match name {
            // Rows, then columns.
            "shape" => Type::new(self.db, TypeKind::Tuple(vec![int, int])),
            "columns" => {
                let text = Type::new(self.db, TypeKind::Str);
                Type::new(self.db, TypeKind::List(text))
            }
            _ => match columns.iter().find(|column| column.name == name) {
                Some(column) => self.column_type(column),
                None => self.no_column(node, base, name),
            },
        })
    }

    /// `df["name"]` on a frame: the column of that name. `None` when `base` is not a frame.
    pub(super) fn frame_item(&mut self, node: NodeId, base: Type<'db>) -> Option<Type<'db>> {
        let TypeKind::DataFrame { columns, .. } = base.kind(self.db) else {
            return None;
        };
        let index = self.tree.child(node, Role::Index)?;
        let Some(name) = self.literal_text(index) else {
            let found = self.expression(index, None);
            if relations::is_open(self.db, found) {
                return Some(found);
            }
            let message = format!(
                "a `DataFrame` is indexed by a column's name, found `{}`",
                self.show(found)
            );
            return Some(self.lowerer.error(
                index,
                &message,
                "not a column's name",
                Some(
                    "write the name as a string literal, `df[\"price\"]`; rows by number, masks \
                     and slices come with roadmap items 3.13.2 and 3.13.4",
                ),
            ));
        };
        let name = name.to_owned();
        let column = columns.iter().find(|column| column.name == name);
        Some(match column {
            Some(column) => self.column_type(column),
            None => self.no_column(index, base, &name),
        })
    }

    /// Whether `node` reads a column of a frame, `df.price` or `df["price"]`: a place inside
    /// the frame, which can be borrowed but not moved out.
    pub(super) fn is_column_read(&self, node: NodeId) -> bool {
        matches!(
            self.tree.kind(node),
            SyntaxKind::Attribute | SyntaxKind::Subscript
        ) && self.reads_frame(node)
            && !self.is_frame_attribute(node)
    }

    /// Whether `node` reads `shape` or `columns` of a frame: a value computed at each read,
    /// so keeping it moves nothing.
    pub(super) fn is_frame_attribute(&self, node: NodeId) -> bool {
        self.tree.kind(node) == SyntaxKind::Attribute
            && self.reads_frame(node)
            && self
                .tree
                .child(node, Role::Attribute)
                .is_some_and(|name| matches!(self.tree.text(name, self.text), "shape" | "columns"))
    }

    /// Reports an assignment to `df.shape` or `df.columns` at `target`, and returns whether it
    /// was one. Both are computed from the frame: its shape changes only with its columns, and
    /// its column names are part of its type.
    pub(super) fn frame_attribute_assigned(&mut self, target: NodeId) -> bool {
        if !self.is_frame_attribute(target) {
            return false;
        }
        let name = self
            .tree
            .child(target, Role::Attribute)
            .map_or("", |name| self.tree.text(name, self.text));
        let message = format!("`{name}` of a `DataFrame` cannot be assigned");
        self.lowerer.error(
            target,
            &message,
            "computed from the frame",
            Some(
                "a frame's column names and their number are part of its type; `rename`, \
                 `assign` and `drop` give a new frame (roadmap 3.13.4)",
            ),
        );
        true
    }

    /// Whether the value `node` reads from, the `df` of `df.price` or `df["price"]`, is a
    /// frame.
    fn reads_frame(&self, node: NodeId) -> bool {
        self.tree.child(node, Role::Value).is_some_and(|value| {
            self.typed
                .expressions
                .get(&types::Key(value))
                .is_some_and(|ty| matches!(ty.kind(self.db), TypeKind::DataFrame { .. }))
        })
    }

    /// Reports the column `node` moved out of its frame, which would leave the frame with a
    /// column missing.
    pub(super) fn moved_out_of_frame(&mut self, node: NodeId) {
        let hint = format!(
            "a column can be read, passed to a call or assigned to; to keep it on its own, copy \
             it: `{}.copy()`",
            self.tree.text(node, self.text)
        );
        self.lowerer.error(
            node,
            "a column cannot be moved out of its `DataFrame`",
            "moved out of the frame",
            Some(&hint),
        );
    }

    /// The type of a column read from a frame: an array of rank 1 of the column's elements.
    fn column_type(&self, column: &Column<'db>) -> Type<'db> {
        let element = column.element;
        Type::new(self.db, TypeKind::Array { element, rank: 1 })
    }

    /// Reports `name` read from the frame type `base`, which has no such column, and lists
    /// the columns it has.
    fn no_column(&mut self, node: NodeId, base: Type<'db>, name: &str) -> Type<'db> {
        let TypeKind::DataFrame { columns, .. } = base.kind(self.db) else {
            return self.lowerer.unknown();
        };
        let names: Vec<String> = columns
            .iter()
            .map(|column| format!("`{}`", column.name))
            .collect();
        let message = format!("`{}` has no column `{name}`", self.show(base));
        let hint = format!("its columns are {}", names.join(", "));
        self.lowerer
            .error(node, &message, "not a column", Some(&hint))
    }

    /// `df.method(arguments)`, the call `node`, on a frame of type `base`: `copy()`, a new
    /// frame with a copy of every column.
    pub(super) fn frame_method(
        &mut self,
        function: NodeId,
        base: Type<'db>,
        callee: &str,
        arguments: &Arguments,
    ) -> Type<'db> {
        let given = arguments.positional.len() + arguments.keywords.len();
        self.untyped(arguments);
        match (callee, given) {
            ("copy", 0) => base,
            ("copy", _) => self.lowerer.error(
                function,
                "`copy` of a `DataFrame` takes no arguments",
                "wrong arguments",
                None,
            ),
            _ => {
                let message = format!("a `DataFrame` has no method `{callee}` yet");
                self.lowerer.error(
                    function,
                    &message,
                    "not defined yet",
                    Some(
                        "a `DataFrame` has `copy()` today; the rest of decision 0045's methods \
                         come with roadmap items 3.13.4 to 3.13.9",
                    ),
                )
            }
        }
    }
}
