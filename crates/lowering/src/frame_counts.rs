//! Counting the rows of a frame that hold each value (decision 0045, roadmap 3.13.9):
//! `df.value_counts("symbol")` and `df.value_counts(["symbol", "day"])`, a new frame with one
//! row for each distinct value of the columns named, those columns first and then `count`,
//! the number of rows that hold it, the largest count first.
//!
//! The work is Baryon code in the standard library's `<std>/frame_joins.byn` (decision
//! 0038). The rows are numbered by their values as `drop_duplicates` numbers them,
//! `first_rows` finds where each value first stands, `valued_rows` leaves out a value with
//! a NaN, `id_counts` counts the rows of each, and the stable sort puts the counts in
//! falling order, so values with one count stay in the order they first appear, as in
//! pandas. Cost for `n` rows: `n log n` comparisons for each column named, and a copy of
//! the distinct values.

use crate::builder::Builder;
use mir::{Constant, ConstantValue, Operand, Place, Projection, Rvalue};
use syntax::NodeId;
use types::{Primitive, TypeKind};

impl Builder<'_> {
    /// The call `node` of `value_counts` on the frame `frame`, into `place`. The checker has
    /// made sure of the columns named and that none is called `count`.
    pub(crate) fn frame_counts(&mut self, place: Place, frame: NodeId, node: NodeId) {
        let ty = self.ty(node);
        let (Some(columns), Some((source, compared, keys))) = (
            self.frame_columns(frame),
            self.compared_columns(frame, node),
        ) else {
            return self.unsupported(node, "this `value_counts`");
        };
        let (int, boolean) = (self.int_type(), self.boolean_type());
        let positions = self.positions_type();
        let call = |builder: &mut Self, function: &str, types, arguments: Vec<Place>| {
            let result = builder.frame_call(function, types, arguments, positions, node)?;
            builder.drop_later(result);
            Some(Place::from(result))
        };
        let Some(ids) = self.key_ids(&keys, node) else {
            return;
        };
        let ids = Place::from(ids);
        let Some(mut rows) = call(self, "first_rows", Vec::new(), vec![ids.clone()]) else {
            return;
        };
        // A value with a NaN in it is not counted, as in pandas.
        for (key, element) in &keys {
            if matches!(
                element.kind(self.db),
                TypeKind::Primitive(Primitive::Float(_))
            ) {
                let arguments = vec![key.clone(), rows];
                let Some(kept) = call(self, "valued_rows", vec![*element], arguments) else {
                    return;
                };
                rows = kept;
            }
        }
        let Some(counts) = call(self, "id_counts", Vec::new(), vec![ids, rows.clone()]) else {
            return;
        };
        let distinct = self.temporary(int, node);
        self.assign(Place::from(distinct), Rvalue::Length(rows.clone(), 0), node);
        let falling = Operand::Constant(Constant {
            ty: boolean,
            value: ConstantValue::Bool(false),
        });
        let falling = Place::from(self.held(boolean, falling, node));
        let Some(start) = call(
            self,
            "row_positions",
            Vec::new(),
            vec![Place::from(distinct)],
        ) else {
            return;
        };
        let sorted = vec![counts.clone(), start, falling];
        let Some(order) = call(self, "ordered", vec![int], sorted) else {
            return;
        };
        let Some(rows) = call(self, "taken", vec![int], vec![rows, order.clone()]) else {
            return;
        };
        let mut made = Vec::with_capacity(compared.len() + 1);
        for position in compared {
            let Some(column) = usize::try_from(position)
                .ok()
                .and_then(|at| columns.get(at))
            else {
                return self.unsupported(node, "this `value_counts`");
            };
            let from = source.clone().project(Projection::Field(position));
            let Some(taken) = self.taken_column(from, column, rows.clone(), node) else {
                return;
            };
            made.push(Operand::Move(Place::from(taken)));
        }
        let arguments = vec![counts, order];
        let Some(count) = self.frame_call("taken", vec![int], arguments, positions, node) else {
            return;
        };
        made.push(Operand::Move(Place::from(count)));
        self.assign(place, Rvalue::Aggregate(ty, made), node);
    }
}
