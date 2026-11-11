//! Dictionaries (decision 0035): displays, `d[k]`, `k in d`, the methods and the loops.
//!
//! A `dict[K, V]` maps keys to values in insertion order. Keys compare exactly, so `K` is an
//! integer, `bool`, `str`, enum or a tuple of them, never a float ([`key_error`]). A lookup
//! borrows the key, a display or `d[k] = v` moves the key and value in, and `d.pop(k)` is the
//! only way a value moves out.
//!
//! ```text
//! error: `f64` cannot be a dict key
//!  --> bins.byn:2:20
//!   |
//! 2 |     counts: dict[f64, i64] = {}
//!   |                  ^^^ a float key
//!   = note: floats are not compared exactly: `0.1 + 0.2` is not `0.3`. Bin the value to an integer first, such as `int(x / width)`
//! ```

use super::Body;
use super::calls::Arguments;
use super::exclusive::Passed;
use super::relations;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Mode, Primitive, Type, TypeDb, TypeKind};

/// The methods a `dict` has, for the error on any other.
const DICT_METHODS: &str = "`get`, `pop`, `setdefault`, `update`, `clear`, `copy`, and `keys`, \
     `values` and `items` in a `for` loop";

impl<'db> Body<'db> {
    /// `{k: v, ...}`: a `dict` whose keys share one type and values share one type, taken from
    /// the context or the first entry. An empty display needs its type written, unless the
    /// annotation it would come from already had an error.
    pub(super) fn dictionary(&mut self, node: NodeId, expected: Option<Type<'db>>) -> Type<'db> {
        let (mut key, mut value) = match expected.map(|ty| ty.kind(self.db)) {
            Some(&TypeKind::Dict { key, value }) => (Some(key), Some(value)),
            Some(TypeKind::Unknown)
                if self.tree.children_with(node, Role::Item).next().is_none() =>
            {
                return self.lowerer.unknown();
            }
            _ => (None, None),
        };
        let items: Vec<NodeId> = self.tree.children_with(node, Role::Item).collect();
        if items.is_empty() {
            return match (key, value) {
                (Some(key), Some(value)) => Type::new(self.db, TypeKind::Dict { key, value }),
                _ => self.lowerer.error(
                    node,
                    "an empty dict needs its type written",
                    "the key and value types are unknown",
                    Some("for example `counts: dict[str, i64] = {}`"),
                ),
            };
        }
        for item in items {
            if self.tree.kind(item) != SyntaxKind::DictionaryEntry {
                for child in self.tree.children(item).collect::<Vec<_>>() {
                    self.expression(child, None);
                }
                self.lowerer.error(
                    item,
                    "`**` in a dict display is not supported",
                    "unpacks another dict",
                    Some("copy the other dict and set the extra keys: `merged = other.copy()`"),
                );
                continue;
            }
            for (role, slot, what) in [
                (Role::Key, &mut key, "keys"),
                (Role::Value, &mut value, "values"),
            ] {
                let Some(part) = self.tree.child(item, role) else {
                    continue;
                };
                let found = self.expression(part, *slot);
                self.consume(part, found);
                match *slot {
                    None => *slot = Some(found),
                    Some(first) if !relations::compatible(self.db, first, found) => {
                        let message = format!(
                            "dict {what} must share one type, found `{}` and `{}`",
                            self.show(first),
                            self.show(found)
                        );
                        self.lowerer.error(part, &message, "a different type", None);
                    }
                    Some(_) => {}
                }
            }
        }
        let unknown = self.lowerer.unknown();
        let (key, value) = (key.unwrap_or(unknown), value.unwrap_or(unknown));
        if expected.is_none()
            && let Some((message, label, hint)) = key_error(self.db, key)
            && let Some(first) = self.tree.children_with(node, Role::Item).next()
        {
            let at = self.tree.child(first, Role::Key).unwrap_or(first);
            return self.lowerer.error(at, &message, label, Some(&hint));
        }
        Type::new(self.db, TypeKind::Dict { key, value })
    }

    /// `{k: v for ...}`: a `dict` whose key and value types come from the context or from the
    /// key and value expressions. The value moves into each entry, so it cannot be an element a
    /// clause borrows; the key is borrowed and copied in, as `d[k] = v` copies it, so
    /// `{name: len(name) for name in names}` reads `name` twice. A key written twice keeps its
    /// first place and its last value, as in Python.
    pub(super) fn dict_comprehension(
        &mut self,
        node: NodeId,
        expected: Option<Type<'db>>,
    ) -> Type<'db> {
        let (key, value) = match expected.map(|ty| ty.kind(self.db)) {
            Some(&TypeKind::Dict { key, value }) => (Some(key), Some(value)),
            _ => (None, None),
        };
        let unknown = self.lowerer.unknown();
        let part = |body: &mut Self, role, hint: Option<Type<'db>>, what| {
            let Some(child) = body.tree.child(node, role) else {
                return hint.unwrap_or(unknown);
            };
            let found = body.expression(child, hint);
            if role == Role::Value {
                body.consume(child, found);
            }
            match hint {
                Some(hint) => {
                    body.expect(child, hint, found, what);
                    hint
                }
                None => found,
            }
        };
        let clauses: Vec<NodeId> = self.tree.children_with(node, Role::Item).collect();
        let mut key_type = unknown;
        let value_type = self.comprehension_clauses(&clauses, &mut |body| {
            key_type = part(body, Role::Key, key, "the dict's keys are");
            part(body, Role::Value, value, "the dict's values are")
        });
        if expected.is_none()
            && let Some((message, label, hint)) = key_error(self.db, key_type)
            && let Some(at) = self.tree.child(node, Role::Key)
        {
            return self.lowerer.error(at, &message, label, Some(&hint));
        }
        Type::new(
            self.db,
            TypeKind::Dict {
                key: key_type,
                value: value_type,
            },
        )
    }

    /// `d[k]`, the value for the key `index` in a `dict[K, V]`: the key is checked against `K`
    /// and borrowed, and the value is `V`. A slice has no meaning on a `dict`.
    pub(super) fn dict_subscript(
        &mut self,
        node: NodeId,
        (key, value): (Type<'db>, Type<'db>),
        indices: &[NodeId],
    ) -> Type<'db> {
        let [index] = indices else {
            for &index in indices {
                self.expression(index, None);
            }
            return self.lowerer.error(
                node,
                "a dict takes one key",
                "more than one key",
                Some("a key of several parts is a tuple: `d[(i, j)]`"),
            );
        };
        if self.tree.kind(*index) == SyntaxKind::Slice {
            return self.lowerer.error(
                node,
                "a dict cannot be sliced",
                "a slice",
                Some("a dict is looked up by key: `d[k]`"),
            );
        }
        let found = self.expression(*index, Some(key));
        self.expect(*index, key, found, "the dict's keys are");
        value
    }

    /// `del d[k]`, which removes `k`'s entry from the `dict` `d` and drops its key and value; a
    /// missing key panics, as `d[k]` does. The syntax check allows `del` only on one subscript,
    /// and here the subscript must be of a `dict`: a list or an array keeps its length.
    pub(super) fn delete(&mut self, statement: NodeId) {
        let Some(target) = self.tree.child(statement, Role::Target) else {
            return;
        };
        let (Some(receiver), Some(index)) = (
            self.tree.child(target, Role::Value),
            self.tree.child(target, Role::Index),
        ) else {
            return;
        };
        if self.environ_changed(target) {
            return;
        }
        let base = self.expression(receiver, None);
        let &TypeKind::Dict { key, .. } = base.kind(self.db) else {
            self.expression(index, None);
            if !matches!(base.kind(self.db), TypeKind::Unknown) {
                let message = format!(
                    "`del` removes a dict entry, not an element of `{}`",
                    self.show(base)
                );
                self.lowerer.error(
                    target,
                    &message,
                    "not a dict",
                    Some("a list shrinks with `xs.pop()`; an array keeps its length"),
                );
            }
            return;
        };
        let found = self.expression(index, Some(key));
        self.expect(index, key, found, "the dict's keys are");
        self.change(receiver);
    }

    /// Checks `left in right` or `left not in right` when `right` is a `dict`: `left` must be
    /// of the key type.
    pub(super) fn dict_contains(&mut self, left: NodeId, left_type: Type<'db>, right: Type<'db>) {
        if let &TypeKind::Dict { key, .. } = right.kind(self.db) {
            self.expect(left, key, left_type, "the dict's keys are");
        }
    }

    /// A method call on a `dict[K, V]`, such as `d.get(k)` or `d.pop(k, default)`, whose
    /// receiver is the expression `receiver` of type `base`: the result's type. `pop`,
    /// `setdefault`, `update` and `clear` change the receiver; `update` borrows its argument, a
    /// `dict` of the same type.
    /// `keys`, `values` and `items` are only for `for` loops, which [`dict_iteration`]
    /// handles, so here they are an error.
    ///
    /// [`dict_iteration`]: Self::dict_iteration
    pub(super) fn dict_method(
        &mut self,
        node: NodeId,
        (receiver, base): (NodeId, Type<'db>),
        (key, value): (Type<'db>, Type<'db>),
        name: &str,
        arguments: &Arguments,
    ) -> Type<'db> {
        let positional = arguments.positional.as_slice();
        if !arguments.keywords.is_empty() {
            self.untyped(arguments);
            let message = format!("`dict.{name}` takes no keyword arguments");
            return self
                .lowerer
                .error(node, &message, "a keyword argument", None);
        }
        let key_argument = |body: &mut Self, argument: NodeId| {
            let found = body.expression(argument, Some(key));
            body.expect(argument, key, found, "the dict's keys are");
        };
        let default = |body: &mut Self, argument: NodeId| {
            let found = body.expression(argument, Some(value));
            body.consume(argument, found);
            body.expect(argument, value, found, "the dict's values are");
        };
        match (name, positional) {
            ("get", [k]) => {
                key_argument(self, *k);
                if let Some(error) = self.copied_out(node, "`get` with no default", value) {
                    return error;
                }
                Type::new(self.db, TypeKind::Optional(value))
            }
            ("get", [k, fallback]) => {
                key_argument(self, *k);
                default(self, *fallback);
                self.copied_out(node, "`get`", value).unwrap_or(value)
            }
            ("setdefault", [k, fallback]) => {
                key_argument(self, *k);
                default(self, *fallback);
                self.change(receiver);
                self.copied_out(node, "`setdefault`", value)
                    .unwrap_or(value)
            }
            ("update", [other]) => {
                let found = self.expression(*other, Some(base));
                self.expect(
                    *other,
                    base,
                    found,
                    "`update` takes a dict of the same type,",
                );
                // `d.update(d)` would read the dict it changes, so it is an overlap.
                let mode = |node, mode| Passed {
                    node,
                    mode,
                    ty: base,
                };
                self.pass(&[mode(receiver, Mode::InOut), mode(*other, Mode::Borrow)]);
                Type::new(self.db, TypeKind::None)
            }
            ("clear", []) => {
                self.change(receiver);
                Type::new(self.db, TypeKind::None)
            }
            ("pop", [k, rest @ ..]) if rest.len() <= 1 => {
                key_argument(self, *k);
                if let Some(fallback) = rest.first() {
                    default(self, *fallback);
                }
                self.change(receiver);
                value
            }
            ("copy", []) => base,
            ("keys" | "values" | "items", []) => {
                let message = format!("`{name}()` of a dict is only for a `for` loop");
                self.lowerer.error(
                    node,
                    &message,
                    "not in a loop",
                    Some("loop over it, `for k, v in d.items():`; `len(d)` counts the keys"),
                )
            }
            _ => {
                self.untyped(arguments);
                let message = format!(
                    "`{}` has no method `{name}` taking these arguments",
                    self.show(base)
                );
                let hint = format!("a dict has {DICT_METHODS}");
                self.lowerer
                    .error(node, &message, "not defined", Some(&hint))
            }
        }
    }

    /// The error for `what`, a method that copies a value of type `value` out of a `dict`, when
    /// the value cannot be copied out, or `None` when it can: a copy type, a `str`, or a tuple
    /// of them. A `str` is copied as a key is, since it is short text. Copying a list or an
    /// array out of a lookup would hide a large copy, so the program asks with `in` and borrows
    /// `d[k]` instead (Collins's choice for roadmap 1.13.14).
    fn copied_out(&mut self, node: NodeId, what: &str, value: Type<'db>) -> Option<Type<'db>> {
        if copies_out(self.db, value) {
            return None;
        }
        let message = format!(
            "{what} needs values that copy or are `str`, found `{}`",
            self.show(value)
        );
        Some(self.lowerer.error(
            node,
            &message,
            "would copy the value out",
            Some("ask first, then borrow it: `if k in d:` and `d[k]`"),
        ))
    }

    /// The element type of a `for` loop over `d.keys()`, `d.values()` or `d.items()`, or `None`
    /// when `iterator` is not such a call. The `dict` is typed, so lowering can read it.
    pub(super) fn dict_iteration(&mut self, iterator: NodeId) -> Option<Type<'db>> {
        if self.tree.kind(iterator) != SyntaxKind::Call {
            return None;
        }
        let function = self.tree.child(iterator, Role::Function)?;
        if self.tree.kind(function) != SyntaxKind::Attribute
            || !self.arguments(iterator).positional.is_empty()
        {
            return None;
        }
        let (value, attribute) = (
            self.tree.child(function, Role::Value)?,
            self.tree.child(function, Role::Attribute)?,
        );
        let method = self.tree.text(attribute, self.text);
        if !matches!(method, "keys" | "values" | "items") {
            return None;
        }
        let dict = self.expression(value, None);
        let &TypeKind::Dict { key, value: held } = dict.kind(self.db) else {
            return None;
        };
        let element = match method {
            "keys" => key,
            "values" => held,
            _ => Type::new(self.db, TypeKind::Tuple(vec![key, held])),
        };
        Some(element)
    }

    /// The `dict` `d` when `iterator` is `d.keys()`, `d.values()` or `d.items()`: the
    /// collection a loop over it borrows, so the body cannot change `d`.
    pub(super) fn dict_view_receiver(&self, iterator: NodeId) -> Option<NodeId> {
        if self.tree.kind(iterator) != SyntaxKind::Call {
            return None;
        }
        let function = self.tree.child(iterator, Role::Function)?;
        let attribute = self.tree.child(function, Role::Attribute)?;
        let receiver = self.tree.child(function, Role::Value)?;
        let is_view = self.tree.kind(function) == SyntaxKind::Attribute
            && matches!(
                self.tree.text(attribute, self.text),
                "keys" | "values" | "items"
            )
            && matches!(
                self.typed.expression(receiver).map(|ty| ty.kind(self.db)),
                Some(TypeKind::Dict { .. })
            );
        is_view.then_some(receiver)
    }
}

/// Whether `get` and `setdefault` can copy a value of type `ty` out of a `dict`: a copy type, a
/// `str`, or a tuple of them, the values lowering copies as it copies keys.
fn copies_out<'db>(db: &'db dyn TypeDb, ty: Type<'db>) -> bool {
    match ty.kind(db) {
        TypeKind::Str => true,
        TypeKind::Tuple(items) => items.iter().all(|&item| copies_out(db, item)),
        _ => crate::forms::is_copy(db, ty),
    }
}

/// Why `ty` cannot be a `dict` key, as a message, a label and a hint, or `None` when it can:
/// integers, `bool`, `str`, enums and tuples of them compare exactly (decision 0035). An unknown
/// type had its error already.
pub(crate) fn key_error<'db>(
    db: &'db dyn TypeDb,
    ty: Type<'db>,
) -> Option<(String, &'static str, String)> {
    let shown = ty.display(db);
    match ty.kind(db) {
        TypeKind::Primitive(Primitive::Integer(_) | Primitive::Bool)
        | TypeKind::Str
        | TypeKind::Enum(_)
        | TypeKind::Unknown => None,
        TypeKind::Tuple(items) => items.iter().find_map(|&item| key_error(db, item)),
        TypeKind::Primitive(Primitive::Float(_) | Primitive::Complex(_)) => Some((
            format!("`{shown}` cannot be a dict key"),
            "a float key",
            "floats are not compared exactly: `0.1 + 0.2` is not `0.3`. Bin the value to an \
             integer first, such as `int(x / width)`"
                .to_owned(),
        )),
        TypeKind::Struct(_) => Some((
            format!("`{shown}` cannot be a dict key"),
            "a struct key",
            "use a tuple of its fields as the key".to_owned(),
        )),
        _ => Some((
            format!("`{shown}` cannot be a dict key"),
            "not a key type",
            "a key is an integer, `bool`, `str`, enum, or a tuple of them".to_owned(),
        )),
    }
}
