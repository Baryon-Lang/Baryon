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

use crate::builder::Builder;
use mir::{
    Argument, BinaryOperator, BlockId, Builtin, Callee, Check, Constant, ConstantValue, Local,
    Operand, Piece, Place, Projection, Rvalue, TerminatorKind,
};
use syntax::{NodeId, Operator, Role, SyntaxKind};
use types::{Declaration, FormatSpec, Item, Type, TypeKind};

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
        let key = self.owned_key(ty, key);
        self.set_entry(&place, key, value, target);
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

    /// Whether the assignment target `target` is `d[k]` on a `dict`, which
    /// [`dict_store`](Self::dict_store) sets, adding the key when it is missing.
    pub(crate) fn is_dict_subscript(&self, target: NodeId) -> bool {
        self.tree.kind(target) == SyntaxKind::Subscript
            && self
                .tree
                .child(target, Role::Value)
                .is_some_and(|dict| matches!(self.ty(dict).kind(self.db), TypeKind::Dict { .. }))
    }

    /// Whether `iterator` is a `dict`, or `d.keys()`, `d.values()` or `d.items()`, which a
    /// `for` loop goes over with [`dict_loop`](Self::dict_loop).
    pub(crate) fn is_dict_loop(&self, iterator: NodeId) -> bool {
        self.dict_view(iterator).is_some()
    }

    /// `for target in d`, or over `d.keys()`, `d.values()` or `d.items()`: steps through the
    /// entries in insertion order with [`Builtin::DictNext`], copying the key, the value or
    /// both into `target` each pass. The `dict` is borrowed for the whole loop, so the body
    /// cannot change it (the checker's loop rule), and the positions stay valid.
    ///
    /// Keys and values are copied, a `str` with a new string, so the loop variables never alias
    /// the table. Values of other types that do not copy are not built yet.
    pub(crate) fn dict_loop(&mut self, statement: NodeId, target: NodeId, iterator: NodeId) {
        let Some((dict, part)) = self.dict_view(iterator) else {
            return;
        };
        let ty = self.ty(dict);
        let Some((key, value)) = self.dict_types(ty, iterator) else {
            return;
        };
        let parts: Vec<(u32, Type<'db>)> = match part {
            View::Keys => vec![(0, key)],
            View::Values => vec![(1, value)],
            View::Items => vec![(0, key), (1, value)],
        };
        for &(_, part) in &parts {
            if !self.is_copy(part) && !matches!(part.kind(self.db), TypeKind::Str) {
                let shown = part.display(self.db);
                return self.unsupported(iterator, &format!("loops over a dict of `{shown}`"));
            }
        }
        let targets: Vec<NodeId> = if parts.len() == 2 {
            if self.tree.kind(target) != SyntaxKind::Tuple {
                return self.unsupported(target, "a loop over `items()` into one name");
            }
            self.tree.children_with(target, Role::Item).collect()
        } else {
            vec![target]
        };
        if targets.len() != parts.len() {
            return self.unsupported(target, "this loop target");
        }
        let base = self.borrow(dict);
        let int = self.int_type();
        let position = self.temporary(int, iterator);
        self.next_entry(
            &base,
            position,
            Operand::Constant(Constant::integer(int, 0)),
            iterator,
        );
        let header = self.new_block();
        self.terminate(TerminatorKind::Goto(header), statement);
        self.start(header);
        let (body, next, end) = (self.new_block(), self.new_block(), self.new_block());
        let more = self.at_least_zero(position, iterator);
        self.branch(more, (body, end), iterator);
        self.start(body);
        for (&target, &(field, part)) in targets.iter().zip(&parts) {
            let source = entry(base.clone(), position, field);
            let source = self.copied(source, part, iterator);
            self.assign_part(target, source, part);
        }
        self.repeat(statement, next, end);
        self.start(next);
        let after = Rvalue::Binary(
            BinaryOperator::Add,
            Operand::Copy(Place::from(position)),
            Operand::Constant(Constant::integer(int, 1)),
        );
        let from = self.temporary(int, iterator);
        self.assign(Place::from(from), after, iterator);
        self.next_entry(&base, position, Operand::Copy(Place::from(from)), iterator);
        self.terminate(TerminatorKind::Goto(header), statement);
        self.start(end);
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

    /// A new `str` holding the key at `key`, of type `ty`, as Python's `repr` writes it, for a
    /// missing key's panic: `'Ar'`, `3`, `(1, 2)`, `True` or `Phase.GAS`.
    fn key_text(&mut self, key: &Place, ty: Type<'db>, node: NodeId) -> Operand<'db> {
        let mut pieces = Vec::new();
        self.key_pieces(key, ty, &mut pieces, node);
        let text_type = Type::new(self.db, TypeKind::Str);
        let text = self.temporary(text_type, node);
        self.assign(Place::from(text), Rvalue::Format(pieces), node);
        Operand::Move(Place::from(text))
    }

    /// Appends the pieces that write the key part at `key`, of type `ty`, to `pieces`.
    fn key_pieces(&mut self, key: &Place, ty: Type<'db>, pieces: &mut Vec<Piece>, node: NodeId) {
        let text = |text: &str| Piece::Text(text.to_owned());
        match ty.kind(self.db) {
            TypeKind::Str => {
                pieces.push(text("'"));
                pieces.push(Piece::Value(key.clone(), FormatSpec::default()));
                pieces.push(text("'"));
            }
            TypeKind::Tuple(items) => {
                pieces.push(text("("));
                for (field, &item) in (0u32..).zip(items) {
                    if field > 0 {
                        pieces.push(text(", "));
                    }
                    let part = key.clone().project(Projection::Field(field));
                    self.key_pieces(&part, item, pieces, node);
                }
                pieces.push(text(if items.len() == 1 { ",)" } else { ")" }));
            }
            TypeKind::Enum(nominal) => {
                let name = self.enum_member_text(key, ty, nominal, node);
                pieces.push(Piece::Value(Place::from(name), FormatSpec::default()));
            }
            _ => pieces.push(Piece::Value(key.clone(), FormatSpec::default())),
        }
    }

    /// A `str` local holding `Class.MEMBER` for the enum value at `key`: a switch on the value
    /// to one block per member, each writing its name.
    fn enum_member_text(
        &mut self,
        key: &Place,
        ty: Type<'db>,
        nominal: &types::Nominal<'db>,
        node: NodeId,
    ) -> Local {
        let text_type = Type::new(self.db, TypeKind::Str);
        let name = self.temporary(text_type, node);
        let item = Item::new(self.db, self.package, nominal.file, nominal.name);
        let Declaration::Enum { variants } = &self.db.declaration(item).declaration else {
            return name;
        };
        let class = ty.display(self.db).to_string();
        let blocks: Vec<BlockId> = variants.iter().map(|_| self.new_block()).collect();
        let (otherwise, join) = (self.new_block(), self.new_block());
        self.terminate(
            TerminatorKind::Switch {
                value: Operand::Copy(key.clone()),
                cases: (0..).zip(blocks.iter().copied()).collect(),
                otherwise,
            },
            node,
        );
        self.start(otherwise);
        self.terminate(TerminatorKind::Unreachable, node);
        for (variant, block) in variants.iter().zip(blocks) {
            self.start(block);
            let constant = Constant {
                ty: text_type,
                value: ConstantValue::Str(format!("{class}.{variant}")),
            };
            self.assign(
                Place::from(name),
                Rvalue::Use(Operand::Constant(constant)),
                node,
            );
            self.terminate(TerminatorKind::Goto(join), node);
        }
        self.start(join);
        name
    }

    /// The `dict` and the part a `for` loop over `iterator` reads: a `dict` itself gives its
    /// keys; `d.keys()`, `d.values()` and `d.items()` their part.
    fn dict_view(&self, iterator: NodeId) -> Option<(NodeId, View)> {
        let is_dict = |node: NodeId| matches!(self.ty(node).kind(self.db), TypeKind::Dict { .. });
        if is_dict(iterator) {
            return Some((iterator, View::Keys));
        }
        if self.tree.kind(iterator) != SyntaxKind::Call {
            return None;
        }
        let function = self.tree.child(iterator, Role::Function)?;
        let dict = self.tree.child(function, Role::Value)?;
        let attribute = self.tree.child(function, Role::Attribute)?;
        if self.tree.kind(function) != SyntaxKind::Attribute || !is_dict(dict) {
            return None;
        }
        let view = match self.text_of(attribute) {
            "keys" => View::Keys,
            "values" => View::Values,
            "items" => View::Items,
            _ => return None,
        };
        Some((dict, view))
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

/// Which part of each entry a loop over a `dict` reads.
#[derive(Clone, Copy)]
enum View {
    /// The keys: `for k in d` and `for k in d.keys()`.
    Keys,
    /// The values: `for v in d.values()`.
    Values,
    /// Both, as a pair: `for k, v in d.items()`.
    Items,
}

/// Field `field` (0 the key, 1 the value) of the entry at the position in `position` of the
/// `dict` at `dict`.
fn entry(dict: Place, position: Local, field: u32) -> Place {
    dict.project(Projection::Index(vec![position]))
        .project(Projection::Field(field))
}
