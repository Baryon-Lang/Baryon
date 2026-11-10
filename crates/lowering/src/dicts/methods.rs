//! The methods of a `dict` and `in`: `get`, `pop`, `copy`, and `k in d`.

use super::entry;
use crate::builder::Builder;
use mir::{
    Argument, BinaryOperator, Builtin, Callee, Constant, ConstantValue, Local, Operand, Place,
    Rvalue, TerminatorKind,
};
use syntax::{NodeId, Operator};
use types::TypeKind;

impl Builder<'_> {
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

    /// A method of a `dict`, `d.get(k)`, `d.pop(k, default)` or `d.copy()`, with `arguments`,
    /// the receiver first, into `place`.
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
            _ => self.unsupported(node, &format!("the dict method `{method}`")),
        }
    }

    /// `d.get(k)` or `d.get(k, default)` into `place`: the value copied out as a `V | None`, or
    /// the value or the default. The checker allows `get` only on values that copy.
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
        let value = Operand::Copy(entry(base, position, 1));
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
        let Some((key, value)) = self.dict_types(ty, node) else {
            return;
        };
        for part in [key, value] {
            if !self.is_copy(part) && !matches!(part.kind(self.db), TypeKind::Str) {
                let shown = part.display(self.db);
                return self.unsupported(node, &format!("`copy` of a dict of `{shown}`"));
            }
        }
        let base = self.borrow(dict);
        self.assign(place.clone(), Rvalue::Aggregate(ty, Vec::new()), node);
        let int = self.int_type();
        let position = self.temporary(int, node);
        self.next_entry(
            &base,
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
        let key_copy = self.copied(entry(base.clone(), position, 0), key, node);
        let value_copy = self.copied(entry(base.clone(), position, 1), value, node);
        let added = self.temporary(int, node);
        self.then(node, |next| TerminatorKind::Call {
            callee: Callee::Builtin(Builtin::DictInsert),
            arguments: vec![
                Argument::InOut(place.clone()),
                Argument::Owned(Operand::Move(key_copy)),
            ],
            destination: Place::from(added),
            target: next,
            error: None,
        });
        self.assign(
            entry(place.clone(), added, 1),
            Rvalue::Use(Operand::Move(value_copy)),
            node,
        );
        let after = Rvalue::Binary(
            BinaryOperator::Add,
            Operand::Copy(Place::from(position)),
            Operand::Constant(Constant::integer(int, 1)),
        );
        let from = self.temporary(int, node);
        self.assign(Place::from(from), after, node);
        self.next_entry(&base, position, Operand::Copy(Place::from(from)), node);
        self.terminate(TerminatorKind::Goto(header), node);
        self.start(end);
    }

    /// Moves the value of the entry at `position` out into `place` and removes the entry.
    fn remove(&mut self, dict: &Place, position: Local, place: Place, node: NodeId) {
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
