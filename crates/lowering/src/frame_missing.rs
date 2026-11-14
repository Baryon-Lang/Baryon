//! Taking the missing values out (decision 0045, part 3, roadmap 3.13.6): `dropna()` and
//! `fillna(value)` of an array with missing values and of a frame, and the reductions of an
//! array that skip them, `count`, `sum`, `mean`, `min` and `max`. The rows of a column that
//! may hold `None` are chosen here too ([`Builder::chosen_column`]).
//!
//! The result holds no `None`, so it is stored as a plain array: `dropna` chooses the values
//! whose validity bit is set, through the mask the library's `missing` makes and the
//! `masked` every choice of rows uses, and `fillna` copies the values and writes the value
//! where a bit is clear, in the library's `filled` (`<std>/frame_missing.byn`, decision
//! 0038). A frame does the same for each column that may hold `None`, with one mask for all
//! of them, and copies or chooses from its other columns.

use crate::builder::Builder;
use crate::frame_distinct::Key;
use crate::frames::erase;
use crate::optional_arrays::{VALIDITY, VALUES};
use mir::{BinaryOperator, Constant, ConstantValue, Local, Operand, Place, Projection, Rvalue};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Column, Integer, Primitive, Type, TypeKind};

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

    /// The two keys that stand for the column that may hold `None` stored at `column`, whose
    /// values are `element`s, where rows are compared: the library's `presence`, 1 for a
    /// value and 0 for `None`, and the values with the filler where one is `None`, a `bool`
    /// as the integers 0 and 1. Two rows match in both exactly when both hold `None` or both
    /// hold the same value, so a `None` equals a `None`, as pandas compares rows for
    /// `drop_duplicates`. Both arrays are dropped when the statement ends. `None` after a
    /// report.
    pub(crate) fn optional_keys(
        &mut self,
        column: &Place,
        element: Type<'db>,
        node: NodeId,
    ) -> Option<[Key<'db>; 2]> {
        let int = self.int_type();
        let count = self.optional_array_length(column, node);
        let count = Place::from(self.held(int, count, node));
        let bits = column.clone().project(Projection::Field(VALIDITY));
        let positions = self.positions_type();
        let flags = self.frame_call("presence", Vec::new(), vec![bits, count], positions, node)?;
        self.drop_later(flags);
        let filler = Operand::Constant(self.filler(element)?);
        let Operand::Move(values) = self.none_filled(column, element, filler, node)? else {
            return None;
        };
        let local = values.local;
        self.drop_later(local);
        let values = match element.kind(self.db) {
            TypeKind::Primitive(Primitive::Bool) => {
                (self.converted(values, element, int, node), int)
            }
            _ => (values, element),
        };
        Some([(Place::from(flags), int), values])
    }

    /// `values.count()`, `sum()`, `min()`, `max()` or `mean()`, the method `method`, on the
    /// array with missing values `array`, into `place`: the reduction of the elements that
    /// are there.
    ///
    /// `sum` and `mean` gather those elements into a new array, freed when the statement
    /// ends, and add it up as any array is added ([`Builder::sum_of`]), so a float sum has
    /// the bits of `values.dropna().sum()`. The mean is that sum over the count, NaN when no
    /// element is there, as the mean of an empty array is. `min` and `max` scan in the
    /// library after a check that an element is there, and panic as they do on an empty
    /// array when none is.
    pub(crate) fn optional_array_reduced(
        &mut self,
        place: Place,
        method: &str,
        array: NodeId,
        node: NodeId,
    ) {
        let Some(element) = self.optional_array_element(array) else {
            return self.unsupported(node, "this reduction of an array with missing values");
        };
        let int = self.int_type();
        let result = self.ty(node);
        let base = self.borrow(array);
        let values = base.clone().project(Projection::Field(VALUES));
        let bits = base.clone().project(Projection::Field(VALIDITY));
        let length = self.optional_array_length(&base, node);
        let length = Place::from(self.held(int, length, node));
        let arguments = vec![bits.clone(), length];
        let Some(count) = self.frame_call("present_count", Vec::new(), arguments, int, node) else {
            return;
        };
        let count = Operand::Copy(Place::from(count));
        if method == "count" {
            return self.assign(place, Rvalue::Use(count), node);
        }
        if matches!(method, "min" | "max") {
            self.not_empty(std::slice::from_ref(&count), false, node);
            let Some(start) = self.filler(element) else {
                return self.unsupported(node, "this reduction of an array with missing values");
            };
            let start = Place::from(self.held(element, Operand::Constant(start), node));
            let boolean = self.boolean_type();
            let smallest = Constant {
                ty: boolean,
                value: ConstantValue::Bool(method == "min"),
            };
            let smallest = Place::from(self.held(boolean, Operand::Constant(smallest), node));
            let arguments = vec![values, bits, start, smallest];
            let found = self.frame_call("present_extreme", vec![element], arguments, element, node);
            if let Some(found) = found {
                self.assign(place, Rvalue::Use(Operand::Copy(Place::from(found))), node);
            }
            return;
        }
        let gathered = self.array_of(element);
        let arguments = vec![values, bits];
        let Some(there) = self.frame_call("present", vec![element], arguments, gathered, node)
        else {
            return;
        };
        self.drop_later(there);
        if method == "sum" {
            return self.sum_of(place, (&Place::from(there), gathered, result), node);
        }
        let total = self.temporary(result, node);
        self.sum_of(
            Place::from(total),
            (&Place::from(there), gathered, result),
            node,
        );
        let count = self.convert(count, int, result, node);
        let total = Operand::Copy(Place::from(total));
        self.assign(
            place,
            Rvalue::Binary(BinaryOperator::Divide, total, count),
            node,
        );
    }

    /// The elements of the column `column`, stored at `from`, that the library function
    /// `kernel.0` chooses with `kernel.1`: `masked` with a mask, or `taken` with positions.
    /// The caller owns the new column. `None` after a report.
    ///
    /// A column of `str` goes through `text_masked` or `text_taken`. A column that may hold
    /// `None` has its values chosen as any array's are, and its validity bitmap rebuilt for
    /// the rows chosen by `masked_bits` or `taken_bits`, so a `None` stays with its row.
    pub(crate) fn chosen_column(
        &mut self,
        from: Place,
        column: &Column<'db>,
        (function, by): (&str, &Place),
        node: NodeId,
    ) -> Option<Operand<'db>> {
        let stored = erase(self.db, column.array(self.db));
        let chosen = match column.element.kind(self.db) {
            TypeKind::Str => {
                let name = format!("text_{function}");
                self.frame_call(&name, Vec::new(), vec![from, by.clone()], stored, node)?
            }
            &TypeKind::Optional(element) => {
                let values = from.clone().project(Projection::Field(VALUES));
                let bits = from.project(Projection::Field(VALIDITY));
                let plain = self.array_of(element);
                let arguments = vec![values, by.clone()];
                let values = self.frame_call(function, vec![element], arguments, plain, node)?;
                let name = format!("{function}_bits");
                let bytes = self.array_of(Type::new(
                    self.db,
                    TypeKind::Primitive(Primitive::Integer(Integer::U8)),
                ));
                let arguments = vec![bits, by.clone()];
                let bits = self.frame_call(&name, Vec::new(), arguments, bytes, node)?;
                let parts = vec![
                    Operand::Move(Place::from(values)),
                    Operand::Move(Place::from(bits)),
                ];
                let whole = self.temporary(stored, node);
                self.assign(Place::from(whole), Rvalue::Aggregate(stored, parts), node);
                whole
            }
            _ => {
                let types = vec![column.element];
                self.frame_call(function, types, vec![from, by.clone()], stored, node)?
            }
        };
        Some(Operand::Move(Place::from(chosen)))
    }

    /// The row positions `order` split by the validity bitmap of the column stored at
    /// `column`, which may hold `None`: those of the rows that have a value, then those of
    /// the rows that hold `None`, each in the order `order` has them. `sort_values` sorts the
    /// first and puts the second after it. Both are dropped when the statement ends. `None`
    /// after a report. Cost: two passes over `order` and two arrays of positions.
    pub(crate) fn split_missing(
        &mut self,
        column: &Place,
        order: Local,
        node: NodeId,
    ) -> Option<(Local, Local)> {
        let boolean = self.boolean_type();
        let positions = self.positions_type();
        let bits = column.clone().project(Projection::Field(VALIDITY));
        let mut halves = Vec::with_capacity(2);
        for wanted in [true, false] {
            let wanted = Constant {
                ty: boolean,
                value: ConstantValue::Bool(wanted),
            };
            let wanted = Place::from(self.held(boolean, Operand::Constant(wanted), node));
            let arguments = vec![bits.clone(), Place::from(order), wanted];
            let half =
                self.frame_call("valid_positions", Vec::new(), arguments, positions, node)?;
            self.drop_later(half);
            halves.push(half);
        }
        Some((halves[0], halves[1]))
    }

    /// The column `first`, which may hold `None` and whose values are `element`s, with each
    /// column of `rest` after it, in order, as a new column: for each column of `rest`, which
    /// is not empty, one call of the library's `stacked` for the values and one of
    /// `stacked_bits` for the validity bitmaps. The steps on the way are dropped when the
    /// statement ends. `None` after a report.
    pub(crate) fn optional_stacked(
        &mut self,
        element: Type<'db>,
        first: Place,
        rest: Vec<Place>,
        node: NodeId,
    ) -> Option<Operand<'db>> {
        let int = self.int_type();
        let plain = self.array_of(element);
        let bytes = self.array_of(Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::U8)),
        ));
        let stored = Type::new(self.db, TypeKind::Tuple(vec![plain, bytes]));
        let filler = self.filler(element)?;
        let filler = Place::from(self.held(element, Operand::Constant(filler), node));
        let mut values = first.clone().project(Projection::Field(VALUES));
        let mut bits = first.project(Projection::Field(VALIDITY));
        let mut newest: Option<(Local, Local)> = None;
        for part in rest {
            let more = part.clone().project(Projection::Field(VALUES));
            let more_bits = part.project(Projection::Field(VALIDITY));
            let mut counts = Vec::with_capacity(2);
            for side in [&values, &more] {
                let length = self.temporary(int, node);
                self.assign(Place::from(length), Rvalue::Length(side.clone(), 0), node);
                counts.push(Place::from(length));
            }
            let arguments = vec![bits, counts[0].clone(), more_bits, counts[1].clone()];
            let next_bits = self.frame_call("stacked_bits", Vec::new(), arguments, bytes, node)?;
            let arguments = vec![values, more, filler.clone()];
            let next = self.frame_call("stacked", vec![element], arguments, plain, node)?;
            // Every pair of arrays but the last is only a step on the way.
            if let Some((step, step_bits)) = newest.replace((next, next_bits)) {
                self.drop_later(step);
                self.drop_later(step_bits);
            }
            values = Place::from(next);
            bits = Place::from(next_bits);
        }
        let (values, bits) = newest?;
        let parts = vec![
            Operand::Move(Place::from(values)),
            Operand::Move(Place::from(bits)),
        ];
        let whole = self.temporary(stored, node);
        self.assign(Place::from(whole), Rvalue::Aggregate(stored, parts), node);
        Some(Operand::Move(Place::from(whole)))
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
