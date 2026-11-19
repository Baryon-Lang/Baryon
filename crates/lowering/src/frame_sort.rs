//! Sorting the rows of a frame (decision 0045, roadmap 3.13.5): `df.sort_values(...)`, a new
//! frame with the same columns and the rows in the order of one or more of them.
//!
//! The sort is Baryon code in the standard library's `<std>/frames.byn` (decision 0038): a
//! stable merge sort of row positions by one column. Sorting by several columns is that sort
//! once for each, the last column first, since a stable sort keeps the order the sorts
//! before it made among equal values. Then `taken` copies each column in the final order.
//! The cost is `n log n` comparisons for each column sorted by, and one copy of the frame.
//!
//! A column of `str` is sorted by integers that stand for its text, which the library's
//! `text_codes` gives (`<std>/frame_joins.byn`): one more sort of that column, comparing
//! text, and one array of `n` integers.
//!
//! A column that may hold `None` is sorted by the values that are there, and its rows that
//! hold `None` come last in both directions, in the order they had (decision 0045): the row
//! positions are split in two by the validity bitmap, the first part is sorted, and the two
//! are put end to end again, three more arrays of positions. `na_position="first"` (roadmap
//! 3.13.16) puts the rows with a missing value first instead: the rows that hold `None`, then
//! those that hold NaN, which costs one more pass over a column of real numbers.

use crate::builder::Builder;
use crate::optional_arrays::VALUES;
use mir::{Constant, ConstantValue, Local, Operand, Place, Projection, Rvalue};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Primitive, Type, TypeKind};

impl<'db> Builder<'db> {
    /// The call `node` of `sort_values` on the frame `frame`, into `place`. The checker has
    /// made sure of the columns named and of `ascending`.
    pub(crate) fn frame_sort(&mut self, place: Place, frame: NodeId, node: NodeId) {
        let Some(columns) = self.frame_columns(frame) else {
            return self.unsupported(node, "this sort");
        };
        let written: Vec<NodeId> = self
            .tree
            .child(node, Role::Arguments)
            .into_iter()
            .flat_map(|list| self.tree.children_with(list, Role::Item))
            .collect();
        let (mut by, mut ascending, mut first) = (None, None, false);
        for argument in written {
            if self.tree.kind(argument) != SyntaxKind::Keyword {
                by = Some(argument);
                continue;
            }
            let value = self.tree.child(argument, Role::Value);
            let name = self.tree.child(argument, Role::Name);
            match name.map(|name| self.text_of(name)) {
                Some("by") => by = value,
                Some("ascending") => ascending = value,
                Some("na_position") => {
                    first = value.is_some_and(|value| self.written_text(value) == "first");
                }
                _ => {}
            }
        }
        let Some(by) = by else {
            return self.unsupported(node, "this sort");
        };
        let keys = self.listed(by);
        // The directions are computed first, in the order written, each once.
        let directions: Vec<Local> = ascending
            .map(|ascending| self.listed(ascending))
            .unwrap_or_default()
            .into_iter()
            .map(|direction| {
                let operand = self.operand(direction);
                self.held(self.boolean_type(), operand, direction)
            })
            .collect();
        let rising = Operand::Constant(Constant {
            ty: self.boolean_type(),
            value: ConstantValue::Bool(true),
        });
        let rising = self.held(self.boolean_type(), rising, node);
        let source = self.borrow(frame);
        let count = self.counted_rows(frame, &source, node);
        let positions = Type::new(
            self.db,
            TypeKind::Array {
                element: self.int_type(),
                rank: 1,
            },
        );
        let start = vec![Place::from(count)];
        let Some(mut order) = self.frame_call("row_positions", Vec::new(), start, positions, node)
        else {
            return;
        };
        self.drop_later(order);
        for (index, &key) in keys.iter().enumerate().rev() {
            let name = self.text_of(key);
            let name = name.get(1..name.len().saturating_sub(1)).unwrap_or(name);
            let found = (0_u32..)
                .zip(columns)
                .find(|(_, column)| column.name == name);
            let Some((position, column)) = found else {
                return self.unsupported(key, "this column to sort by");
            };
            // One direction is for every column; a list gives one for each.
            let direction = match directions.as_slice() {
                [] => rising,
                [one] => *one,
                several => several.get(index).copied().unwrap_or(rising),
            };
            let values = source.clone().project(Projection::Field(position));
            let Some(next) =
                self.ordered_by(values, column.element, (order, direction, first), key)
            else {
                return;
            };
            order = next;
        }
        self.each_column(place, frame, &source, ("taken", &Place::from(order)), node);
    }

    /// The row positions `by.0` put in the order of the column stored at `values`, whose
    /// elements are `element`s, rising when the `bool` local `by.1` is true, with the rows
    /// that hold a missing value first when `by.2`, as `na_position="first"` asks: one
    /// stable sort by one key of `sort_values`, at `key`. The new positions are dropped when the
    /// statement ends. `None` after a report.
    ///
    /// A column of `str` is sorted by the integers that stand for its text. A column that
    /// may hold `None` is sorted by the values that are there, and the rows that hold `None`
    /// go last in both directions, in the order they had. Put first, the rows that hold
    /// `None` come before those that hold NaN, the mirror of their order when last; the
    /// library's `missing_first` moves the NaN rows of a column of real numbers.
    fn ordered_by(
        &mut self,
        values: Place,
        element: Type<'db>,
        (order, direction, first): (Local, Local, bool),
        key: NodeId,
    ) -> Option<Local> {
        let positions = self.positions_type();
        let (mut values, mut element, mut order) = (values, element, order);
        let mut gone = None;
        if let &TypeKind::Optional(inner) = element.kind(self.db) {
            let (there, rest) = self.split_missing(&values, order, key)?;
            values = values.project(Projection::Field(VALUES));
            element = inner;
            order = there;
            gone = Some(rest);
        }
        if matches!(element.kind(self.db), TypeKind::Str) {
            values = self.text_codes(values, key)?;
            element = self.int_type();
        }
        let flags = matches!(element.kind(self.db), TypeKind::Primitive(Primitive::Bool));
        let (function, type_arguments) = if flags {
            ("ordered_flags", Vec::new())
        } else {
            ("ordered", vec![element])
        };
        let floats = matches!(
            element.kind(self.db),
            TypeKind::Primitive(Primitive::Float(_))
        );
        let column = values.clone();
        let arguments = vec![values, Place::from(order), Place::from(direction)];
        let mut sorted = self.frame_call(function, type_arguments, arguments, positions, key)?;
        self.drop_later(sorted);
        if first && floats {
            let arguments = vec![column, Place::from(sorted)];
            sorted = self.frame_call("missing_first", vec![element], arguments, positions, key)?;
            self.drop_later(sorted);
        }
        if let Some(gone) = gone {
            let mut ends = vec![Place::from(sorted), Place::from(gone)];
            if first {
                ends.reverse();
            }
            let int = self.int_type();
            sorted = self.frame_call("joined", vec![int], ends, positions, key)?;
            self.drop_later(sorted);
        }
        Some(sorted)
    }

    /// `a.argsort()` on the rank-1 array `array`, into `place`: the positions of its elements
    /// in rising order, with equal elements in the order they stand in and NaN last. It is
    /// the sort `sort_values` uses, on one column that is the array. Cost: `n log n`
    /// comparisons and two arrays of `n` positions. A text array is put in the order of
    /// Python's `<` on `str`, at the cost of one more sort, by `text_codes`. An array with
    /// missing values has the positions of its `None`s last, after any NaN, in the order
    /// they stand in, as a column of `sort_values` has: three more arrays of positions.
    pub(crate) fn array_argsort(&mut self, place: Place, array: NodeId, node: NodeId) {
        let int = self.int_type();
        let positions = self.ty(node);
        let whole = self.borrow(array);
        let (mut source, mut element, count) =
            if let Some(inner) = self.optional_array_element(array) {
                let values = whole.clone().project(Projection::Field(VALUES));
                (
                    values,
                    inner,
                    self.optional_array_length(&whole, inner, node),
                )
            } else if self.is_text_array(array) {
                let text = Type::new(self.db, TypeKind::Str);
                (whole.clone(), text, self.text_array_length(&whole, node))
            } else if let TypeKind::Array { element, .. } = self.ty(array).kind(self.db) {
                let count = self.temporary(int, node);
                self.assign(Place::from(count), Rvalue::Length(whole.clone(), 0), node);
                (whole.clone(), *element, Operand::Copy(Place::from(count)))
            } else {
                return self.unsupported(node, "`argsort` of this value");
            };
        let start = vec![Place::from(self.held(int, count, node))];
        let Some(mut order) = self.frame_call("row_positions", Vec::new(), start, positions, node)
        else {
            return;
        };
        self.drop_later(order);
        // The positions of the `None`s are set aside and put after the sorted rest.
        let mut gone = None;
        if self.optional_array_element(array).is_some() {
            let Some((there, rest)) = self.split_missing(&whole, order, node) else {
                return;
            };
            order = there;
            gone = Some(rest);
        }
        // A text array is sorted by the whole numbers that stand for its text.
        if matches!(element.kind(self.db), TypeKind::Str) {
            let Some(codes) = self.text_codes(source, node) else {
                return;
            };
            (source, element) = (codes, int);
        }
        let rising = Operand::Constant(Constant {
            ty: self.boolean_type(),
            value: ConstantValue::Bool(true),
        });
        let rising = self.held(self.boolean_type(), rising, node);
        let flags = matches!(element.kind(self.db), TypeKind::Primitive(Primitive::Bool));
        let (function, type_arguments) = if flags {
            ("ordered_flags", Vec::new())
        } else {
            ("ordered", vec![element])
        };
        let arguments = vec![source, Place::from(order), Place::from(rising)];
        let Some(mut sorted) =
            self.frame_call(function, type_arguments, arguments, positions, node)
        else {
            return;
        };
        if let Some(gone) = gone {
            self.drop_later(sorted);
            let ends = vec![Place::from(sorted), Place::from(gone)];
            let Some(joined) = self.frame_call("joined", vec![int], ends, positions, node) else {
                return;
            };
            sorted = joined;
        }
        self.assign(place, Rvalue::Use(Operand::Move(Place::from(sorted))), node);
    }

    /// The items of the list `node`, or `node` itself when it is not a list written out:
    /// `"price"` and `["price"]` both name one column.
    pub(crate) fn listed(&self, node: NodeId) -> Vec<NodeId> {
        if self.tree.kind(node) == SyntaxKind::List {
            self.tree.children_with(node, Role::Item).collect()
        } else {
            vec![node]
        }
    }
}
