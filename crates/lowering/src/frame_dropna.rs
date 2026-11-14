//! `df.dropna()` of a frame (decision 0045, part 3, roadmap 3.13.6): the rows that have a
//! value where it is asked for, chosen with one mask made from the validity bitmaps of the
//! columns looked at.
//!
//! `subset=` names the columns looked at, every column when it is left out. `how="any"`,
//! the default, drops a row with a `None` in any of them, so those columns hold no `None`
//! in the result and are stored as plain arrays. `how="all"` drops a row only when all of
//! them are `None`, so every column keeps its type. The checker's type of the result says
//! which columns lost their `None`.

use crate::builder::Builder;
use crate::optional_arrays::{VALIDITY, VALUES};
use mir::{Local, Operand, Place, Projection, Rvalue};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Column, Type, TypeKind};

impl<'db> Builder<'db> {
    /// `df.dropna()`, the call `node` on the frame `frame`, into `place`: a new frame of
    /// the rows kept. A call that can drop no row copies the frame: one that looks at no
    /// column that may hold `None`, or `how="all"` with a column that cannot.
    ///
    /// Cost: one mask of a byte for each row for each column looked at that may hold
    /// `None`, all dropped when the statement ends, then one pass over each column that
    /// copies the rows kept.
    pub(crate) fn frame_dropna(&mut self, place: Place, frame: NodeId, node: NodeId) {
        let (Some(columns), Some(results)) = (self.frame_columns(frame), self.frame_columns(node))
        else {
            return;
        };
        let (all, subset) = self.dropna_arguments(node);
        let looked_at: Vec<(u32, &Column<'db>)> = (0_u32..)
            .zip(columns)
            .filter(|(_, column)| {
                subset
                    .as_ref()
                    .is_none_or(|names| names.contains(&column.name.as_str()))
            })
            .collect();
        let gaps: Vec<u32> = looked_at
            .iter()
            .filter(|(_, column)| self.gap_element(column.element).is_some())
            .map(|&(position, _)| position)
            .collect();
        if gaps.is_empty() || (all && gaps.len() != looked_at.len()) {
            return self.frame_copy(place, frame, node);
        }
        let source = self.borrow(frame);
        let count = self.counted_rows(frame, &source, node);
        let Some(mask) = self.kept_rows(&source, &gaps, count, all, node) else {
            return;
        };
        let mut chosen = Vec::with_capacity(columns.len());
        for ((position, column), result) in (0_u32..).zip(columns).zip(results) {
            let from = source.clone().project(Projection::Field(position));
            let by = Place::from(mask);
            let kept = match self.gap_element(column.element) {
                // The column lost its `None`: only its values are kept, as a plain array.
                Some(element) if column.element != result.element => {
                    let values = from.project(Projection::Field(VALUES));
                    self.values_call("masked", element, vec![values, by], node)
                        .map(|values| Operand::Move(Place::from(values)))
                }
                _ => self.chosen_column(from, column, ("masked", &by), node),
            };
            let Some(kept) = kept else {
                return;
            };
            chosen.push(kept);
        }
        self.assign(place, Rvalue::Aggregate(self.ty(node), chosen), node);
    }

    /// The `T` of a column whose elements are `T | None`, and `None` for any other column.
    fn gap_element(&self, element: Type<'db>) -> Option<Type<'db>> {
        match element.kind(self.db) {
            &TypeKind::Optional(element) => Some(element),
            _ => None,
        }
    }

    /// What the call `node` of `dropna` asks for: whether `how` is `"all"`, and the names
    /// `subset=` lists, one name or a list of them. The checker has checked both.
    fn dropna_arguments(&self, node: NodeId) -> (bool, Option<Vec<&'db str>>) {
        let (_, keywords) = self.call_arguments(node);
        let value = |name: &str| {
            keywords
                .iter()
                .find(|(keyword, _)| *keyword == name)
                .map(|&(_, value)| value)
        };
        let all = value("how").is_some_and(|how| self.written_text(how) == "all");
        let subset = value("subset").map(|subset| {
            if self.tree.kind(subset) == SyntaxKind::List {
                self.tree
                    .children_with(subset, Role::Item)
                    .map(|item| self.written_text(item))
                    .collect()
            } else {
                vec![self.written_text(subset)]
            }
        });
        (all, subset)
    }

    /// The mask of the rows `dropna` keeps, for the frame stored at `source` with `count`
    /// rows: true where every one of the columns at the positions `gaps` has a value or,
    /// when `all`, where any of them has. It is dropped when the statement ends. `None`
    /// after a report.
    fn kept_rows(
        &mut self,
        source: &Place,
        gaps: &[u32],
        count: Local,
        all: bool,
        node: NodeId,
    ) -> Option<Local> {
        let mut mask: Option<Local> = None;
        for &position in gaps {
            let bits = source
                .clone()
                .project(Projection::Field(position))
                .project(Projection::Field(VALIDITY));
            let next = match mask {
                None => {
                    let count = Operand::Copy(Place::from(count));
                    self.validity_mask(bits, count, false, node)?
                }
                Some(mask) => {
                    let arguments = vec![Place::from(mask), bits];
                    let flags = self.array_of(self.boolean_type());
                    let joined = if all { "any_there" } else { "both" };
                    self.frame_call(joined, Vec::new(), arguments, flags, node)?
                }
            };
            self.drop_later(next);
            mask = Some(next);
        }
        mask
    }
}
