//! Text arrays, `Array[str, 1]` (decision 0045): stored as a tuple of two arrays of numbers
//! ([`Type::text_storage`]), the bytes of every element end to end and the offset each
//! element starts at, as Arrow's `large_utf8` lays text out.
//!
//! [`erase`](crate::frames::erase) replaces the type by that tuple, so this module is the only
//! place after the checker that knows a text array from a tuple. It builds one from a list,
//! counts its elements, and reads an element as a `str` that borrows the array's bytes
//! ([`TextArrayStep::View`]), so a read allocates nothing.

use crate::builder::Builder;
use mir::{
    Argument, BinaryOperator, Builtin, Constant, Local, Operand, Place, Projection, Rvalue,
    TextArrayStep, UnaryOperator,
};
use syntax::{NodeId, Operator, Role, SyntaxKind};
use types::{Integer, Primitive, Type, TypeKind};

/// The position of the bytes in the tuple that stores a text array.
const BYTES: u32 = 0;

/// The position of the offsets in the tuple that stores a text array.
const OFFSETS: u32 = 1;

impl<'db> Builder<'db> {
    /// Whether the expression `node` gives a text array, by the type the checker found.
    pub(crate) fn is_text_array(&self, node: NodeId) -> bool {
        self.typed
            .expression(node)
            .is_some_and(|ty| matches!(ty.kind(self.db), TypeKind::TextArray))
    }

    /// The number of elements of the text array at `array`, an `i64`: one less than the number
    /// of its offsets, since the last offset is where the bytes end.
    pub(crate) fn text_array_length(&mut self, array: &Place, node: NodeId) -> Operand<'db> {
        let int = self.int_type();
        let offsets = array.clone().project(Projection::Field(OFFSETS));
        let count = self.temporary(int, node);
        self.assign(Place::from(count), Rvalue::Length(offsets, 0), node);
        let length = self.temporary(int, node);
        let one = Operand::Constant(Constant::integer(int, 1));
        let less = Rvalue::Binary(
            BinaryOperator::Subtract,
            Operand::Copy(Place::from(count)),
            one,
        );
        self.assign(Place::from(length), less, node);
        Operand::Copy(Place::from(length))
    }

    /// The number of rows of the column at `column`, stored as `stored`: the elements of a
    /// text column, which is stored as a tuple of its two buffers, the length of the values
    /// of a column that may hold `None`, which is stored as a tuple of its values and its
    /// validity bitmap, or the length of an array.
    pub(crate) fn column_length(
        &mut self,
        column: Place,
        stored: Option<Type<'db>>,
        node: NodeId,
    ) -> Operand<'db> {
        if let Some(TypeKind::Tuple(parts)) = stored.map(|stored| stored.kind(self.db)) {
            // A text column's second buffer is its offsets, an array of `i64`; a column that
            // may hold `None` ends in its validity bitmap, an array of `u8`.
            let bitmap = parts.last().is_some_and(|&last| {
                let byte = TypeKind::Primitive(Primitive::Integer(Integer::U8));
                matches!(last.kind(self.db), TypeKind::Array { element, .. }
                    if *element.kind(self.db) == byte)
            });
            if bitmap {
                let values = column.project(Projection::Field(0));
                return self.column_length(values, parts.first().copied(), node);
            }
            return self.text_array_length(&column, node);
        }
        let length = self.temporary(self.int_type(), node);
        self.assign(Place::from(length), Rvalue::Length(column, 0), node);
        Operand::Copy(Place::from(length))
    }

    /// The element at the position `index` of the text array at `array`: a `str` in a temporary
    /// that borrows the array's bytes from `offsets[index]` up to `offsets[index + 1]`. The
    /// position must be inside the array, which the caller checks or guarantees.
    pub(crate) fn text_array_element(
        &mut self,
        array: &Place,
        index: Local,
        node: NodeId,
    ) -> Place {
        let int = self.int_type();
        let offsets = array.clone().project(Projection::Field(OFFSETS));
        let after = self.temporary(int, node);
        let one = Operand::Constant(Constant::integer(int, 1));
        let next = Rvalue::Binary(BinaryOperator::Add, Operand::Copy(Place::from(index)), one);
        self.assign(Place::from(after), next, node);
        let start = offsets.clone().project(Projection::Index(vec![index]));
        let end = offsets.project(Projection::Index(vec![after]));
        let length = self.temporary(int, node);
        let bytes = Rvalue::Binary(
            BinaryOperator::Subtract,
            Operand::Copy(end),
            Operand::Copy(start.clone()),
        );
        self.assign(Place::from(length), bytes, node);
        let element = self.temporary(Type::new(self.db, TypeKind::Str), node);
        let arguments = vec![
            Argument::Borrow(array.clone().project(Projection::Field(BYTES))),
            Argument::Owned(Operand::Copy(start)),
            Argument::Owned(Operand::Copy(Place::from(length))),
        ];
        self.text_call(
            Place::from(element),
            Builtin::TextArray(TextArrayStep::View),
            arguments,
            node,
        );
        Place::from(element)
    }

    /// `Array(words)` with `words: list[str]` into `place`: a new text array holding a copy of
    /// each string's bytes. The list is borrowed, so it stays usable.
    ///
    /// The first pass adds up the bytes of the strings into the offsets, the second copies
    /// each string to its offset, so the array is exactly as large as its text.
    ///
    /// Cost: two allocations, whatever the number of strings, and two passes over the list.
    pub(crate) fn text_array_from_list(&mut self, place: &Place, list: NodeId, node: NodeId) {
        let ty = self.ty(list);
        if self.list_element(ty, node).is_none() {
            return;
        }
        let source = self.borrow(list);
        let length = self.length(&source, ty, 0, node);
        self.text_array_of(place, (&source, None), length, self.ty(node), node);
    }

    /// A new text array of type `ty` into `place`, holding a copy of the bytes of one `str`
    /// for each of the `length` items of the list at `source.0`: the item itself, or its
    /// field `source.1` when the items are structs, as for the rows of `DataFrame(rows)`.
    /// The passes and the cost are those of [`Builder::text_array_from_list`].
    pub(crate) fn text_array_of(
        &mut self,
        place: &Place,
        (source, field): (&Place, Option<u32>),
        length: Operand<'db>,
        ty: Type<'db>,
        node: NodeId,
    ) {
        let item = |indices: &[Local]| {
            let item = source.clone().project(Projection::Index(indices.to_vec()));
            match field {
                Some(field) => item.project(Projection::Field(field)),
                None => item,
            }
        };
        let int = self.int_type();
        let integer =
            |integer| Type::new(self.db, TypeKind::Primitive(Primitive::Integer(integer)));
        let (bytes_type, offsets_type) = (
            self.array_of(integer(Integer::U8)),
            self.array_of(integer(Integer::I64)),
        );
        let zero = Operand::Constant(Constant::integer(int, 0));
        let one = Operand::Constant(Constant::integer(int, 1));

        // The offsets: one more than the strings, the first 0, each next one the one before
        // plus the bytes of its string.
        let count = self.temporary(int, node);
        let more = Rvalue::Binary(BinaryOperator::Add, length.clone(), one.clone());
        self.assign(Place::from(count), more, node);
        let offsets = self.temporary(offsets_type, node);
        let sized = vec![Operand::Copy(Place::from(count))];
        self.assign(
            Place::from(offsets),
            Rvalue::NewArray(offsets_type, sized),
            node,
        );
        let total = self.temporary(int, node);
        self.assign(Place::from(total), Rvalue::Use(zero.clone()), node);
        let first = self.temporary(int, node);
        self.assign(Place::from(first), Rvalue::Use(zero.clone()), node);
        let at = |index: Local| Place::from(offsets).project(Projection::Index(vec![index]));
        self.assign(at(first), Rvalue::Use(zero.clone()), node);
        let lengths = [length];
        self.loop_nest(&lengths, &mut Vec::new(), node, &mut |builder, indices| {
            let text = item(indices);
            let size = builder.temporary(int, node);
            let text = vec![Argument::Borrow(text)];
            builder.text_call(
                Place::from(size),
                Builtin::TextArray(TextArrayStep::ByteLength),
                text,
                node,
            );
            let sum = Rvalue::Binary(
                BinaryOperator::Add,
                Operand::Copy(Place::from(total)),
                Operand::Copy(Place::from(size)),
            );
            builder.assign(Place::from(total), sum, node);
            let after = builder.temporary(int, node);
            let next = Rvalue::Binary(
                BinaryOperator::Add,
                Operand::Copy(Place::from(indices[0])),
                one.clone(),
            );
            builder.assign(Place::from(after), next, node);
            let total = Rvalue::Use(Operand::Copy(Place::from(total)));
            builder.assign(at(after), total, node);
        });

        // The bytes: each string copied to its offset, which the copy gives back moved on.
        let bytes = self.temporary(bytes_type, node);
        let sized = vec![Operand::Copy(Place::from(total))];
        self.assign(
            Place::from(bytes),
            Rvalue::NewArray(bytes_type, sized),
            node,
        );
        let cursor = self.temporary(int, node);
        self.assign(Place::from(cursor), Rvalue::Use(zero), node);
        self.loop_nest(&lengths, &mut Vec::new(), node, &mut |builder, indices| {
            let text = item(indices);
            let arguments = vec![
                Argument::Borrow(text),
                Argument::InOut(Place::from(bytes)),
                Argument::Owned(Operand::Copy(Place::from(cursor))),
            ];
            builder.text_call(
                Place::from(cursor),
                Builtin::TextArray(TextArrayStep::CopyBytes),
                arguments,
                node,
            );
        });
        let buffers = vec![
            Operand::Move(Place::from(bytes)),
            Operand::Move(Place::from(offsets)),
        ];
        self.assign(place.clone(), Rvalue::Aggregate(ty, buffers), node);
    }

    /// Whether the subscript `node` is a slice of a text array, `names[1:3]`.
    pub(crate) fn is_text_slice(&self, node: NodeId) -> bool {
        let sliced = self
            .tree
            .child(node, Role::Index)
            .is_some_and(|index| self.tree.kind(index) == SyntaxKind::Slice);
        sliced
            && self
                .tree
                .child(node, Role::Value)
                .is_some_and(|value| self.is_text_array(value))
    }

    /// `names[mask]`, `names[positions]` or `names[i:j:k]`, the subscript `node` of the
    /// text array `value` with the index `index`, into `place`: a new text array holding a
    /// copy of each element chosen, made by the library's `text_masked` or `text_taken`.
    ///
    /// A slice of an array of numbers is a view; this one is a copy, since the elements of a
    /// text array lie end to end and cannot be assigned through a view anyway. A slice
    /// follows Python, its bounds clamped to the array; a mask of another length panics,
    /// and so does a position outside the array.
    pub(crate) fn text_selection(
        &mut self,
        place: Place,
        value: NodeId,
        index: NodeId,
        node: NodeId,
    ) {
        let source = self.borrow(value);
        let length = self.text_array_length(&source, node);
        let (function, by) = if self.tree.kind(index) == SyntaxKind::Slice {
            let count = self.held(self.int_type(), length, node);
            let Some(positions) = self.slice_positions(count, index, node) else {
                return;
            };
            ("text_taken", Place::from(positions))
        } else {
            let by = self.borrow(index);
            let by_type = self.ty(index);
            if self.element(by_type) == self.boolean_type() {
                let chosen = self.length(&by, by_type, 0, index);
                self.same_length(0, length, chosen, index);
                ("text_masked", by)
            } else {
                ("text_taken", by)
            }
        };
        let arguments = vec![source, by];
        let Some(result) = self.frame_call(function, Vec::new(), arguments, self.ty(node), node)
        else {
            return;
        };
        self.assign(place, Rvalue::Use(Operand::Move(Place::from(result))), node);
    }

    /// Whether `node` is a comparison of a text array, element by element: `names ==
    /// "alpha"` or `names < other`, which gives an `Array[bool, 1]`.
    pub(crate) fn is_text_comparison(&self, node: NodeId) -> bool {
        self.tree.kind(node) == SyntaxKind::Compare
            && self.is_array(self.ty(node))
            && self.compared(node).is_some_and(|(_, left, right)| {
                self.is_text_array(left) || self.is_text_array(right)
            })
    }

    /// The operator and the two sides of the comparison `node`, which has one link.
    fn compared(&self, node: NodeId) -> Option<(Operator, NodeId, NodeId)> {
        let comparator = self.tree.children_with(node, Role::Item).next()?;
        let left = self.tree.child(node, Role::Left)?;
        let right = self.tree.child(comparator, Role::Right)?;
        Some((self.tree.node(comparator).operator(), left, right))
    }

    /// The comparison `node` of a text array with another or with one `str`, into `place`:
    /// a new `Array[bool, 1]` with one element for each pair, made by the library's
    /// `text_compared` or `text_compared_to`. Text is compared as Python compares `str`. Two
    /// arrays of different lengths panic, as two arrays of numbers do. One `str` on the left
    /// is the same comparison turned round, `"m" < names` being `names > "m"`. Cost: one
    /// pass, comparing text.
    pub(crate) fn text_array_comparison(&mut self, place: Place, node: NodeId) {
        let Some((operator, left, right)) = self.compared(node) else {
            return;
        };
        let (array, other, turned) = if self.is_text_array(left) {
            (left, right, false)
        } else {
            (right, left, true)
        };
        // The numbers the library knows the comparisons by.
        let how = match (operator, turned) {
            (Operator::Equal, _) => 0,
            (Operator::NotEqual, _) => 1,
            (Operator::Less, false) | (Operator::Greater, true) => 2,
            (Operator::LessEqual, false) | (Operator::GreaterEqual, true) => 3,
            (Operator::Greater, false) | (Operator::Less, true) => 4,
            (Operator::GreaterEqual, false) | (Operator::LessEqual, true) => 5,
            _ => return self.unsupported(node, "this comparison of an array of `str`"),
        };
        let int = self.int_type();
        let source = self.borrow(array);
        let with = self.borrow(other);
        let function = if self.is_text_array(other) {
            let length = self.text_array_length(&source, node);
            let other_length = self.text_array_length(&with, other);
            self.same_length(0, length, other_length, node);
            "text_compared"
        } else {
            "text_compared_to"
        };
        let how = Operand::Constant(Constant::integer(int, how));
        let how = Place::from(self.held(int, how, node));
        let arguments = vec![source, with, how];
        let Some(result) = self.frame_call(function, Vec::new(), arguments, self.ty(node), node)
        else {
            return;
        };
        self.assign(place, Rvalue::Use(Operand::Move(Place::from(result))), node);
    }

    /// `text in names` or `text not in names` into `place`: whether an element of the text
    /// array `sides.1` equals the `str` `sides.0`, found by the library's `text_found`, which
    /// stops at the first one. Cost: at most one pass, comparing text.
    pub(crate) fn text_array_contains(
        &mut self,
        place: Place,
        operator: Operator,
        (text, array): (NodeId, NodeId),
        node: NodeId,
    ) {
        let boolean = self.boolean_type();
        let value = self.borrow(text);
        let source = self.borrow(array);
        let Some(found) =
            self.frame_call("text_found", Vec::new(), vec![source, value], boolean, node)
        else {
            return;
        };
        let found = Operand::Copy(Place::from(found));
        let result = if operator == Operator::NotIn {
            Rvalue::Unary(UnaryOperator::Not, found)
        } else {
            Rvalue::Use(found)
        };
        self.assign(place, result, node);
    }
}
