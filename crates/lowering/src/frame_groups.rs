//! Grouping the rows of a frame (decision 0045, roadmap 3.13.7):
//! `df.groupby(keys).agg(name=("column", "reduction"), ...)`, a new frame with one row for
//! each group: the key columns, then one column for each argument of `agg`.
//!
//! The work is Baryon code in the standard library's `<std>/frames.byn` (decision 0038).
//! The rows are sorted by the keys with the stable sort `sort_values` uses, so the rows of a
//! group are next to each other and the groups come out in the keys' order; then each
//! reduction is one pass over one column. The library reduces `f64` and `i64` columns only,
//! so a column of another number type is converted here first, and the result converted
//! back where pandas keeps the column's type. Cost for `n` rows: `n log n` comparisons for
//! each key column, and one pass for each new column.

use crate::builder::Builder;
use mir::{BinaryOperator, Constant, Local, Operand, Place, Projection, Rvalue};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Column, Float, Integer, Primitive, Type, TypeKind};

/// The reductions of `agg`, each at the number the library's `group_floats` and
/// `group_integers` know it by.
const REDUCTIONS: [&str; 10] = [
    "sum", "min", "max", "first", "last", "count", "mean", "var", "std", "median",
];

/// The rows of a frame sorted into groups: what each reduction reads.
struct Groups {
    /// The local holding the rows' positions, sorted by the keys.
    order: Local,
    /// The local holding where each group starts in `order`, and last where `order` ends.
    starts: Local,
}

impl<'db> Builder<'db> {
    /// The call `node`, `frame.groupby(...).agg(...)`, into `place`. The checker has made
    /// sure of every column and reduction named.
    pub(crate) fn frame_agg(&mut self, place: Place, frame: NodeId, node: NodeId) {
        let ty = self.ty(node);
        let (Some(columns), Some(result)) = (self.frame_columns(frame), self.frame_columns(node))
        else {
            return self.unsupported(node, "this `groupby`");
        };
        let grouping = self
            .tree
            .child(node, Role::Function)
            .and_then(|function| self.tree.child(function, Role::Value));
        let by = grouping.and_then(|grouping| self.written(grouping).first().copied());
        let Some(by) = by.map(|by| self.keyword_value(by)) else {
            return self.unsupported(node, "this `groupby`");
        };
        let source = self.borrow(frame);
        let mut keys = Vec::new();
        for key in self.listed(by) {
            let Some((position, column)) = self.named_column(columns, key) else {
                return self.unsupported(key, "this column to group by");
            };
            keys.push((source.clone().project(Projection::Field(position)), column));
        }
        let Some(groups) = self.groups(&source, &keys, node) else {
            return;
        };
        let positions = self.positions_type();
        let arguments = vec![Place::from(groups.order), Place::from(groups.starts)];
        let Some(firsts) = self.frame_call("group_keys", Vec::new(), arguments, positions, node)
        else {
            return;
        };
        self.drop_later(firsts);
        let mut made = Vec::with_capacity(result.len());
        for (key, column) in &keys {
            let array = self.array_of(column.element);
            let arguments = vec![key.clone(), Place::from(firsts)];
            let Some(taken) =
                self.frame_call("taken", vec![column.element], arguments, array, node)
            else {
                return;
            };
            made.push(Operand::Move(Place::from(taken)));
        }
        let reduced = self.written(node);
        for (argument, new) in reduced.into_iter().zip(&result[keys.len()..]) {
            let pair: Vec<NodeId> = self
                .tree
                .children_with(self.keyword_value(argument), Role::Item)
                .collect();
            let (Some(&read), Some(&how)) = (pair.first(), pair.get(1)) else {
                return self.unsupported(argument, "this column of `agg`");
            };
            let how = REDUCTIONS
                .iter()
                .position(|name| *name == self.written_text(how))
                .and_then(|how| i128::try_from(how).ok());
            let (Some((position, column)), Some(how)) = (self.named_column(columns, read), how)
            else {
                return self.unsupported(argument, "this column of `agg`");
            };
            let values = source.clone().project(Projection::Field(position));
            let Some(column) = self.group_column(&groups, (values, column), how, new, argument)
            else {
                return;
            };
            made.push(Operand::Move(column));
        }
        self.assign(place, Rvalue::Aggregate(ty, made), node);
    }

    /// The rows of the frame at `source` sorted into groups by `keys`, each a key column's
    /// place and its column: rows with a missing key left out, the rest sorted by the keys,
    /// the last key first, and the start of each group found.
    fn groups(
        &mut self,
        source: &Place,
        keys: &[(Place, &Column<'db>)],
        node: NodeId,
    ) -> Option<Groups> {
        let positions = self.positions_type();
        let boolean = self.boolean_type();
        let count = self.row_count(source, node);
        let start = vec![Place::from(count)];
        let mut order = self.frame_call("row_positions", Vec::new(), start, positions, node)?;
        self.drop_later(order);
        // A `bool` key is sorted and compared as the integers 0 and 1.
        let int = self.int_type();
        let sorted: Vec<(Place, Type<'db>)> = keys
            .iter()
            .map(|(key, column)| {
                if matches!(
                    column.element.kind(self.db),
                    TypeKind::Primitive(Primitive::Bool)
                ) {
                    (self.converted(key.clone(), column.element, int, node), int)
                } else {
                    (key.clone(), column.element)
                }
            })
            .collect();
        let rising = Operand::Constant(Constant {
            ty: boolean,
            value: mir::ConstantValue::Bool(true),
        });
        let rising = self.held(boolean, rising, node);
        for key in &sorted {
            if matches!(
                key.1.kind(self.db),
                TypeKind::Primitive(Primitive::Float(_))
            ) {
                let arguments = vec![key.0.clone(), Place::from(order)];
                let kept = self.frame_call("kept_rows", vec![key.1], arguments, positions, node)?;
                self.drop_later(kept);
                order = kept;
            }
        }
        for key in sorted.iter().rev() {
            let arguments = vec![key.0.clone(), Place::from(order), Place::from(rising)];
            order = self.frame_call("ordered", vec![key.1], arguments, positions, node)?;
            self.drop_later(order);
        }
        let flags = self.array_of(boolean);
        let unmarked = vec![Place::from(order)];
        let mut marks = self.frame_call("no_marks", Vec::new(), unmarked, flags, node)?;
        self.drop_later(marks);
        for key in &sorted {
            let arguments = vec![key.0.clone(), Place::from(order), Place::from(marks)];
            marks = self.frame_call("group_marks", vec![key.1], arguments, flags, node)?;
            self.drop_later(marks);
        }
        let marks = vec![Place::from(marks)];
        let starts = self.frame_call("group_starts", Vec::new(), marks, positions, node)?;
        self.drop_later(starts);
        Some(Groups { order, starts })
    }

    /// The place of a new array holding one value for each group of `groups`: the values of
    /// `read`, a column's place and its column, reduced the way `how` numbers, as the column
    /// `new` of the result. Integers are reduced as `i64` where the result is an integer and
    /// as `f64` where it is a mean, a spread or a median; floats always as `f64`.
    fn group_column(
        &mut self,
        groups: &Groups,
        (values, column): (Place, &Column<'db>),
        how: i128,
        new: &Column<'db>,
        node: NodeId,
    ) -> Option<Place> {
        let int = self.int_type();
        let float = Type::new(self.db, TypeKind::Primitive(Primitive::Float(Float::F64)));
        let is_float =
            |ty: Type<'db>| matches!(ty.kind(self.db), TypeKind::Primitive(Primitive::Float(_)));
        let whole = !is_float(column.element) && !is_float(new.element);
        let (function, through) = if whole {
            ("group_integers", int)
        } else {
            ("group_floats", float)
        };
        let values = self.converted(values, column.element, through, node);
        let how = Operand::Constant(Constant::integer(int, how));
        let how = self.held(int, how, node);
        let arguments = vec![
            values,
            Place::from(groups.order),
            Place::from(groups.starts),
            Place::from(how),
        ];
        let array = self.array_of(through);
        let reduced = self.frame_call(function, Vec::new(), arguments, array, node)?;
        if through == new.element {
            return Some(Place::from(reduced));
        }
        self.drop_later(reduced);
        let result = self.converted_new(&Place::from(reduced), new.element, node);
        Some(Place::from(result))
    }

    /// The place of the array `array`, whose elements are `from`, with its elements as `to`:
    /// `array` itself when the two are one type, and otherwise a new array, dropped when the
    /// statement ends. Cost: one pass, and the new array.
    pub(crate) fn converted(
        &mut self,
        array: Place,
        from: Type<'db>,
        to: Type<'db>,
        node: NodeId,
    ) -> Place {
        if from == to {
            return array;
        }
        let new = self.converted_new(&array, to, node);
        self.drop_later(new);
        Place::from(new)
    }

    /// A new array of rank 1 holding each element of `array` converted to the number type
    /// `to`, which the caller owns. An array made into `bool` is one of `i64`.
    fn converted_new(&mut self, array: &Place, to: Type<'db>, node: NodeId) -> Local {
        let length = self.temporary(self.int_type(), node);
        self.assign(Place::from(length), Rvalue::Length(array.clone(), 0), node);
        let length = Operand::Copy(Place::from(length));
        let is_flag = matches!(to.kind(self.db), TypeKind::Primitive(Primitive::Bool));
        let ty = self.array_of(to);
        let new = self.temporary(ty, node);
        self.assign(
            Place::from(new),
            Rvalue::NewArray(ty, vec![length.clone()]),
            node,
        );
        self.loop_nest(&[length], &mut Vec::new(), node, &mut |builder, indices| {
            let from = array.clone().project(Projection::Index(indices.to_vec()));
            let to_place = Place::from(new).project(Projection::Index(indices.to_vec()));
            let value = if is_flag {
                // A backend converts no number to `bool`; a flag is a number that is not 0.
                let zero = Operand::Constant(Constant::integer(builder.int_type(), 0));
                Rvalue::Binary(BinaryOperator::NotEqual, Operand::Copy(from), zero)
            } else {
                Rvalue::Convert(Operand::Copy(from), to)
            };
            builder.assign(to_place, value, node);
        });
        new
    }

    /// The arguments written in the call `call`, in order, keywords included.
    pub(crate) fn written(&self, call: NodeId) -> Vec<NodeId> {
        self.tree
            .child(call, Role::Arguments)
            .into_iter()
            .flat_map(|list| self.tree.children_with(list, Role::Item))
            .collect()
    }

    /// The value of the argument `argument`: what follows `name=` for a keyword, and the
    /// argument itself otherwise.
    pub(crate) fn keyword_value(&self, argument: NodeId) -> NodeId {
        (self.tree.kind(argument) == SyntaxKind::Keyword)
            .then(|| self.tree.child(argument, Role::Value))
            .flatten()
            .unwrap_or(argument)
    }

    /// The text the string literal `node` writes, without its quotes.
    fn written_text(&self, node: NodeId) -> &'db str {
        let text = self.text_of(node);
        text.get(1..text.len().saturating_sub(1)).unwrap_or(text)
    }

    /// The column of `columns` that the string literal `node` names, with its position in
    /// the tuple that stores the frame.
    pub(crate) fn named_column(
        &self,
        columns: &'db [Column<'db>],
        node: NodeId,
    ) -> Option<(u32, &'db Column<'db>)> {
        let name = self.written_text(node);
        (0_u32..)
            .zip(columns)
            .find(|(_, column)| column.name == name)
    }

    /// The type of an array of rank 1 of `element`.
    pub(crate) fn array_of(&self, element: Type<'db>) -> Type<'db> {
        Type::new(self.db, TypeKind::Array { element, rank: 1 })
    }

    /// The type of an array of row positions, `Array[i64, 1]`.
    pub(crate) fn positions_type(&self) -> Type<'db> {
        self.array_of(Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::I64)),
        ))
    }
}
