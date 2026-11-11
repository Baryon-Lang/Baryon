//! The methods of a `dict` and `in`: `get`, `pop`, `setdefault`, `update`, `clear`, `copy`, and
//! `k in d`.

use super::entry;
use crate::builder::Builder;
use mir::{
    Argument, BinaryOperator, Builtin, Callee, Constant, ConstantValue, Local, Operand, Place,
    Rvalue, StatementKind, TerminatorKind,
};
use syntax::{NodeId, Operator};
use types::{Type, TypeKind};

impl<'db> Builder<'db> {
    /// `key in d` or `key not in d` on a `dict`, into the `bool` at `place`.
    pub(crate) fn dict_contains(
        &mut self,
        place: Place,
        operator: Operator,
        (key, dict): (NodeId, NodeId),
        node: NodeId,
    ) {
        let base = self.borrow(dict);
        let Some(position) = self.find(&base, self.ty(dict), key, node) else {
            return;
        };
        let comparison = if operator == Operator::NotIn {
            BinaryOperator::Less
        } else {
            BinaryOperator::GreaterEqual
        };
        let zero = Operand::Constant(Constant::integer(self.int_type(), 0));
        self.assign(
            place,
            Rvalue::Binary(comparison, Operand::Copy(Place::from(position)), zero),
            node,
        );
    }

    /// A method of a `dict`, such as `d.get(k)`, `d.pop(k, default)` or `d.update(other)`, with
    /// `arguments`, the receiver first, into `place`.
    pub(crate) fn dict_method(
        &mut self,
        place: Place,
        method: &str,
        arguments: &[NodeId],
        node: NodeId,
    ) {
        match (method, arguments) {
            ("get", [dict, key, rest @ ..]) if rest.len() <= 1 => {
                self.dict_get(place, (*dict, *key, rest.first().copied()), node);
            }
            ("pop", [dict, key, rest @ ..]) if rest.len() <= 1 => {
                self.dict_pop(place, (*dict, *key, rest.first().copied()), node);
            }
            ("copy", [dict]) => self.dict_copy(&place, *dict, node),
            ("setdefault", [dict, key, default]) => {
                self.dict_setdefault(place, (*dict, *key, *default), node);
            }
            ("update", [dict, other]) => self.dict_update(*dict, *other, node),
            ("clear", [dict]) => self.dict_clear(*dict, node),
            _ => self.unsupported(node, &format!("the dict method `{method}`")),
        }
    }

    /// `d.get(k)` or `d.get(k, default)` into `place`: the value copied out as a `V | None`, or
    /// the value or the default. The checker allows `get` only on values that copy or are
    /// `str`, or tuples of them, which [`key_copy`](Self::key_copy) copies as it copies keys.
    fn dict_get(
        &mut self,
        place: Place,
        (dict, key, default): (NodeId, NodeId, Option<NodeId>),
        node: NodeId,
    ) {
        let ty = self.ty(dict);
        let Some((_, held)) = self.dict_types(ty, node) else {
            return;
        };
        let base = self.borrow(dict);
        let Some(position) = self.find(&base, ty, key, node) else {
            return;
        };
        let (found, missing, join) = (self.new_block(), self.new_block(), self.new_block());
        let present = self.at_least_zero(position, node);
        self.branch(present, (found, missing), node);
        self.start(found);
        let value = self.key_copy(entry(base, position, 1), held, node);
        let result = self.ty(node);
        let rvalue = if default.is_some() {
            Rvalue::Use(value)
        } else {
            Rvalue::Aggregate(result, vec![value])
        };
        self.assign(place.clone(), rvalue, node);
        self.terminate(TerminatorKind::Goto(join), node);
        self.start(missing);
        let fallback = match default {
            Some(default) => self.operand_as(default, held),
            None => Operand::Constant(Constant {
                ty: result,
                value: ConstantValue::None,
            }),
        };
        self.assign(place, Rvalue::Use(fallback), node);
        self.terminate(TerminatorKind::Goto(join), node);
        self.start(join);
    }

    /// `d.pop(k)` or `d.pop(k, default)` into `place`: the value moved out of the `dict` and
    /// its entry removed; without a default a missing key panics.
    fn dict_pop(
        &mut self,
        place: Place,
        (dict, key, default): (NodeId, NodeId, Option<NodeId>),
        node: NodeId,
    ) {
        let ty = self.ty(dict);
        let Some((_, held)) = self.dict_types(ty, node) else {
            return;
        };
        let Some(base) = self.place(dict) else {
            return;
        };
        let Some(default) = default else {
            let Some(position) = self.existing_entry(&base, ty, key, node) else {
                return;
            };
            return self.remove(&base, position, place, node);
        };
        let Some(position) = self.find(&base, ty, key, node) else {
            return;
        };
        let (found, missing, join) = (self.new_block(), self.new_block(), self.new_block());
        let present = self.at_least_zero(position, node);
        self.branch(present, (found, missing), node);
        self.start(found);
        self.remove(&base, position, place.clone(), node);
        self.terminate(TerminatorKind::Goto(join), node);
        self.start(missing);
        let fallback = self.operand_as(default, held);
        self.assign(place, Rvalue::Use(fallback), node);
        self.terminate(TerminatorKind::Goto(join), node);
        self.start(join);
    }

    /// `d.copy()` into `place`: a new `dict` with a copy of each key and value, in order. Keys
    /// are distinct, so each is added without looking it up first.
    ///
    /// Cost: one pass over the entries, and the new table's amortized growth.
    fn dict_copy(&mut self, place: &Place, dict: NodeId, node: NodeId) {
        let ty = self.ty(dict);
        let Some((key, value)) = self.copyable_parts(ty, "copy", node) else {
            return;
        };
        let base = self.borrow(dict);
        self.assign(place.clone(), Rvalue::Aggregate(ty, Vec::new()), node);
        let int = self.int_type();
        self.each_entry(&base, node, &mut |builder, position| {
            let key_copy = builder.copied(entry(base.clone(), position, 0), key, node);
            let value_copy = builder.copied(entry(base.clone(), position, 1), value, node);
            let added = builder.temporary(int, node);
            builder.then(node, |next| TerminatorKind::Call {
                callee: Callee::Builtin(Builtin::DictInsert),
                arguments: vec![
                    Argument::InOut(place.clone()),
                    Argument::Owned(Operand::Move(key_copy)),
                ],
                destination: Place::from(added),
                target: next,
                error: None,
            });
            builder.assign(
                entry(place.clone(), added, 1),
                Rvalue::Use(Operand::Move(value_copy)),
                node,
            );
        });
    }

    /// `d.update(other)`: sets each key of `other` in `d` to a copy of its value, in `other`'s
    /// order, so a key already in `d` keeps its place and takes the new value, and a new key
    /// goes last, as in Python. `other` is borrowed; the checker rejects `d.update(d)`.
    ///
    /// Cost: one lookup per entry of `other`.
    fn dict_update(&mut self, dict: NodeId, other: NodeId, node: NodeId) {
        let ty = self.ty(dict);
        let Some((key, value)) = self.copyable_parts(ty, "update", node) else {
            return;
        };
        let Some(target) = self.place(dict) else {
            return;
        };
        let source = self.borrow(other);
        self.each_entry(&source, node, &mut |builder, position| {
            let key_copy = builder.copied(entry(source.clone(), position, 0), key, node);
            let owned = builder.temporary(key, node);
            builder.assign(
                Place::from(owned),
                Rvalue::Use(Operand::Move(key_copy)),
                node,
            );
            let value_copy = builder.copied(entry(source.clone(), position, 1), value, node);
            builder.set_entry(&target, owned, Operand::Move(value_copy), node);
            if builder.needs_drop(key) {
                // The key moved into a new entry, or is dropped here when the entry was there.
                builder.push(StatementKind::Drop(Place::from(owned)), node);
            }
        });
    }

    /// `d.setdefault(k, default)` into `place`: the value for `k`, after adding `k` with the
    /// value `default` when it is missing. The key and then the default are computed once,
    /// before the lookup, as Python passes them. The checker allows `setdefault` only on values
    /// that copy or are `str`, or tuples of them; an unused `str` default is dropped at the end
    /// of the statement.
    fn dict_setdefault(
        &mut self,
        place: Place,
        (dict, key, default): (NodeId, NodeId, NodeId),
        node: NodeId,
    ) {
        let ty = self.ty(dict);
        let Some((_, held)) = self.dict_types(ty, node) else {
            return;
        };
        let Some(base) = self.place(dict) else {
            return;
        };
        let owned = self.stored_key(ty, key);
        let value = self.owned_value(held, default);
        let position = self.lookup(&base, &Place::from(owned), node);
        let (found, missing) = (self.new_block(), self.new_block());
        let present = self.at_least_zero(position, node);
        self.branch(present, (found, missing), node);
        self.start(missing);
        self.then(node, |next| TerminatorKind::Call {
            callee: Callee::Builtin(Builtin::DictInsert),
            arguments: vec![
                Argument::InOut(base.clone()),
                Argument::Owned(Operand::Move(Place::from(owned))),
            ],
            destination: Place::from(position),
            target: next,
            error: None,
        });
        self.assign(entry(base.clone(), position, 1), Rvalue::Use(value), node);
        self.terminate(TerminatorKind::Goto(found), node);
        self.start(found);
        let copy = self.key_copy(entry(base, position, 1), held, node);
        self.assign(place, Rvalue::Use(copy), node);
    }

    /// `d.clear()`: drops every key and value and leaves `d` empty, as a new empty `dict`
    /// replacing the old one.
    fn dict_clear(&mut self, dict: NodeId, node: NodeId) {
        let ty = self.ty(dict);
        let Some(base) = self.place(dict) else {
            return;
        };
        let empty = self.temporary(ty, node);
        self.assign(Place::from(empty), Rvalue::Aggregate(ty, Vec::new()), node);
        self.replace(base, Operand::Move(Place::from(empty)), node);
    }

    /// The key and value types of the `dict` type `ty`, when both are copy types or `str`, which
    /// `method` copies; otherwise reports `method` as not built for them.
    fn copyable_parts(
        &mut self,
        ty: Type<'db>,
        method: &str,
        node: NodeId,
    ) -> Option<(Type<'db>, Type<'db>)> {
        let (key, value) = self.dict_types(ty, node)?;
        for part in [key, value] {
            if !self.is_copy(part) && !matches!(part.kind(self.db), TypeKind::Str) {
                let shown = part.display(self.db);
                self.unsupported(node, &format!("`{method}` of a dict of `{shown}`"));
                return None;
            }
        }
        Some((key, value))
    }

    /// Runs `visit` with the position of each entry of the `dict` at `dict`, in insertion order.
    /// The `dict` must not change while it runs.
    fn each_entry(&mut self, dict: &Place, node: NodeId, visit: &mut dyn FnMut(&mut Self, Local)) {
        let int = self.int_type();
        let position = self.temporary(int, node);
        self.next_entry(
            dict,
            position,
            Operand::Constant(Constant::integer(int, 0)),
            node,
        );
        let (header, body, end) = (self.new_block(), self.new_block(), self.new_block());
        self.terminate(TerminatorKind::Goto(header), node);
        self.start(header);
        let more = self.at_least_zero(position, node);
        self.branch(more, (body, end), node);
        self.start(body);
        visit(self, position);
        let after = Rvalue::Binary(
            BinaryOperator::Add,
            Operand::Copy(Place::from(position)),
            Operand::Constant(Constant::integer(int, 1)),
        );
        let from = self.temporary(int, node);
        self.assign(Place::from(from), after, node);
        self.next_entry(dict, position, Operand::Copy(Place::from(from)), node);
        self.terminate(TerminatorKind::Goto(header), node);
        self.start(end);
    }

    /// Moves the value of the entry at `position` out into `place` and removes the entry.
    pub(super) fn remove(&mut self, dict: &Place, position: Local, place: Place, node: NodeId) {
        self.then(node, |next| TerminatorKind::Call {
            callee: Callee::Builtin(Builtin::DictRemove),
            arguments: vec![
                Argument::InOut(dict.clone()),
                Argument::Owned(Operand::Copy(Place::from(position))),
            ],
            destination: place,
            target: next,
            error: None,
        });
    }
}
