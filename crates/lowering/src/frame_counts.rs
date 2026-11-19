//! Counting the rows of a frame that hold each value (decision 0045, roadmap 3.13.9):
//! `df.value_counts("symbol")` and `df.value_counts(["symbol", "day"])`, a new frame with one
//! row for each distinct value of the columns named, those columns first and then `count`,
//! the number of rows that hold it, the largest count first.
//!
//! The work is Baryon code in the standard library's `<std>/frame_joins.byn` (decision
//! 0038). The rows are numbered by their values as `drop_duplicates` numbers them,
//! `first_rows` finds where each value first stands, `valued_rows` leaves out a value with
//! a NaN or a `None`, `id_counts` counts the rows of each, and the stable sort puts the counts in
//! falling order, so values with one count stay in the order they first appear, as in
//! pandas. Cost for `n` rows: `n log n` comparisons for each column named, and a copy of
//! the distinct values.
//!
//! Four options, written as `True` or `False`, change the steps (roadmap 3.13.16):
//! `dropna=False` skips `valued_rows`, so a missing value is counted like any other;
//! `sort=False` skips the sort; `ascending=True` sorts the counts rising; and
//! `normalize=True` ends with the library's `proportions`, each count as a part of their
//! sum, in a column named `proportion`.

use crate::builder::Builder;
use crate::frame_distinct::Key;
use crate::optional_arrays::VALIDITY;
use mir::{Constant, ConstantValue, Operand, Place, Projection, Rvalue};
use syntax::{NodeId, SyntaxKind};
use types::{Column, Float, Primitive, Type, TypeKind};

impl<'db> Builder<'db> {
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
        let flag = |builder: &Self, name: &str, default: bool| {
            builder.keyword_of(node, name).map_or(default, |value| {
                builder.tree.kind(value) == SyntaxKind::True
            })
        };
        if flag(self, "dropna", true) {
            let Some(kept) = self.valued_keys(rows, &source, (&compared, &keys), columns, node)
            else {
                return;
            };
            rows = kept;
        }
        let Some(counts) = call(self, "id_counts", Vec::new(), vec![ids, rows.clone()]) else {
            return;
        };
        let distinct = self.temporary(int, node);
        self.assign(Place::from(distinct), Rvalue::Length(rows.clone(), 0), node);
        let rising = Operand::Constant(Constant {
            ty: boolean,
            value: ConstantValue::Bool(flag(self, "ascending", false)),
        });
        let rising = Place::from(self.held(boolean, rising, node));
        let Some(start) = call(
            self,
            "row_positions",
            Vec::new(),
            vec![Place::from(distinct)],
        ) else {
            return;
        };
        // Unsorted, the values stay in the order they first appear.
        let order = if flag(self, "sort", true) {
            let sorted = vec![counts.clone(), start, rising];
            let Some(order) = call(self, "ordered", vec![int], sorted) else {
                return;
            };
            order
        } else {
            start
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
            let Some(taken) = self.chosen_column(from, column, ("taken", &rows), node) else {
                return;
            };
            made.push(taken);
        }
        let arguments = vec![counts, order];
        let Some(count) = self.frame_call("taken", vec![int], arguments, positions, node) else {
            return;
        };
        let mut last = count;
        if flag(self, "normalize", false) {
            self.drop_later(count);
            let float = Type::new(self.db, TypeKind::Primitive(Primitive::Float(Float::F64)));
            let parts = self.array_of(float);
            let arguments = vec![Place::from(count)];
            let Some(shares) = self.frame_call("proportions", Vec::new(), arguments, parts, node)
            else {
                return;
            };
            last = shares;
        }
        made.push(Operand::Move(Place::from(last)));
        self.assign(place, Rvalue::Aggregate(ty, made), node);
    }

    /// The positions `rows`, one row of each distinct value of a `value_counts`, without
    /// those whose value has a NaN or a `None` in it, which pandas does not count unless
    /// `dropna=False`. `source` is the frame borrowed, and `compared` are the positions of
    /// its columns counted by and the keys made of them, as
    /// [`Builder::compared_columns`] gives them, and `columns` all its columns. The
    /// positions are dropped when the statement ends. `None` after a report.
    fn valued_keys(
        &mut self,
        rows: Place,
        source: &Place,
        (compared, keys): (&[u32], &[Key<'db>]),
        columns: &[Column<'db>],
        node: NodeId,
    ) -> Option<Place> {
        let positions = self.positions_type();
        let mut rows = rows;
        for (key, element) in keys {
            if matches!(
                element.kind(self.db),
                TypeKind::Primitive(Primitive::Float(_))
            ) {
                let arguments = vec![key.clone(), rows];
                let kept =
                    self.frame_call("valued_rows", vec![*element], arguments, positions, node)?;
                self.drop_later(kept);
                rows = Place::from(kept);
            }
        }
        for (&position, column) in compared.iter().zip(
            compared
                .iter()
                .filter_map(|&at| columns.get(usize::try_from(at).ok()?)),
        ) {
            if matches!(column.element.kind(self.db), TypeKind::Optional(_)) {
                let stored = source.clone().project(Projection::Field(position));
                let arguments = vec![stored.project(Projection::Field(VALIDITY)), rows];
                let kept =
                    self.frame_call("present_rows", Vec::new(), arguments, positions, node)?;
                self.drop_later(kept);
                rows = Place::from(kept);
            }
        }
        Some(rows)
    }
}
