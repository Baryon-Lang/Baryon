//! Leaving out the rows of a frame that repeat (decision 0045, roadmap 3.13.9):
//! `df.drop_duplicates()` and `df.drop_duplicates(subset=[...])`, a new frame with the
//! first of every set of rows that hold the same values in the columns compared.
//!
//! The work is Baryon code in the standard library's `<std>/frame_joins.byn` (decision
//! 0038). Each row gets the number of its key the way a join numbers them, `first_rows`
//! keeps the first row of each number, and `taken` copies every column at those rows. The
//! frame is borrowed. Cost for `n` rows: `n log n` comparisons for each column compared,
//! and one copy of the rows kept.

use crate::builder::Builder;
use mir::{Place, Projection};
use syntax::{NodeId, SyntaxKind};
use types::{Primitive, Type, TypeKind};

impl<'db> Builder<'db> {
    /// The call `node` of `drop_duplicates` on the frame `frame`, into `place`. The checker
    /// has made sure of the columns `subset` names.
    pub(crate) fn frame_distinct(&mut self, place: Place, frame: NodeId, node: NodeId) {
        let Some(columns) = self.frame_columns(frame) else {
            return self.unsupported(node, "this `drop_duplicates`");
        };
        // The columns compared: the ones `subset` names, by position or keyword, or all.
        let subset = self.written(node).first().map(|&argument| {
            if self.tree.kind(argument) == SyntaxKind::Keyword {
                self.keyword_value(argument)
            } else {
                argument
            }
        });
        let mut compared: Vec<u32> = Vec::new();
        match subset {
            Some(subset) => {
                for name in self.listed(subset) {
                    let Some((position, _)) = self.named_column(columns, name) else {
                        return self.unsupported(name, "this column to compare");
                    };
                    compared.push(position);
                }
            }
            None => compared.extend((0_u32..).zip(columns).map(|(position, _)| position)),
        }
        let source = self.borrow(frame);
        let int = self.int_type();
        // Each column compared, a `bool` column as the integers 0 and 1.
        let mut keys: Vec<(Place, Type<'db>)> = Vec::new();
        for (position, column) in (0_u32..).zip(columns) {
            if !compared.contains(&position) {
                continue;
            }
            let flag = matches!(
                column.element.kind(self.db),
                TypeKind::Primitive(Primitive::Bool)
            );
            let element = if flag { int } else { column.element };
            let values = source.clone().project(Projection::Field(position));
            let values = self.converted(values, column.element, element, node);
            keys.push((values, element));
        }
        if keys.is_empty() {
            return self.unsupported(node, "`drop_duplicates` of a frame with no columns");
        }
        let Some(ids) = self.key_ids(&keys, node) else {
            return;
        };
        let positions = self.positions_type();
        let arguments = vec![Place::from(ids)];
        let Some(rows) = self.frame_call("first_rows", Vec::new(), arguments, positions, node)
        else {
            return;
        };
        self.drop_later(rows);
        self.each_column(place, frame, &source, ("taken", &Place::from(rows)), node);
    }
}
