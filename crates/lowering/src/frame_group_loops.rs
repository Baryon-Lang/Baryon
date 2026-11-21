//! The loop over a grouping (decision 0045's amendment of 2026-11-19, point 5; roadmap
//! 3.13.25): `for key, group in df.groupby("k"):`.
//!
//! The rows are sorted into groups once, before the loop, as for `agg` (the `frame_groups`
//! module). Each pass then builds a pair in one temporary: the group's key, read from the
//! group's first row, and a new frame of the group's rows, each column copied by the
//! library's `taken` from the positions `group_members` gives. The pair is dropped when the
//! next pass replaces it, and the last when the loop's statement ends; the loop's variables
//! borrow its parts. Cost: the grouping's sort, then one copy of each group's rows.

use crate::builder::Builder;
use mir::{Local, Operand, Place, Projection, Rvalue, StatementKind};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Column, Type, TypeKind};

/// The groups of a frame that a `for` loop over `df.groupby(...)` walks.
pub(crate) struct GroupWalk<'db> {
    /// The borrowed frame.
    frame: Place,
    /// The frame's columns.
    columns: &'db [Column<'db>],
    /// The key columns, each with its position in the frame.
    keys: Vec<(u32, &'db Column<'db>)>,
    /// Whether the keys were written as a list, so the key of a group is a tuple.
    listed: bool,
    /// The local holding the rows' positions, sorted by the keys.
    order: Local,
    /// The local holding where each group starts in `order`.
    starts: Local,
    /// The local holding the position of the first row of each group, in the order the
    /// loop visits the groups.
    firsts: Local,
    /// With `sort=False`, the place of the array that says which sorted group each pass
    /// visits.
    turn: Option<Place>,
    /// The local holding the number of groups.
    pub(crate) length: Local,
    /// The type of the pair a pass sees: the key, then the frame as it is stored.
    pub(crate) element: Type<'db>,
}

impl<'db> Builder<'db> {
    /// The frame `df` when `node` is the call `df.groupby(...)` on a frame, whose groups a
    /// `for` loop walks; `None` for any other expression.
    pub(crate) fn grouping_frame(&self, node: NodeId) -> Option<NodeId> {
        if self.tree.kind(node) != SyntaxKind::Call {
            return None;
        }
        let function = self.tree.child(node, Role::Function)?;
        let name = self.tree.child(function, Role::Attribute)?;
        let frame = self.tree.child(function, Role::Value)?;
        let is_frame = matches!(
            self.typed.expression(frame)?.kind(self.db),
            TypeKind::DataFrame { .. }
        );
        (is_frame && self.text_of(name) == "groupby").then_some(frame)
    }

    /// The groups a loop over the call `node`, `df.groupby(...)`, walks: the frame computed
    /// once and borrowed for the loop, its rows sorted into groups, and the groups counted.
    /// `None` after a report.
    pub(crate) fn group_walk(&mut self, node: NodeId) -> Option<GroupWalk<'db>> {
        let value = self.grouping_frame(node)?;
        let columns = self.frame_columns(value)?;
        let (by, sorted) = self.grouping_arguments(node)?;
        let listed = self.tree.kind(by) == SyntaxKind::List;
        let frame = self.borrow(value);
        let mut keys = Vec::new();
        let mut places = Vec::new();
        for key in self.listed(by) {
            let (position, column) = self.named_column(columns, key)?;
            places.push((frame.clone().project(Projection::Field(position)), column));
            keys.push((position, column));
        }
        let count = self.counted_rows(value, &frame, node);
        let groups = self.groups(count, &places, node)?;
        let positions = self.positions_type();
        let arguments = vec![Place::from(groups.order), Place::from(groups.starts)];
        let firsts = self.frame_call("group_keys", Vec::new(), arguments, positions, node)?;
        self.drop_later(firsts);
        let (firsts, turn) = self.first_seen(firsts, sorted, node)?;
        let int = self.int_type();
        let length = self.temporary(int, node);
        let counted = Rvalue::Length(Place::from(firsts), 0);
        self.assign(Place::from(length), counted, node);
        let key = match keys.as_slice() {
            [(_, key)] if !listed => key.element,
            keys => {
                let items = keys.iter().map(|(_, key)| key.element).collect();
                Type::new(self.db, TypeKind::Tuple(items))
            }
        };
        let element = Type::new(self.db, TypeKind::Tuple(vec![key, self.ty(value)]));
        Some(GroupWalk {
            frame,
            columns,
            keys,
            listed,
            order: groups.order,
            starts: groups.starts,
            firsts,
            turn,
            length,
            element,
        })
    }

    /// A temporary holding what pass `counter` of a loop over `walk` sees: the key of the
    /// group and a new frame of its rows. `counter` is a pass the loop has counted, so it
    /// names a group.
    pub(crate) fn group_at(
        &mut self,
        walk: &GroupWalk<'db>,
        counter: Local,
        node: NodeId,
    ) -> Place {
        let int = self.int_type();
        let value = self.temporary(walk.element, node);
        // The pair owns the frame's arrays, and text when a key is one: it is dropped when
        // the next pair replaces it, and the last pair when the loop's statement ends.
        self.push(StatementKind::Drop(Place::from(value)), node);
        self.drop_later(value);
        let TypeKind::Tuple(parts) = walk.element.kind(self.db) else {
            return Place::from(value);
        };
        let (Some(&key_type), Some(&frame_type)) = (parts.first(), parts.get(1)) else {
            return Place::from(value);
        };
        let pass = vec![counter];
        let group = match &walk.turn {
            Some(turn) => {
                let turned = Operand::Copy(turn.clone().project(Projection::Index(pass.clone())));
                self.held(int, turned, node)
            }
            None => counter,
        };
        let first = Place::from(walk.firsts).project(Projection::Index(pass));
        let first = self.held(int, Operand::Copy(first), node);
        let mut key: Vec<Operand<'db>> = Vec::with_capacity(walk.keys.len());
        for &(position, column) in &walk.keys {
            let values = walk.frame.clone().project(Projection::Field(position));
            key.push(self.row_field(values, column, first, node));
        }
        let key = match (key.pop(), walk.listed || !key.is_empty()) {
            (Some(last), true) => {
                key.push(last);
                let tuple = self.temporary(key_type, node);
                self.assign(Place::from(tuple), Rvalue::Aggregate(key_type, key), node);
                Operand::Move(Place::from(tuple))
            }
            (Some(only), false) => only,
            (None, _) => return Place::from(value),
        };
        let arguments = vec![
            Place::from(walk.order),
            Place::from(walk.starts),
            Place::from(group),
        ];
        let positions = self.positions_type();
        let members = self.frame_call("group_members", Vec::new(), arguments, positions, node);
        let Some(members) = members else {
            return Place::from(value);
        };
        let rows = Place::from(members);
        let mut taken = Vec::with_capacity(walk.columns.len());
        for (position, column) in (0_u32..).zip(walk.columns) {
            let from = walk.frame.clone().project(Projection::Field(position));
            match self.chosen_column(from, column, ("taken", &rows), node) {
                Some(column) => taken.push(column),
                None => return Place::from(value),
            }
        }
        self.push(StatementKind::Drop(rows), node);
        let frame = self.temporary(frame_type, node);
        self.assign(
            Place::from(frame),
            Rvalue::Aggregate(frame_type, taken),
            node,
        );
        let pair = vec![key, Operand::Move(Place::from(frame))];
        self.assign(
            Place::from(value),
            Rvalue::Aggregate(walk.element, pair),
            node,
        );
        Place::from(value)
    }
}
