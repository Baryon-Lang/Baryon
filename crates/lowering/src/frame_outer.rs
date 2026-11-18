//! The joins that keep rows without a partner (decision 0045, roadmap 3.13.8):
//! `left.merge(right, on=..., how="left")`, `"right"` and `"outer"`.
//!
//! [`Builder::frame_merge`] numbers the keys as it does for an inner join. The library's
//! `outer_pairs` (`<std>/frame_outer.byn`, decision 0038) then lists the pairs of rows, with
//! -1 for the frame that has no row for a pair, and this module builds the columns that such
//! pairs need: a column of the frame that may have no row becomes one that may hold `None`
//! ([`Builder::gap_column`]), and a key of an outer join takes its value from whichever
//! frame has the row ([`Builder::either_key`]).

use crate::builder::Builder;
use crate::optional_arrays::{VALIDITY, VALUES};
use mir::{Local, Operand, Place, Projection, Rvalue};
use syntax::{NodeId, SyntaxKind};
use types::{Column, Integer, Primitive, Type, TypeKind};

/// The join `how=` names, by the number the library's `outer_pairs` knows it by.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Join {
    /// Rows with a partner only.
    Inner = 0,
    /// Every row of the left frame.
    Left = 1,
    /// Every row of the right frame.
    Right = 2,
    /// Every row of both frames.
    Outer = 3,
}

impl Join {
    /// Whether the frame on `side`, 0 for the left one and 1 for the right, may have no row
    /// for a pair of this join, so its columns hold `None` there.
    pub(crate) fn leaves_gaps(self, side: i128) -> bool {
        match self {
            Join::Inner => false,
            Join::Left => side == 1,
            Join::Right => side == 0,
            Join::Outer => true,
        }
    }
}

impl<'db> Builder<'db> {
    /// The join the call `node` of `merge` asks for with `how=`, a string literal the checker
    /// has made sure of: the inner join when it is left out.
    pub(crate) fn join_kind(&self, node: NodeId) -> Join {
        let how = self
            .written(node)
            .into_iter()
            .filter(|&argument| self.tree.kind(argument) == SyntaxKind::Keyword)
            .find(|&argument| self.keyword_name(argument) == Some("how"))
            .map(|argument| self.keyword_value(argument));
        let text = how.map(|how| self.text_of(how)).unwrap_or_default();
        match text.get(1..text.len().saturating_sub(1)) {
            Some("left") => Join::Left,
            Some("right") => Join::Right,
            Some("outer") => Join::Outer,
            _ => Join::Inner,
        }
    }

    /// A key column of a join of kind `kind`, from the key's column `column` in each frame,
    /// `sides` with the left frame's first, and the positions `rows` of the pairs in each. An
    /// inner or left join has a left row in every pair, so the key is the left frame's; a
    /// right join has a right row in every pair, so it is the right frame's; an outer join
    /// takes it from whichever frame has the row ([`Builder::either_key`]). The caller owns
    /// the column. `None` after a report.
    pub(crate) fn key_column(
        &mut self,
        kind: Join,
        sides: [Place; 2],
        column: &Column<'db>,
        rows: &[Place; 2],
        node: NodeId,
    ) -> Option<Operand<'db>> {
        let [left, right] = sides;
        match kind {
            Join::Inner | Join::Left => self.chosen_column(left, column, ("taken", &rows[0]), node),
            Join::Right => self.chosen_column(right, column, ("taken", &rows[1]), node),
            Join::Outer => self.either_key([left, right], column, rows, node),
        }
    }

    /// The type of an array of validity bytes.
    pub(crate) fn bitmap_type(&self) -> Type<'db> {
        self.array_of(Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::U8)),
        ))
    }

    /// The values and the validity bitmap `bits` put together as a column that may hold
    /// `None`, whose values are `element`s. The caller owns the column.
    pub(crate) fn with_bits(
        &mut self,
        element: Type<'db>,
        values: Local,
        bits: Local,
        node: NodeId,
    ) -> Operand<'db> {
        let parts = vec![self.values_type(element), self.bitmap_type()];
        let stored = Type::new(self.db, TypeKind::Tuple(parts));
        let parts = vec![
            Operand::Move(Place::from(values)),
            Operand::Move(Place::from(bits)),
        ];
        let whole = self.temporary(stored, node);
        self.assign(Place::from(whole), Rvalue::Aggregate(stored, parts), node);
        Operand::Move(Place::from(whole))
    }

    /// The elements of the values at `values`, `element`s, at the positions `rows`, with the
    /// filler where a position is -1: the library's `taken_gaps`, or `text_taken_gaps` for
    /// text. The caller owns the new values. `None` after a report.
    fn taken_gaps(
        &mut self,
        values: Place,
        element: Type<'db>,
        rows: &Place,
        node: NodeId,
    ) -> Option<Local> {
        let returns = self.values_type(element);
        if matches!(element.kind(self.db), TypeKind::Str) {
            let arguments = vec![values, rows.clone()];
            return self.frame_call("text_taken_gaps", Vec::new(), arguments, returns, node);
        }
        let filler = Operand::Constant(self.filler(element)?);
        let filler = Place::from(self.held(element, filler, node));
        let arguments = vec![values, rows.clone(), filler];
        self.frame_call("taken_gaps", vec![element], arguments, returns, node)
    }

    /// The column `column`, stored at `from`, taken at the positions `rows` of a join's
    /// pairs, where -1 says the frame has no row: a new column that may hold `None`, with
    /// `None` at each -1 and wherever the column itself held one. The caller owns it. `None`
    /// after a report. Cost: one pass over `rows` for the values and one for the bitmap.
    pub(crate) fn gap_column(
        &mut self,
        from: Place,
        column: &Column<'db>,
        rows: &Place,
        node: NodeId,
    ) -> Option<Operand<'db>> {
        let bytes = self.bitmap_type();
        if let &TypeKind::Optional(element) = column.element.kind(self.db) {
            let values = from.clone().project(Projection::Field(VALUES));
            let values = self.taken_gaps(values, element, rows, node)?;
            let arguments = vec![from.project(Projection::Field(VALIDITY)), rows.clone()];
            let bits = self.frame_call("taken_gap_bits", Vec::new(), arguments, bytes, node)?;
            return Some(self.with_bits(element, values, bits, node));
        }
        let values = self.taken_gaps(from, column.element, rows, node)?;
        let bits = self.frame_call("gap_bits", Vec::new(), vec![rows.clone()], bytes, node)?;
        Some(self.with_bits(column.element, values, bits, node))
    }

    /// The key column `column` of an outer join, from the key's column in each frame,
    /// `sides` with the left frame's first, and the positions `rows` of the pairs in each:
    /// the left frame's key where a pair has a left row and the right frame's where it has
    /// none (the library's `either`, `text_either` for text, and `either_bits` for the
    /// validity bitmap of a key that may hold `None`). The caller owns it. `None` after a
    /// report.
    fn either_key(
        &mut self,
        sides: [Place; 2],
        column: &Column<'db>,
        rows: &[Place; 2],
        node: NodeId,
    ) -> Option<Operand<'db>> {
        let [left, right] = sides;
        let &TypeKind::Optional(element) = column.element.kind(self.db) else {
            let values = self.either_values([left, right], column.element, rows, node)?;
            return Some(Operand::Move(Place::from(values)));
        };
        let values = [&left, &right].map(|side| side.clone().project(Projection::Field(VALUES)));
        let values = self.either_values(values, element, rows, node)?;
        let arguments = vec![
            left.project(Projection::Field(VALIDITY)),
            right.project(Projection::Field(VALIDITY)),
            rows[0].clone(),
            rows[1].clone(),
        ];
        let bytes = self.bitmap_type();
        let bits = self.frame_call("either_bits", Vec::new(), arguments, bytes, node)?;
        Some(self.with_bits(element, values, bits, node))
    }

    /// The values of a key of an outer join, `element`s, from the values of the key in each
    /// frame, `sides`, at the positions `rows`. The caller owns them. `None` after a report.
    fn either_values(
        &mut self,
        sides: [Place; 2],
        element: Type<'db>,
        rows: &[Place; 2],
        node: NodeId,
    ) -> Option<Local> {
        let returns = self.values_type(element);
        let [left, right] = sides;
        let mut arguments = vec![left, right, rows[0].clone(), rows[1].clone()];
        if matches!(element.kind(self.db), TypeKind::Str) {
            return self.frame_call("text_either", Vec::new(), arguments, returns, node);
        }
        let filler = Operand::Constant(self.filler(element)?);
        arguments.push(Place::from(self.held(element, filler, node)));
        self.frame_call("either", vec![element], arguments, returns, node)
    }
}
