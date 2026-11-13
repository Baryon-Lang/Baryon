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
    TextArrayStep,
};
use syntax::{NodeId, Role, SyntaxKind};
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
    /// text column, which is stored as a tuple of its two buffers, or the length of an array.
    pub(crate) fn column_length(
        &mut self,
        column: Place,
        stored: Option<Type<'db>>,
        node: NodeId,
    ) -> Operand<'db> {
        if stored.is_some_and(|stored| matches!(stored.kind(self.db), TypeKind::Tuple(_))) {
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
}
