//! Stacking frames (decision 0045, roadmap 3.13.9): `concat([first, second])`, a new frame
//! holding the rows of the first frame, then those of the second, and so on.
//!
//! The checker has made sure the frames have one schema, so each column of the result is the
//! frames' columns of that name end to end, which the standard library's `joined` builds,
//! or `text_joined` for a column of `str` (`<std>/frame_joins.byn`, decision 0038). A column
//! that may hold `None` has its values and its validity bitmap put end to end apart, by
//! `stacked` and `stacked_bits` (`<std>/frame_missing.byn`). Every
//! frame is borrowed. Cost: the result's columns are copied once for each frame after the
//! first, so stacking many frames in one call copies the early ones several times.

use crate::builder::Builder;
use crate::frames::erase;
use mir::{Operand, Place, Projection, Rvalue};
use syntax::{NodeId, Role};
use types::{Primitive, Type, TypeKind};

impl<'db> Builder<'db> {
    /// The call `node`, `concat(list)` with `list` the frames written out, into `place`.
    pub(crate) fn frame_concat(&mut self, place: Place, list: NodeId, node: NodeId) {
        let ty = self.ty(node);
        let frames: Vec<NodeId> = self.tree.children_with(list, Role::Item).collect();
        let columns = frames.first().and_then(|&first| self.frame_columns(first));
        let Some(columns) = columns else {
            return self.unsupported(node, "this `concat`");
        };
        let sources: Vec<Place> = frames.iter().map(|&frame| self.borrow(frame)).collect();
        let int = self.int_type();
        let mut made = Vec::with_capacity(columns.len());
        for (position, column) in (0_u32..).zip(columns) {
            let element = column.element;
            let array = erase(self.db, column.array(self.db));
            let mut parts = sources
                .iter()
                .map(|source| source.clone().project(Projection::Field(position)));
            let Some(first) = parts.next() else {
                return self.unsupported(node, "this `concat`");
            };
            if sources.len() == 1 {
                made.push(self.column_copy(first, array, node));
                continue;
            }
            if matches!(element.kind(self.db), TypeKind::Str) {
                let Some(text) = self.text_stacked(first, parts.collect(), node) else {
                    return;
                };
                made.push(text);
                continue;
            }
            if let &TypeKind::Optional(inner) = element.kind(self.db) {
                let Some(whole) = self.optional_stacked(inner, first, parts.collect(), node) else {
                    return;
                };
                made.push(whole);
                continue;
            }
            // `joined` takes numbers, so a `bool` column is stacked as the integers 0 and 1.
            let flag = matches!(element.kind(self.db), TypeKind::Primitive(Primitive::Bool));
            let through = if flag { int } else { element };
            let stacked = self.array_of(through);
            let mut whole = self.converted(first, element, through, node);
            let mut newest = None;
            for part in parts {
                let part = self.converted(part, element, through, node);
                let Some(next) =
                    self.frame_call("joined", vec![through], vec![whole, part], stacked, node)
                else {
                    return;
                };
                // Every array but the last is only a step on the way.
                if let Some(step) = newest.replace(next) {
                    self.drop_later(step);
                }
                whole = Place::from(next);
            }
            let Some(mut result) = newest else {
                return self.unsupported(node, "this `concat`");
            };
            if flag {
                self.drop_later(result);
                result = self.converted_new(&Place::from(result), element, node);
            }
            made.push(Operand::Move(Place::from(result)));
        }
        self.assign(place, Rvalue::Aggregate(ty, made), node);
    }

    /// The text column `first` with each column of `rest` after it, in order, as a new text
    /// array: one call of the library's `text_joined` for each column of `rest`, which is
    /// not empty. `None` after an error.
    fn text_stacked(
        &mut self,
        first: Place,
        rest: Vec<Place>,
        node: NodeId,
    ) -> Option<Operand<'db>> {
        let text = erase(self.db, Type::new(self.db, TypeKind::TextArray));
        let mut whole = first;
        let mut newest = None;
        for part in rest {
            let next = self.frame_call("text_joined", Vec::new(), vec![whole, part], text, node)?;
            // Every array but the last is only a step on the way.
            if let Some(step) = newest.replace(next) {
                self.drop_later(step);
            }
            whole = Place::from(next);
        }
        newest.map(|result| Operand::Move(Place::from(result)))
    }
}
