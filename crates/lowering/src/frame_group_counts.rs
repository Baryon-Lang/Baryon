//! The reductions of `agg` that count rather than combine (decision 0045, roadmap 3.13.17):
//! `size`, the number of rows of each group, and `nunique`, the number of different values
//! a group holds in a column, with `None` and NaN left out, as pandas counts them.
//!
//! Both read any column, whatever it holds, and give a column of `i64`. `size` needs only
//! the groups. `nunique` gives every row the number of its value among the column's values
//! ([`Builder::key_ids`], the numbering a join uses) and the library's `group_distinct`
//! counts the numbers each group holds.

use crate::builder::Builder;
use crate::frame_groups::Groups;
use crate::optional_arrays::{VALIDITY, VALUES};
use mir::{Constant, Operand, Place, Projection};
use syntax::NodeId;
use types::{Column, Primitive, TypeKind};

impl<'db> Builder<'db> {
    /// The place of a new array holding the number of rows of each group of `groups`, the
    /// `size` of `agg`: every row counts, one whose value is `None` or NaN too. The library's
    /// `group_integers` counts the rows of each group for any column, here the rows'
    /// positions. Cost: one pass over the groups.
    pub(crate) fn group_sizes(&mut self, groups: &Groups, node: NodeId) -> Option<Place> {
        let int = self.int_type();
        let positions = self.positions_type();
        let count = vec![Place::from(groups.count)];
        let rows = self.frame_call("row_positions", Vec::new(), count, positions, node)?;
        self.drop_later(rows);
        let how = Operand::Constant(Constant::integer(int, 5));
        let how = self.held(int, how, node);
        let arguments = vec![
            Place::from(rows),
            Place::from(groups.order),
            Place::from(groups.starts),
            Place::from(how),
        ];
        let sizes = self.frame_call("group_integers", Vec::new(), arguments, positions, node)?;
        Some(Place::from(sizes))
    }

    /// The place of a new array holding how many different values each group of `groups`
    /// holds in `read`, a column's place and its column: the `nunique` of `agg`. A `None`
    /// leaves its group first, as for any reduction of a column that may hold `None`, and
    /// `group_distinct` skips a NaN. Text is counted by the whole numbers `text_codes` gives
    /// it, and a `bool` as 0 or 1. Cost: a sort of the column to number its values, then
    /// one pass over the rows and an array of one number for each row.
    pub(crate) fn group_distinct(
        &mut self,
        groups: &Groups,
        (stored, column): (Place, &Column<'db>),
        node: NodeId,
    ) -> Option<Place> {
        let int = self.int_type();
        let positions = self.positions_type();
        let (values, element, there) = match column.element.kind(self.db) {
            &TypeKind::Optional(element) => {
                let bits = stored.clone().project(Projection::Field(VALIDITY));
                let there = self.present_groups(groups, bits, node)?;
                let values = stored.project(Projection::Field(VALUES));
                (values, element, (there.order, there.starts))
            }
            _ => (stored, column.element, (groups.order, groups.starts)),
        };
        let (values, compared) = match element.kind(self.db) {
            TypeKind::Str => (self.text_codes(values, node)?, int),
            TypeKind::Primitive(Primitive::Bool) => {
                (self.converted(values, element, int, node), int)
            }
            _ => (values, element),
        };
        let ids = self.key_ids(&[(values.clone(), compared)], node)?;
        let arguments = vec![
            values,
            Place::from(ids),
            Place::from(there.0),
            Place::from(there.1),
        ];
        let counts =
            self.frame_call("group_distinct", vec![compared], arguments, positions, node)?;
        Some(Place::from(counts))
    }

    /// The groups of `groups` without the rows whose validity bit in `bits` is clear: the
    /// rows where a column is `None` leave the groups (the library's `present_rows` and
    /// `present_starts`), so a group may be left with no rows. Cost: two passes over the
    /// rows.
    pub(crate) fn present_groups(
        &mut self,
        groups: &Groups,
        bits: Place,
        node: NodeId,
    ) -> Option<Groups> {
        let positions = self.positions_type();
        let arguments = vec![bits.clone(), Place::from(groups.order)];
        let order = self.frame_call("present_rows", Vec::new(), arguments, positions, node)?;
        self.drop_later(order);
        let arguments = vec![bits, Place::from(groups.order), Place::from(groups.starts)];
        let starts = self.frame_call("present_starts", Vec::new(), arguments, positions, node)?;
        self.drop_later(starts);
        Some(Groups {
            order,
            starts,
            count: groups.count,
            gaps: true,
        })
    }
}
