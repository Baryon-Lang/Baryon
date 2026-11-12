//! Choosing and changing the columns of a frame (decision 0045, roadmap 3.13.4):
//! `df[["a", "b"]]`, `df.assign(name=values)`, `df.drop(columns=[...])` and
//! `df.rename(columns={...})`.
//!
//! The checker has worked out the columns of each result, so the work here is moving arrays
//! between two tuples. `df[["a", "b"]]` borrows the frame and copies the columns named.
//! The three methods take the frame: every column that stays moves into the result, which
//! costs nothing however long the columns are, and a column left out or replaced is freed
//! when the statement ends.

use crate::builder::Builder;
use mir::{Local, Operand, Place, Projection, Rvalue};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Type, TypeKind};

impl Builder<'_> {
    /// `df[["a", "b"]]`, the subscript `node` of the frame `value`, into `place`: a new frame
    /// holding a copy of each column the list names, in the order written. The cost is one
    /// copy of those columns.
    pub(crate) fn frame_select(&mut self, place: Place, value: NodeId, node: NodeId) {
        let ty = self.ty(node);
        let (Some(columns), Some(picked)) = (self.frame_columns(value), self.frame_columns(node))
        else {
            return self.unsupported(node, "these columns of a `DataFrame`");
        };
        let source = self.borrow(value);
        let mut copies = Vec::with_capacity(picked.len());
        for column in picked {
            let Some(position) = position(columns.iter().map(|from| &from.name), &column.name)
            else {
                return self.unsupported(node, "this column of a `DataFrame`");
            };
            let element = column.element;
            let array = Type::new(self.db, TypeKind::Array { element, rank: 1 });
            let from = source.clone().project(Projection::Field(position));
            let copy = self.copy_view(from, array, node);
            copies.push(Operand::Move(Place::from(copy)));
        }
        self.assign(place, Rvalue::Aggregate(ty, copies), node);
    }

    /// The call `node` of `assign`, `drop` or `rename` on the frame `frame`, into `place`. `false`, with nothing lowered, for any other method.
    ///
    /// The arguments are computed before the frame is taken, since
    /// `df.assign(total=df.a + df.b)` reads the frame it then moves.
    pub(crate) fn schema_method(
        &mut self,
        place: &Place,
        method: &str,
        frame: NodeId,
        node: NodeId,
    ) -> bool {
        match method {
            // The tuple that stores the frame does not hold the names, so a renamed frame is
            // the same value under another type.
            "rename" => {
                let moved = self.operand(frame);
                self.assign(place.clone(), Rvalue::Use(moved), node);
            }
            "assign" => self.frame_rebuilt(place, frame, true, node),
            "drop" => self.frame_rebuilt(place, frame, false, node),
            _ => return false,
        }
        true
    }

    /// `df.assign(...)`, when `assigns`, or `df.drop(...)`, the call `node` on the frame
    /// `frame`, into `place`: the frame's columns that the result keeps move into it, and
    /// each column `assign` gives moves in after a check that it has the frame's number of
    /// rows, so the columns keep one length. A column of another length panics at its
    /// argument. The one argument of `drop` names columns and computes nothing.
    fn frame_rebuilt(&mut self, place: &Place, frame: NodeId, assigns: bool, node: NodeId) {
        let ty = self.ty(node);
        let (Some(columns), Some(kept)) = (self.frame_columns(frame), self.frame_columns(node))
        else {
            return self.unsupported(node, "this method of a `DataFrame`");
        };
        let mut given: Vec<(&str, NodeId, Local)> = Vec::new();
        let written: Vec<NodeId> = self
            .tree
            .child(node, Role::Arguments)
            .into_iter()
            .flat_map(|list| self.tree.children_with(list, Role::Item))
            .filter(|&item| assigns && self.tree.kind(item) == SyntaxKind::Keyword)
            .collect();
        for argument in written {
            let name = self.tree.child(argument, Role::Name);
            let value = self.tree.child(argument, Role::Value);
            let (Some(name), Some(value)) = (name, value) else {
                continue;
            };
            let array = self.ty(value);
            let new = self.temporary(array, value);
            self.write(Place::from(new), array, value);
            given.push((self.text_of(name), value, new));
        }
        let held = self.temporary(self.ty(frame), frame);
        let moved = self.operand(frame);
        self.assign(Place::from(held), Rvalue::Use(moved), frame);
        self.drop_later(held);
        let rows = self.temporary(self.int_type(), node);
        let first = Place::from(held).project(Projection::Field(0));
        self.assign(Place::from(rows), Rvalue::Length(first, 0), node);
        for &(_, value, new) in &given {
            let length = self.temporary(self.int_type(), value);
            self.assign(
                Place::from(length),
                Rvalue::Length(Place::from(new), 0),
                value,
            );
            let expected = Operand::Copy(Place::from(rows));
            self.same_length(0, expected, Operand::Copy(Place::from(length)), value);
        }
        let mut result = Vec::with_capacity(kept.len());
        for column in kept {
            // The last value written for a name is the one pandas keeps.
            let new = given.iter().rev().find(|(name, ..)| *name == column.name);
            let from = if let Some(&(_, _, new)) = new {
                Place::from(new)
            } else {
                let Some(position) = position(columns.iter().map(|from| &from.name), &column.name)
                else {
                    return self.unsupported(node, "this column of a `DataFrame`");
                };
                Place::from(held).project(Projection::Field(position))
            };
            result.push(Operand::Move(from));
        }
        self.assign(place.clone(), Rvalue::Aggregate(ty, result), node);
    }
}

/// The position of `name` among `names`, as the number of a tuple's item.
fn position<'a>(mut names: impl Iterator<Item = &'a String>, name: &str) -> Option<u32> {
    names
        .position(|candidate| candidate == name)
        .and_then(|position| u32::try_from(position).ok())
}
