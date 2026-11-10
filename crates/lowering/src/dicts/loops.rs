//! `for` loops over a `dict`, `d.keys()`, `d.values()` or `d.items()`, in insertion order.

use super::entry;
use crate::builder::Builder;
use mir::{BinaryOperator, Constant, Operand, Place, Rvalue, TerminatorKind};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Type, TypeKind};

impl<'db> Builder<'db> {
    /// Whether `iterator` is a `dict`, or `d.keys()`, `d.values()` or `d.items()`, which a
    /// `for` loop goes over with [`dict_loop`](Self::dict_loop).
    pub(crate) fn is_dict_loop(&self, iterator: NodeId) -> bool {
        self.dict_view(iterator).is_some()
    }

    /// `for target in d`, or over `d.keys()`, `d.values()` or `d.items()`: steps through the
    /// entries in insertion order with [`Builtin::DictNext`](mir::Builtin::DictNext), copying the key, the value or
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
