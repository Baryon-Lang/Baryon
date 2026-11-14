//! Arrays with missing values, `Array[T | None, 1]` (decision 0045, part 3).
//!
//! One is stored as a tuple of its values, an `Array[T, 1]`, and its validity bitmap, an
//! `Array[u8, 1]` of one bit for each element ([`Type::optional_storage`]), so the stages
//! after lowering see only a tuple of two arrays. The loops are written in Baryon, in the
//! standard library's `<std>/frame_missing.byn` (decision 0038), and this module calls them
//! with the two arrays.

use crate::builder::Builder;
use crate::frames::erase;
use mir::{Constant, ConstantValue, Local, Operand, Place, Projection, Rvalue};
use syntax::NodeId;
use types::{Primitive, Type, TypeKind};

/// The position of the values in the tuple that stores an array with missing values.
pub(crate) const VALUES: u32 = 0;
/// The position of the validity bitmap in that tuple.
pub(crate) const VALIDITY: u32 = 1;

impl<'db> Builder<'db> {
    /// The type of an element that is there, the `T` of `Array[T | None, 1]`, when the
    /// checker gave the expression `node` such a type. The builder's own types are erased,
    /// so this asks the checker's.
    pub(crate) fn optional_array_element(&self, node: NodeId) -> Option<Type<'db>> {
        match self.typed.expression(node)?.kind(self.db) {
            TypeKind::OptionalArray(element) => Some(*element),
            _ => None,
        }
    }

    /// The number of elements of the array with missing values at `array`, an `i64`: the
    /// length of its values.
    pub(crate) fn optional_array_length(&mut self, array: &Place, node: NodeId) -> Operand<'db> {
        let values = array.clone().project(Projection::Field(VALUES));
        let length = self.temporary(self.int_type(), node);
        self.assign(Place::from(length), Rvalue::Length(values, 0), node);
        Operand::Copy(Place::from(length))
    }

    /// `len(values)` of the array with missing values `argument`, into `place`.
    pub(crate) fn optional_array_len(&mut self, place: Place, argument: NodeId, node: NodeId) {
        let array = self.borrow(argument);
        let length = self.optional_array_length(&array, node);
        self.assign(place, Rvalue::Use(length), node);
    }

    /// `Array(items)` with `items: list[T | None]` into `place`: a new array with missing
    /// values, built by the library's `optional_parts`. The list is borrowed, so it stays
    /// usable. Cost: one pass over the list and two allocations.
    pub(crate) fn optional_array_from_list(&mut self, place: &Place, list: NodeId, node: NodeId) {
        let Some(element) = self.optional_array_element(node) else {
            return self.unsupported(node, "an array with missing values of this type");
        };
        let Some(filler) = self.filler(element) else {
            return self.unsupported(node, "an array with missing values of this type");
        };
        let source = self.borrow(list);
        let filler = Place::from(self.held(element, Operand::Constant(filler), node));
        let stored = erase(self.db, self.ty(node));
        let arguments = vec![source, filler];
        if let Some(parts) =
            self.frame_call("optional_parts", vec![element], arguments, stored, node)
        {
            let parts = Operand::Move(Place::from(parts));
            self.assign(place.clone(), Rvalue::Use(parts), node);
        }
    }

    /// The value stored where an element of type `element` is missing: zero, or `False`.
    /// `None` when `element` is not a number or `bool`.
    pub(crate) fn filler(&self, element: Type<'db>) -> Option<Constant<'db>> {
        let value = match element.kind(self.db) {
            TypeKind::Primitive(Primitive::Bool) => ConstantValue::Bool(false),
            TypeKind::Primitive(Primitive::Integer(_)) => ConstantValue::Integer(0),
            TypeKind::Primitive(Primitive::Float(_)) => ConstantValue::Float(0),
            TypeKind::Primitive(Primitive::Complex(_)) => ConstantValue::Complex {
                real: 0,
                imaginary: 0,
            },
            _ => return None,
        };
        Some(Constant { ty: element, value })
    }

    /// `values[index]` on the array with missing values `value`: a temporary holding the
    /// element, a `T | None`, read by the library's `optional_element` after `index` is
    /// checked to be in range. `None` after an error is reported.
    pub(crate) fn optional_array_read(
        &mut self,
        value: NodeId,
        index: NodeId,
        node: NodeId,
    ) -> Option<Place> {
        let element = self.optional_array_element(value)?;
        let base = self.borrow(value);
        let length = self.optional_array_length(&base, node);
        let position = Place::from(self.index_within(Some(length), 0, index, false));
        let values = base.clone().project(Projection::Field(VALUES));
        let bits = base.project(Projection::Field(VALIDITY));
        let optional = Type::new(self.db, TypeKind::Optional(element));
        let arguments = vec![values, bits, position];
        let read = self.frame_call("optional_element", vec![element], arguments, optional, node)?;
        Some(Place::from(read))
    }

    /// `values.isna()`, when `missing`, or `values.notna()` on the array with missing values
    /// `array`, into `place`: a new `Array[bool, 1]`, built by the library's `missing`.
    /// Cost: one pass over the validity bitmap.
    pub(crate) fn optional_array_missing(
        &mut self,
        place: Place,
        array: NodeId,
        missing: bool,
        node: NodeId,
    ) {
        let base = self.borrow(array);
        let length = self.optional_array_length(&base, node);
        let bits = base.project(Projection::Field(VALIDITY));
        if let Some(result) = self.validity_mask(bits, length, missing, node) {
            let result = Operand::Move(Place::from(result));
            self.assign(place, Rvalue::Use(result), node);
        }
    }

    /// A new `Array[bool, 1]` of `count` elements from the validity bitmap at `bits`: true
    /// where an element is `None`, when `missing`, or where it is there. The caller owns
    /// it. `None` after a report. Cost: one pass over the bitmap and a byte for each element.
    pub(crate) fn validity_mask(
        &mut self,
        bits: Place,
        count: Operand<'db>,
        missing: bool,
        node: NodeId,
    ) -> Option<Local> {
        let boolean = self.boolean_type();
        let count = Place::from(self.held(self.int_type(), count, node));
        let wanted = Constant {
            ty: boolean,
            value: ConstantValue::Bool(missing),
        };
        let wanted = Place::from(self.held(boolean, Operand::Constant(wanted), node));
        let mask = self.array_of(boolean);
        let arguments = vec![bits, count, wanted];
        self.frame_call("missing", Vec::new(), arguments, mask, node)
    }
}
