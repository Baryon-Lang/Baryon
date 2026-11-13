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

use crate::builder::Builder;
use mir::{Constant, ConstantValue, Local, Operand, Place, Projection, Rvalue};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Primitive, Type, TypeKind};

impl Builder<'_> {
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
        let (mut by, mut ascending) = (None, None);
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
            let mut element = column.element;
            let mut values = source.clone().project(Projection::Field(position));
            // A column of `str` is sorted by the integers that stand for its text.
            if matches!(element.kind(self.db), TypeKind::Str) {
                let Some(codes) = self.text_codes(values, key) else {
                    return;
                };
                element = self.int_type();
                values = codes;
            }
            let flags = matches!(element.kind(self.db), TypeKind::Primitive(Primitive::Bool));
            let (function, type_arguments) = if flags {
                ("ordered_flags", Vec::new())
            } else {
                ("ordered", vec![element])
            };
            let arguments = vec![values, Place::from(order), Place::from(direction)];
            let Some(next) = self.frame_call(function, type_arguments, arguments, positions, key)
            else {
                return;
            };
            self.drop_later(next);
            order = next;
        }
        self.each_column(place, frame, &source, ("taken", &Place::from(order)), node);
    }

    /// `a.argsort()` on the rank-1 array `array`, into `place`: the positions of its elements
    /// in rising order, with equal elements in the order they stand in and NaN last. It is
    /// the sort `sort_values` uses, on one column that is the array. Cost: `n log n`
    /// comparisons and two arrays of `n` positions.
    pub(crate) fn array_argsort(&mut self, place: Place, array: NodeId, node: NodeId) {
        let TypeKind::Array { element, .. } = self.ty(array).kind(self.db) else {
            return self.unsupported(node, "`argsort` of this value");
        };
        let element = *element;
        let source = self.borrow(array);
        let int = self.int_type();
        let count = self.temporary(int, node);
        self.assign(Place::from(count), Rvalue::Length(source.clone(), 0), node);
        let positions = self.ty(node);
        let start = vec![Place::from(count)];
        let Some(order) = self.frame_call("row_positions", Vec::new(), start, positions, node)
        else {
            return;
        };
        self.drop_later(order);
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
        let Some(sorted) = self.frame_call(function, type_arguments, arguments, positions, node)
        else {
            return;
        };
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
