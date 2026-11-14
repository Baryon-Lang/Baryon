//! Taking the missing values out (decision 0045, part 3, roadmap 3.13.6): `dropna()` and
//! `fillna(value)` of an array with missing values and of a frame.
//!
//! The result holds no `None`, so it is stored as a plain array: `dropna` chooses the values
//! whose validity bit is set, through the mask the library's `missing` makes and the
//! `masked` every choice of rows uses, and `fillna` copies the values and writes the value
//! where a bit is clear, in the library's `filled` (`<std>/frame_missing.byn`, decision
//! 0038). A frame does the same for each column that may hold `None`, with one mask for all
//! of them, and copies or chooses from its other columns.

use crate::builder::Builder;
use crate::frames::erase;
use crate::optional_arrays::{VALIDITY, VALUES};
use mir::{Local, Operand, Place, Projection, Rvalue};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Type, TypeKind};

impl<'db> Builder<'db> {
    /// `values.dropna()` on the array with missing values `array`, into `place`: a new
    /// `Array[T, 1]` of the elements that are there, in order. Cost: one pass over the
    /// bitmap for a mask of one byte for each element, dropped when the statement ends, and
    /// one pass that copies the values kept.
    pub(crate) fn optional_array_dropped(&mut self, place: Place, array: NodeId, node: NodeId) {
        let Some(element) = self.optional_array_element(array) else {
            return self.unsupported(node, "`dropna` of this value");
        };
        let base = self.borrow(array);
        let length = self.optional_array_length(&base, node);
        let bits = base.clone().project(Projection::Field(VALIDITY));
        let Some(mask) = self.validity_mask(bits, length, false, node) else {
            return;
        };
        self.drop_later(mask);
        let values = base.project(Projection::Field(VALUES));
        let arguments = vec![values, Place::from(mask)];
        let narrowed = self.array_of(element);
        if let Some(kept) = self.frame_call("masked", vec![element], arguments, narrowed, node) {
            self.assign(place, Rvalue::Use(Operand::Move(Place::from(kept))), node);
        }
    }

    /// `values.fillna(value)` on the array with missing values `array`, into `place`: a new
    /// `Array[T, 1]` with `value` where an element was `None`. Cost: a copy of the values
    /// and one pass over the bitmap.
    pub(crate) fn optional_array_filled(
        &mut self,
        place: Place,
        array: NodeId,
        value: NodeId,
        node: NodeId,
    ) {
        let Some(element) = self.optional_array_element(array) else {
            return self.unsupported(node, "`fillna` of this value");
        };
        let base = self.borrow(array);
        let value = self.keyword_value(value);
        let value = self.operand_as(value, element);
        if let Some(filled) = self.none_filled(&base, element, value, node) {
            self.assign(place, Rvalue::Use(filled), node);
        }
    }

    /// A new `Array[element, 1]` of the values of the array with missing values at `base`,
    /// with `value` where an element is `None`: the library's `filled`. `None` after a
    /// report.
    fn none_filled(
        &mut self,
        base: &Place,
        element: Type<'db>,
        value: Operand<'db>,
        node: NodeId,
    ) -> Option<Operand<'db>> {
        let value = Place::from(self.held(element, value, node));
        let values = base.clone().project(Projection::Field(VALUES));
        let bits = base.clone().project(Projection::Field(VALIDITY));
        let narrowed = self.array_of(element);
        let arguments = vec![values, bits, value];
        let filled = self.frame_call("filled", vec![element], arguments, narrowed, node)?;
        Some(Operand::Move(Place::from(filled)))
    }

    /// `df.dropna()` on the frame `frame`, into `place`: a new frame of the rows that have
    /// no `None` in any column, whose columns hold no `None`. A frame with no column that
    /// may hold `None` is copied.
    ///
    /// Cost: one mask of a byte for each row for each column that may hold `None`, all
    /// dropped when the statement ends, then one pass over each column that copies the rows
    /// kept.
    pub(crate) fn frame_dropna(&mut self, place: Place, frame: NodeId, node: NodeId) {
        let (ty, Some(columns)) = (self.ty(node), self.frame_columns(frame)) else {
            return;
        };
        let optional = |builder: &Self, element: Type<'db>| match element.kind(builder.db) {
            &TypeKind::Optional(element) => Some(element),
            _ => None,
        };
        if !columns
            .iter()
            .any(|column| optional(self, column.element).is_some())
        {
            return self.frame_copy(place, frame, node);
        }
        let source = self.borrow(frame);
        let count = self.counted_rows(frame, &source, node);
        // One mask for the frame: true where every column that may hold `None` has a value.
        let mut mask: Option<Local> = None;
        for (position, column) in (0_u32..).zip(columns) {
            if optional(self, column.element).is_none() {
                continue;
            }
            let bits = source
                .clone()
                .project(Projection::Field(position))
                .project(Projection::Field(VALIDITY));
            let next = match mask {
                None => {
                    let count = Operand::Copy(Place::from(count));
                    self.validity_mask(bits, count, false, node)
                }
                Some(mask) => {
                    let arguments = vec![Place::from(mask), bits];
                    let flags = self.array_of(self.boolean_type());
                    self.frame_call("both", Vec::new(), arguments, flags, node)
                }
            };
            let Some(next) = next else {
                return;
            };
            self.drop_later(next);
            mask = Some(next);
        }
        let Some(mask) = mask else {
            return;
        };
        let mut chosen = Vec::with_capacity(columns.len());
        for (position, column) in (0_u32..).zip(columns) {
            let from = source.clone().project(Projection::Field(position));
            let by = Place::from(mask);
            let result = if let Some(element) = optional(self, column.element) {
                let values = from.project(Projection::Field(VALUES));
                let narrowed = self.array_of(element);
                self.frame_call("masked", vec![element], vec![values, by], narrowed, node)
            } else if matches!(column.element.kind(self.db), TypeKind::Str) {
                let array = erase(self.db, column.array(self.db));
                self.frame_call("text_masked", Vec::new(), vec![from, by], array, node)
            } else {
                let array = column.array(self.db);
                self.frame_call("masked", vec![column.element], vec![from, by], array, node)
            };
            let Some(result) = result else {
                return;
            };
            chosen.push(Operand::Move(Place::from(result)));
        }
        self.assign(place, Rvalue::Aggregate(ty, chosen), node);
    }

    /// `df.fillna(value)` on the frame `frame`, with `value` the one argument, into `place`:
    /// a new frame with the value in place of each `None`. One value, computed once, fills
    /// every column that may hold `None`; a dict fills the columns its keys name. The
    /// checker's type of the result says which columns are filled: those that may hold
    /// `None` in the frame and not in the result. Every other column is copied.
    pub(crate) fn frame_fillna(
        &mut self,
        place: Place,
        frame: NodeId,
        value: NodeId,
        node: NodeId,
    ) {
        let ty = self.ty(node);
        let stored = match self.ty(frame).kind(self.db) {
            TypeKind::Tuple(stored) => stored.as_slice(),
            _ => &[],
        };
        let (Some(columns), Some(results)) = (self.frame_columns(frame), self.frame_columns(node))
        else {
            return;
        };
        let value = self.keyword_value(value);
        let table = self.tree.kind(value) == SyntaxKind::Dictionary;
        let fills = columns.iter().zip(results).any(|(old, new)| old != new);
        // One value is computed once, before any column is read, and converted for each
        // column it fills.
        let single = (!table && fills).then(|| {
            let found = self.ty(value);
            let operand = self.operand(value);
            (self.held(found, operand, value), found)
        });
        let source = self.borrow(frame);
        let mut filled = Vec::with_capacity(columns.len());
        for (((position, column), result), &array) in
            (0_u32..).zip(columns).zip(results).zip(stored)
        {
            let from = source.clone().project(Projection::Field(position));
            let element = match column.element.kind(self.db) {
                &TypeKind::Optional(element) if column != result => element,
                _ => {
                    filled.push(self.column_copy(from, array, node));
                    continue;
                }
            };
            let operand = match single {
                Some((held, found)) => {
                    self.convert(Operand::Copy(Place::from(held)), found, element, node)
                }
                None => match self.fill_entry(value, &column.name) {
                    Some(given) => self.operand_as(given, element),
                    None => return self.unsupported(node, "this `fillna` of a `DataFrame`"),
                },
            };
            let Some(column) = self.none_filled(&from, element, operand, node) else {
                return;
            };
            filled.push(column);
        }
        self.assign(place, Rvalue::Aggregate(ty, filled), node);
    }

    /// The value the dict `table`, the argument of `fillna`, gives for the column `name`:
    /// that of the last entry whose key is the string literal `name`, as Python keeps the
    /// last of two equal keys.
    fn fill_entry(&self, table: NodeId, name: &str) -> Option<NodeId> {
        self.tree
            .children_with(table, Role::Item)
            .filter(|&entry| {
                self.tree.child(entry, Role::Key).is_some_and(|key| {
                    let text = self.text_of(key);
                    text.len() >= 2 && text.get(1..text.len() - 1) == Some(name)
                })
            })
            .filter_map(|entry| self.tree.child(entry, Role::Value))
            .last()
    }
}
