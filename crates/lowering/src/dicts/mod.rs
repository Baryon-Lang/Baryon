//! Dictionaries (decision 0035): displays, `d[k]` read and written, `k in d`, `len(d)`,
//! `get`, `pop`, `copy`, and loops over the keys, values or items.
//!
//! Every operation is built from four IR built-ins on positions: [`Builtin::DictFind`] gives
//! the position of a key's entry or -1, [`Builtin::DictInsert`] adds a key that is not there,
//! [`Builtin::DictRemove`] takes an entry's value out, and [`Builtin::DictNext`] steps through
//! the entries in insertion order. An [`Index`](Projection::Index) step by a position names the
//! entry as a `(key, value)` pair, so `d[k]` is a place: read, written or updated in place.
//!
//! A missing key panics. The panic names the key as Python's `repr` writes it, and that text is
//! built only on the failing path, so a lookup that succeeds costs one hash and no formatting.

mod loops;
mod methods;
mod text;

use crate::builder::Builder;
use mir::{
    Argument, BinaryOperator, Builtin, Callee, Check, Constant, Local, Operand, Piece, Place,
    Projection, Rvalue, StatementKind, TerminatorKind,
};
use syntax::{NodeId, Role, SyntaxKind};
use types::{FormatSpec, Type, TypeKind};

impl<'db> Builder<'db> {
    /// The display `{k: v, ...}` into `place`: the empty `dict`, then each entry set in order,
    /// so a key written twice keeps its first place and its last value, as in Python.
    pub(crate) fn dict_display(&mut self, place: &Place, node: NodeId) {
        let ty = self.ty(node);
        self.assign(place.clone(), Rvalue::Aggregate(ty, Vec::new()), node);
        let entries: Vec<NodeId> = self.tree.children_with(node, Role::Item).collect();
        for entry in entries {
            let (Some(key), Some(value)) = (
                self.tree.child(entry, Role::Key),
                self.tree.child(entry, Role::Value),
            ) else {
                self.unsupported(entry, "`**` in a dict display");
                continue;
            };
            let key = self.owned_key(ty, key);
            self.dict_set(place, key, value, entry);
        }
    }

    /// `d[k] = v`, the assignment `target = value` whose target is a subscript of a `dict`.
    /// The value is computed first, then the key, as Python does.
    pub(crate) fn dict_store(&mut self, target: NodeId, value: NodeId) {
        let (Some(dict), Some(key)) = (
            self.tree.child(target, Role::Value),
            self.tree.child(target, Role::Index),
        ) else {
            return;
        };
        let Some(place) = self.place(dict) else {
            return;
        };
        let ty = self.ty(dict);
        let Some((_, held)) = self.dict_types(ty, target) else {
            return;
        };
        let value = self.owned_value(held, value);
        let key = self.stored_key(ty, key);
        self.set_entry(&place, key, value, target);
    }

    /// Sets the entry of the dict comprehension `node` in the `dict` at `place`: its key, then
    /// its value, stored as `d[k] = v` stores them, so a key read from a variable is copied and
    /// `{name: len(name) for name in names}` keeps `name` usable for the value.
    pub(crate) fn dict_comprehension_entry(&mut self, place: &Place, node: NodeId) {
        let (Some(key), Some(value)) = (
            self.tree.child(node, Role::Key),
            self.tree.child(node, Role::Value),
        ) else {
            return;
        };
        let key = self.stored_key(self.ty(node), key);
        self.dict_set(place, key, value, node);
    }

    /// `del d[k]`: removes `k`'s entry, after the check that panics when it is missing, and
    /// drops the key and the value. The checker allows `del` only on a `dict` entry.
    pub(crate) fn dict_delete(&mut self, statement: NodeId) {
        let Some(target) = self.tree.child(statement, Role::Target) else {
            return;
        };
        let (Some(dict), Some(key)) = (
            self.tree.child(target, Role::Value),
            self.tree.child(target, Role::Index),
        ) else {
            return;
        };
        let ty = self.ty(dict);
        let Some((_, held)) = self.dict_types(ty, target) else {
            return;
        };
        let Some(base) = self.place(dict) else {
            return;
        };
        let Some(position) = self.existing_entry(&base, ty, key, target) else {
            return;
        };
        let value = self.temporary(held, target);
        self.remove(&base, position, Place::from(value), target);
        if self.needs_drop(held) {
            self.push(StatementKind::Drop(Place::from(value)), target);
        }
    }

    /// The place `d[k]` names, the value of `k`'s entry, after finding it and panicking when it
    /// is missing.
    pub(crate) fn dict_value(&mut self, node: NodeId) -> Option<Place> {
        let dict = self.tree.child(node, Role::Value)?;
        let key = self.tree.child(node, Role::Index)?;
        let base = self.borrow(dict);
        let position = self.existing_entry(&base, self.ty(dict), key, node)?;
        Some(entry(base, position, 1))
    }

    /// Whether the assignment target `target` is `d[k]` on a `dict`, which
    /// [`dict_store`](Self::dict_store) sets, adding the key when it is missing.
    pub(crate) fn is_dict_subscript(&self, target: NodeId) -> bool {
        self.tree.kind(target) == SyntaxKind::Subscript
            && self
                .tree
                .child(target, Role::Value)
                .is_some_and(|dict| matches!(self.ty(dict).kind(self.db), TypeKind::Dict { .. }))
    }

    /// Sets the value for the owned key in the local `key` to the value of `value`, in the
    /// `dict` at `dict`.
    fn dict_set(&mut self, dict: &Place, key: Local, value: NodeId, node: NodeId) {
        let ty = self.place_type(dict);
        let Some((_, held)) = self.dict_types(ty, node) else {
            return;
        };
        let value = self.owned_value(held, value);
        self.set_entry(dict, key, value, node);
    }

    /// Sets the value for the owned key in `key` to the owned `value`: when the key is there its
    /// old value is dropped and replaced, and the key passed in is dropped at the end of the
    /// statement; otherwise the key moves into a new entry, last in the order.
    fn set_entry(&mut self, dict: &Place, key: Local, value: Operand<'db>, node: NodeId) {
        let int = self.int_type();
        let position = self.temporary(int, node);
        self.then(node, |next| TerminatorKind::Call {
            callee: Callee::Builtin(Builtin::DictFind),
            arguments: vec![
                Argument::Borrow(dict.clone()),
                Argument::Borrow(Place::from(key)),
            ],
            destination: Place::from(position),
            target: next,
            error: None,
        });
        let (found, missing, join) = (self.new_block(), self.new_block(), self.new_block());
        let present = self.at_least_zero(position, node);
        self.branch(present, (found, missing), node);
        self.start(found);
        self.replace(entry(dict.clone(), position, 1), value.clone(), node);
        self.terminate(TerminatorKind::Goto(join), node);
        self.start(missing);
        self.then(node, |next| TerminatorKind::Call {
            callee: Callee::Builtin(Builtin::DictInsert),
            arguments: vec![
                Argument::InOut(dict.clone()),
                Argument::Owned(Operand::Move(Place::from(key))),
            ],
            destination: Place::from(position),
            target: next,
            error: None,
        });
        self.assign(entry(dict.clone(), position, 1), Rvalue::Use(value), node);
        self.terminate(TerminatorKind::Goto(join), node);
        self.start(join);
    }

    /// The local holding the position of `key`'s entry in the `dict` at `dict` of type `ty`,
    /// after a check that panics, naming the key, when it is missing.
    fn existing_entry(
        &mut self,
        dict: &Place,
        ty: Type<'db>,
        key: NodeId,
        node: NodeId,
    ) -> Option<Local> {
        let (key_type, _) = self.dict_types(ty, node)?;
        let key_place = self.borrowed_key(key_type, key);
        let position = self.lookup(dict, &key_place, node);
        let found = self.temporary(self.boolean_type(), node);
        let zero = Operand::Constant(Constant::integer(self.int_type(), 0));
        self.assign(
            Place::from(found),
            Rvalue::Binary(
                BinaryOperator::GreaterEqual,
                Operand::Copy(Place::from(position)),
                zero,
            ),
            node,
        );
        let (ok, missing) = (self.new_block(), self.new_block());
        self.branch(Operand::Copy(Place::from(found)), (ok, missing), node);
        self.start(missing);
        let text = self.key_text(&key_place, key_type, node);
        self.terminate(
            TerminatorKind::Assert {
                condition: Operand::Copy(Place::from(found)),
                check: Check::KeyPresent { key: text },
                target: ok,
            },
            node,
        );
        self.start(ok);
        Some(position)
    }

    /// The local holding the position of `key`'s entry in the `dict` at `dict`, or -1.
    fn find(&mut self, dict: &Place, ty: Type<'db>, key: NodeId, node: NodeId) -> Option<Local> {
        let (key_type, _) = self.dict_types(ty, node)?;
        let key_place = self.borrowed_key(key_type, key);
        Some(self.lookup(dict, &key_place, node))
    }

    /// Calls [`Builtin::DictFind`] for the key at `key`, into a new `i64` local.
    fn lookup(&mut self, dict: &Place, key: &Place, node: NodeId) -> Local {
        let position = self.temporary(self.int_type(), node);
        self.then(node, |next| TerminatorKind::Call {
            callee: Callee::Builtin(Builtin::DictFind),
            arguments: vec![
                Argument::Borrow(dict.clone()),
                Argument::Borrow(key.clone()),
            ],
            destination: Place::from(position),
            target: next,
            error: None,
        });
        position
    }

    /// Writes the first position at or after `from` that holds an entry, or -1, into
    /// `position`.
    fn next_entry(&mut self, dict: &Place, position: Local, from: Operand<'db>, node: NodeId) {
        self.then(node, |next| TerminatorKind::Call {
            callee: Callee::Builtin(Builtin::DictNext),
            arguments: vec![Argument::Borrow(dict.clone()), Argument::Owned(from)],
            destination: Place::from(position),
            target: next,
            error: None,
        });
    }

    /// A `bool` operand saying whether the position in `position` is 0 or more: an entry was
    /// found.
    fn at_least_zero(&mut self, position: Local, node: NodeId) -> Operand<'db> {
        let flag = self.temporary(self.boolean_type(), node);
        let zero = Operand::Constant(Constant::integer(self.int_type(), 0));
        self.assign(
            Place::from(flag),
            Rvalue::Binary(
                BinaryOperator::GreaterEqual,
                Operand::Copy(Place::from(position)),
                zero,
            ),
            node,
        );
        Operand::Copy(Place::from(flag))
    }

    /// A place holding the key `key` as the key type, borrowed: the place it names when it has
    /// that type already, or a temporary holding it converted, as `counts[3]` on a
    /// `dict[i32, ...]` looks up an `i32` (decision 0035).
    fn borrowed_key(&mut self, key_type: Type<'db>, key: NodeId) -> Place {
        if self.ty(key) == key_type {
            return self.borrow(key);
        }
        let local = self.temporary(key_type, key);
        let operand = self.operand_as(key, key_type);
        self.assign(Place::from(local), Rvalue::Use(operand), key);
        Place::from(local)
    }

    /// A local owning the key `key`, converted to the key type of the `dict` type `ty`, to move
    /// into the table. It is dropped at the end of the statement unless it moved in.
    fn owned_key(&mut self, ty: Type<'db>, key: NodeId) -> Local {
        let key_type = self
            .dict_types(ty, key)
            .map_or_else(|| self.ty(key), |(key_type, _)| key_type);
        let local = self.temporary(key_type, key);
        let operand = self.operand_as(key, key_type);
        self.assign(Place::from(local), Rvalue::Use(operand), key);
        self.drop_later(local);
        local
    }

    /// A local owning a copy of the key `key` for `d[k] = v`, `setdefault` and a dict
    /// comprehension, which leave the key's variable usable, as in Python: a key read from a
    /// variable, a field or an element is
    /// copied, a `str` into a new string, and a key computed for the statement moves in as
    /// [`owned_key`](Self::owned_key) moves it. Keys are small, so the copy is cheap.
    fn stored_key(&mut self, ty: Type<'db>, key: NodeId) -> Local {
        let Some((key_type, _)) = self.dict_types(ty, key) else {
            return self.owned_key(ty, key);
        };
        let named = matches!(
            self.tree.kind(key),
            SyntaxKind::Name | SyntaxKind::Attribute | SyntaxKind::Subscript
        );
        if !named || !self.needs_drop(key_type) {
            return self.owned_key(ty, key);
        }
        let source = self.borrowed_key(key_type, key);
        let copy = self.key_copy(source, key_type, key);
        let local = self.temporary(key_type, key);
        self.assign(Place::from(local), Rvalue::Use(copy), key);
        self.drop_later(local);
        local
    }

    /// A new copy of the key part at `source`, of type `ty`: a `str` is copied into a new
    /// string, a tuple part by part, and a copy type bit for bit.
    fn key_copy(&mut self, source: Place, ty: Type<'db>, node: NodeId) -> Operand<'db> {
        match ty.kind(self.db) {
            TypeKind::Str => Operand::Move(self.copied(source, ty, node)),
            TypeKind::Tuple(items) => {
                let parts = (0u32..)
                    .zip(items.clone())
                    .map(|(field, item)| {
                        let part = source.clone().project(Projection::Field(field));
                        self.key_copy(part, item, node)
                    })
                    .collect();
                let tuple = self.temporary(ty, node);
                self.assign(Place::from(tuple), Rvalue::Aggregate(ty, parts), node);
                Operand::Move(Place::from(tuple))
            }
            _ => Operand::Copy(source),
        }
    }

    /// The value `value`, converted to `held`, as an operand that owns it. A value that owns
    /// memory is held in a temporary dropped at the end of the statement unless it moved.
    fn owned_value(&mut self, held: Type<'db>, value: NodeId) -> Operand<'db> {
        let operand = self.operand_as(value, held);
        if !self.needs_drop(held) {
            return operand;
        }
        let local = self.temporary(held, value);
        self.assign(Place::from(local), Rvalue::Use(operand), value);
        self.drop_later(local);
        Operand::Move(Place::from(local))
    }

    /// A place holding a copy of the value at `source`, of type `ty`: the place itself for a
    /// copy type, which is copied when read, or a new `str` for a `str`.
    fn copied(&mut self, source: Place, ty: Type<'db>, node: NodeId) -> Place {
        if !matches!(ty.kind(self.db), TypeKind::Str) {
            return source;
        }
        let copy = self.temporary(ty, node);
        let piece = Piece::Value(source, FormatSpec::default());
        self.assign(Place::from(copy), Rvalue::Format(vec![piece]), node);
        Place::from(copy)
    }

    /// The key and value types of the `dict` type `ty`, or `None` after reporting a type that
    /// is not a `dict`.
    fn dict_types(&mut self, ty: Type<'db>, node: NodeId) -> Option<(Type<'db>, Type<'db>)> {
        let &TypeKind::Dict { key, value } = ty.kind(self.db) else {
            self.unsupported(node, "this dict");
            return None;
        };
        Some((key, value))
    }

    /// The type of the value at `place`: its local's type, through each step.
    fn place_type(&self, place: &Place) -> Type<'db> {
        let mut ty = self.local_type(place.local);
        for projection in &place.projections {
            ty = match (projection, ty.kind(self.db)) {
                (Projection::Field(field), TypeKind::Tuple(items)) => {
                    items.get(*field as usize).copied().unwrap_or(ty)
                }
                (Projection::Value, TypeKind::Optional(value)) => *value,
                _ => return ty,
            };
        }
        ty
    }
}

/// Field `field` (0 the key, 1 the value) of the entry at the position in `position` of the
/// `dict` at `dict`.
fn entry(dict: Place, position: Local, field: u32) -> Place {
    dict.project(Projection::Index(vec![position]))
        .project(Projection::Field(field))
}
