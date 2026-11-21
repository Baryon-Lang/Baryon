//! The reductions of a grouping written as methods (decision 0045's amendment of
//! 2026-11-19, roadmap 3.13.22): `df.groupby("k").sum()`, `df.groupby("k")["v"].mean()`,
//! `df.groupby("k")[["v", "n"]].max()`, `df.groupby("k").v.min()` and `size()`.
//!
//! Each is another way to write `df.groupby(...).agg(...)`, which the
//! [`frame_groups`](super::frame_groups) module checks: the result is a frame of the key
//! columns followed by each reduced column under its own name, or by a column `size`. With
//! no columns chosen, every column that is not a key is reduced; a column of `str` there has
//! no sum, mean or spread, so it is a compile error unless `numeric_only=True` is written,
//! which leaves the columns of `str` out, as in pandas.

use super::Body;
use super::calls::Arguments;
use super::frame_groups::REDUCTIONS;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Call, Callee, Column, Integer, Key, Primitive, Type, TypeKind};

/// The reductions a column of `str` has, as `agg` gives them.
const TEXT_REDUCTIONS: [&str; 6] = ["min", "max", "first", "last", "count", "nunique"];

/// The columns a grouping's reduction reads, as the expression chose them.
#[derive(Clone, Copy)]
enum Chosen {
    /// `df.groupby(...).sum()`: every column that is not a key.
    Every,
    /// `df.groupby(...)["v"].sum()` or `df.groupby(...)[["v", "n"]].sum()`: the index.
    Index(NodeId),
    /// `df.groupby(...).v.sum()`: the node of the name `v`.
    Named(NodeId),
}

/// What a reduction of a grouping does with a column of `str`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Text {
    /// It is reduced, or reported when the reduction is one text does not have.
    Reduced,
    /// `numeric_only=True` was written: it is left out.
    Numeric,
    /// The method's arguments were wrong, and reported: it is left out with no word, so
    /// that one mistake is one error.
    Quiet,
}

/// Which columns of `str` a reduction of a grouping leaves out, and how it reports the rest.
#[derive(Clone, Copy)]
pub(super) struct Mode {
    /// Whether every column that is not a key is reduced, none being chosen.
    pub(super) every: bool,
    /// Whether the columns are several, every one or a list: `numeric_only=True` then
    /// leaves text out, where for one chosen column it is a mistake.
    pub(super) listed: bool,
    /// What becomes of a column of `str`.
    pub(super) text: Text,
}

impl<'db> Body<'db> {
    /// The type of the call `node` of `function` with `arguments` when it reduces a
    /// grouping, by `agg` or by a reduction written as a method. `None` for any other call.
    pub(super) fn grouped_call(
        &mut self,
        node: NodeId,
        function: NodeId,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        let grouped = self.grouped_frame(function);
        let grouped = grouped.and_then(|frame| self.frame_agg(node, frame, arguments));
        grouped.or_else(|| {
            let method = self.grouped_method(function)?;
            self.frame_group_method(node, method, arguments)
        })
    }

    /// The frame, the grouping and the columns chosen when `function`, the callee of a call,
    /// is a reduction of a grouping written as a method, read from the syntax alone:
    /// `df.groupby(...).sum`, `df.groupby(...)[...].sum` or `df.groupby(...).v.sum`. `None`
    /// for any other callee, `agg` included.
    fn grouped_method(&self, function: NodeId) -> Option<(NodeId, NodeId, Chosen)> {
        let method = self.tree.child(function, Role::Attribute)?;
        if !REDUCTIONS.contains(&self.tree.text(method, self.text)) {
            return None;
        }
        let value = self.tree.child(function, Role::Value)?;
        let (grouping, chosen) = match self.tree.kind(value) {
            SyntaxKind::Call => (value, Chosen::Every),
            SyntaxKind::Subscript => (
                self.tree.child(value, Role::Value)?,
                Chosen::Index(self.tree.child(value, Role::Index)?),
            ),
            SyntaxKind::Attribute => (
                self.tree.child(value, Role::Value)?,
                Chosen::Named(self.tree.child(value, Role::Attribute)?),
            ),
            _ => return None,
        };
        let groupby = self.tree.child(grouping, Role::Function)?;
        let named = self.tree.kind(grouping) == SyntaxKind::Call
            && self.tree.kind(groupby) == SyntaxKind::Attribute
            && self
                .tree
                .child(groupby, Role::Attribute)
                .is_some_and(|name| self.tree.text(name, self.text) == "groupby");
        let frame = self.tree.child(groupby, Role::Value)?;
        named.then_some((frame, grouping, chosen))
    }

    /// The call `node`, a reduction of the grouping `grouping` of `frame` written as a
    /// method, with `arguments` those of the method: the frame of one row for each group.
    /// `None`, with only `frame` typed, when `frame` is not a `DataFrame`, so the call is
    /// some other type's methods.
    fn frame_group_method(
        &mut self,
        node: NodeId,
        (frame, grouping, chosen): (NodeId, NodeId, Chosen),
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        let base = self.expression(frame, None);
        let TypeKind::DataFrame { columns, .. } = base.kind(self.db) else {
            return None;
        };
        let function = self.tree.child(node, Role::Function)?;
        let method = self.tree.child(function, Role::Attribute)?;
        let how = self.tree.text(method, self.text).to_owned();
        let Some((keys, mut result, mut complete)) = self.grouping_keys(node, Some(grouping), base)
        else {
            self.untyped(arguments);
            return Some(self.lowerer.unknown());
        };
        let numeric = self.numeric_only(node, &how, arguments);
        // After a wrong argument, text is left out, so that one mistake is one error.
        let wrong = numeric.is_none();
        complete &= !wrong;
        let numeric = numeric.unwrap_or(false);
        // The columns to reduce, each with the node an error about it points at.
        let read: Option<Vec<(String, NodeId)>> = match chosen {
            Chosen::Every => Some(
                columns
                    .iter()
                    .filter(|column| !keys.contains(&column.name))
                    .map(|column| (column.name.clone(), method))
                    .collect(),
            ),
            Chosen::Index(index) if self.tree.kind(index) == SyntaxKind::List => {
                let items: Vec<NodeId> = self.tree.children_with(index, Role::Item).collect();
                self.column_names(index, base)
                    .map(|names| names.into_iter().zip(items).collect())
            }
            Chosen::Index(index) => self
                .column_name(index, base)
                .map(|name| vec![(name, index)]),
            Chosen::Named(name) => self
                .grouped_attribute(name, base)
                .map(|column| vec![column]),
        };
        complete &= read.is_some();
        let every = matches!(chosen, Chosen::Every);
        let list = |index| self.tree.kind(index) == SyntaxKind::List;
        let mode = Mode {
            every,
            listed: every || matches!(chosen, Chosen::Index(index) if list(index)),
            text: match (wrong, numeric) {
                (true, _) => Text::Quiet,
                (false, true) => Text::Numeric,
                (false, false) => Text::Reduced,
            },
        };
        if how == "size" {
            let whole = Type::new(
                self.db,
                TypeKind::Primitive(Primitive::Integer(Integer::I64)),
            );
            complete &= self.added(&mut result, "size", whole, method);
        } else {
            let read = read.unwrap_or_default();
            complete &= self.reduce_columns(base, &keys, &mut result, &how, read, mode);
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

    /// Adds to `result`, the columns of the frame a grouping of the frame of type `base`
    /// by `keys` gives, each column of `read` reduced by `how`, under its own name; `read`
    /// pairs a column's name with the node an error about it points at. False after
    /// reporting a column that cannot be reduced so: a key, or text under a reduction text
    /// does not have. `mode` says which columns of `str` are left out without an error.
    pub(super) fn reduce_columns(
        &mut self,
        base: Type<'db>,
        keys: &[String],
        result: &mut Vec<Column<'db>>,
        how: &str,
        read: Vec<(String, NodeId)>,
        mode: Mode,
    ) -> bool {
        let TypeKind::DataFrame { columns, .. } = base.kind(self.db) else {
            return false;
        };
        let mut complete = true;
        for (name, at) in read {
            let stored = columns.iter().find(|column| column.name == name);
            let Some(stored) = stored else {
                continue;
            };
            if !mode.every && keys.contains(&name) {
                // pandas reduces a key as any column; a frame holds one column of each
                // name, so this waits for a name to give the second one.
                let message = format!("a grouping cannot reduce `{name}`, one of its keys, yet");
                let hint = "the result already holds each group's value of it";
                self.lowerer
                    .error(at, &message, "a key, not a value", Some(hint));
                complete = false;
                continue;
            }
            let text = match stored.element.kind(self.db) {
                TypeKind::Optional(element) => matches!(element.kind(self.db), TypeKind::Str),
                kind => matches!(kind, TypeKind::Str),
            };
            let numeric = mode.text == Text::Numeric;
            if text && (mode.text == Text::Quiet || (numeric && mode.listed)) {
                continue;
            }
            if text && (numeric || !TEXT_REDUCTIONS.contains(&how)) {
                self.text_reduced(at, how, &name, (mode.every, numeric));
                complete = false;
                continue;
            }
            match self.reduced_element(at, stored, how) {
                Some(element) => complete &= self.added(result, &name, element, at),
                None => complete = false,
            }
        }
        complete
    }

    /// Whether the reduction `how` of the call `node` was written with `numeric_only=True`,
    /// its one argument, which must be `True` or `False` written out. `None` after
    /// reporting any other argument.
    fn numeric_only(&mut self, node: NodeId, how: &str, arguments: &Arguments) -> Option<bool> {
        let takes = !matches!(how, "count" | "nunique" | "size");
        let flag = match (
            arguments.positional.as_slice(),
            arguments.keywords.as_slice(),
        ) {
            ([], []) => return Some(false),
            ([], &[(name, value)])
                if takes && self.tree.text(name, self.text) == "numeric_only" =>
            {
                match self.tree.text(value, self.text) {
                    "True" => Some(true),
                    "False" => Some(false),
                    _ => None,
                }
            }
            _ => None,
        };
        self.untyped(arguments);
        if flag.is_none() {
            let hint = "to reduce some columns only, choose them first: \
                        `df.groupby(\"symbol\")[[\"price\", \"volume\"]].sum()`";
            let (message, hint) = if takes {
                let message = format!("`{how}` of a grouping takes `numeric_only=True` or nothing");
                (message, Some(hint))
            } else {
                (format!("`{how}` of a grouping takes no arguments"), None)
            };
            self.lowerer.error(node, &message, "wrong arguments", hint);
        }
        flag
    }

    /// The column of the frame of type `base` that `name` names in `df.groupby(...).name`,
    /// with `name` as the node an error about it points at. `None` after reporting a name
    /// that is no column.
    fn grouped_attribute(&mut self, name: NodeId, base: Type<'db>) -> Option<(String, NodeId)> {
        let written = self.tree.text(name, self.text).to_owned();
        let TypeKind::DataFrame { columns, .. } = base.kind(self.db) else {
            return None;
        };
        if columns.iter().any(|column| column.name == written) {
            return Some((written, name));
        }
        let message = format!("the frame has no column `{written}` to reduce for each group");
        let names: Vec<String> = columns
            .iter()
            .map(|column| format!("`{}`", column.name))
            .collect();
        let hint = format!("its columns are {}", names.join(", "));
        self.lowerer
            .error(name, &message, "not a column", Some(&hint));
        None
    }

    /// Adds the column `name` of `element` to `result`, the columns of the frame a grouping
    /// gives. False after reporting at `node` that the result already has a column of that
    /// name, which a key column named `size` gives `size()`.
    pub(super) fn added(
        &mut self,
        result: &mut Vec<Column<'db>>,
        name: &str,
        element: Type<'db>,
        node: NodeId,
    ) -> bool {
        if result.iter().any(|column| column.name == name) {
            let message = format!("the result would have two columns named `{name}`");
            self.lowerer
                .error(node, &message, "one name for two columns", None);
            return false;
        }
        result.push(Column {
            name: name.to_owned(),
            element,
        });
        true
    }

    /// Reports at `node` that the reduction `how` of a grouping met the column `name` of
    /// `str`. It has no sum, mean or spread (decision 0045), which `every`, a reduction of
    /// every column, can leave out with `numeric_only=True`; and `numeric`, that argument
    /// written for the one column chosen, would leave nothing.
    fn text_reduced(
        &mut self,
        node: NodeId,
        how: &str,
        name: &str,
        (every, numeric): (bool, bool),
    ) {
        let (message, hint) = if numeric {
            (
                format!("`numeric_only=True` leaves out `{name}`, the one column chosen"),
                "a column of `str` is not a column of numbers".to_owned(),
            )
        } else if every {
            (
                format!("`{how}` of every column cannot reduce `{name}`, a column of `str`"),
                format!(
                    "write `{how}(numeric_only=True)` to leave the columns of `str` out, or \
                     choose the columns to reduce"
                ),
            )
        } else {
            (
                format!("`{how}` cannot reduce `{name}`, a column of `str`"),
                "a column of `str` has `count`, `size`, `nunique`, `first`, `last`, `min` and \
                 `max`"
                    .to_owned(),
            )
        };
        self.lowerer
            .error(node, &message, "a column of `str`", Some(&hint));
    }
}
