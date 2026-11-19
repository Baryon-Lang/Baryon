//! Arrays with missing values, `Array[T | None, 1]` (decision 0045, part 3).
//!
//! One is stored as a tuple of its values, an `Array[T, 1]`, and its validity bitmap, an
//! `Array[u8, 1]` of one bit for each element ([`Type::optional_storage`]), so the stages
//! after lowering see only a tuple of two arrays. The loops are written in Baryon, in the
//! standard library's `<std>/frame_missing.byn` (decision 0038), and this module calls them
//! with the two arrays.

use crate::builder::Builder;
use crate::frames::erase;
use mir::{Constant, ConstantValue, Local, Operand, Place, Projection, Rvalue, TerminatorKind};
use syntax::{NodeId, Role, SyntaxKind};
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

    /// The type that stores the values of an array with missing values of `element`s: the
    /// two buffers of a text array for `str`, or an `Array[element, 1]`.
    pub(crate) fn values_type(&self, element: Type<'db>) -> Type<'db> {
        match element.kind(self.db) {
            TypeKind::Str => erase(self.db, Type::new(self.db, TypeKind::TextArray)),
            _ => self.array_of(element),
        }
    }

    /// Calls the library function that makes new values of `element`s from `arguments`:
    /// `function` with `element` as its type argument or, for `str`, the function named
    /// `text_` and then `function`, which takes text arrays. Gives the temporary that holds
    /// the values, which the caller owns. `None` after a report.
    pub(crate) fn values_call(
        &mut self,
        function: &str,
        element: Type<'db>,
        arguments: Vec<Place>,
        node: NodeId,
    ) -> Option<Local> {
        let returns = self.values_type(element);
        if matches!(element.kind(self.db), TypeKind::Str) {
            let name = format!("text_{function}");
            return self.frame_call(&name, Vec::new(), arguments, returns, node);
        }
        self.frame_call(function, vec![element], arguments, returns, node)
    }

    /// The number of elements of the array with missing values of `element`s at `array`, an
    /// `i64`: the length of its values.
    pub(crate) fn optional_array_length(
        &mut self,
        array: &Place,
        element: Type<'db>,
        node: NodeId,
    ) -> Operand<'db> {
        let values = array.clone().project(Projection::Field(VALUES));
        if matches!(element.kind(self.db), TypeKind::Str) {
            return self.text_array_length(&values, node);
        }
        let length = self.temporary(self.int_type(), node);
        self.assign(Place::from(length), Rvalue::Length(values, 0), node);
        Operand::Copy(Place::from(length))
    }

    /// `len(values)` of the array with missing values `argument`, into `place`.
    pub(crate) fn optional_array_len(&mut self, place: Place, argument: NodeId, node: NodeId) {
        let Some(element) = self.optional_array_element(argument) else {
            return self.unsupported(node, "`len` of this type");
        };
        let array = self.borrow(argument);
        let length = self.optional_array_length(&array, element, node);
        self.assign(place, Rvalue::Use(length), node);
    }

    /// `Array(items)` with `items: list[T | None]` into `place`: a new array with missing
    /// values, built by the library's `optional_parts`. The list is borrowed, so it stays
    /// usable. Cost: one pass over the list and two allocations.
    pub(crate) fn optional_array_from_list(&mut self, place: &Place, list: NodeId, node: NodeId) {
        let Some(element) = self.optional_array_element(node) else {
            return self.unsupported(node, "an array with missing values of this type");
        };
        let source = self.borrow(list);
        let stored = erase(self.db, self.ty(node));
        let parts = if matches!(element.kind(self.db), TypeKind::Str) {
            // Text needs no filler: the library stores an empty text under each `None`.
            self.frame_call(
                "text_optional_parts",
                Vec::new(),
                vec![source],
                stored,
                node,
            )
        } else {
            let Some(filler) = self.filler(element) else {
                return self.unsupported(node, "an array with missing values of this type");
            };
            let filler = Place::from(self.held(element, Operand::Constant(filler), node));
            let arguments = vec![source, filler];
            self.frame_call("optional_parts", vec![element], arguments, stored, node)
        };
        if let Some(parts) = parts {
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
    /// checked to be in range. An element of text is a copy that the caller owns. `None`
    /// after an error is reported.
    pub(crate) fn optional_array_read(
        &mut self,
        value: NodeId,
        index: NodeId,
        node: NodeId,
    ) -> Option<Place> {
        let element = self.optional_array_element(value)?;
        let base = self.borrow(value);
        let length = self.optional_array_length(&base, element, node);
        let position = Place::from(self.index_within(Some(length), 0, index, false));
        let values = base.clone().project(Projection::Field(VALUES));
        let bits = base.project(Projection::Field(VALIDITY));
        let optional = Type::new(self.db, TypeKind::Optional(element));
        let arguments = vec![values, bits, position];
        let read = if matches!(element.kind(self.db), TypeKind::Str) {
            self.frame_call(
                "text_optional_element",
                Vec::new(),
                arguments,
                optional,
                node,
            )?
        } else {
            self.frame_call("optional_element", vec![element], arguments, optional, node)?
        };
        Some(Place::from(read))
    }

    /// The type a `for` loop over an array with missing values gives its variable: `T | None`
    /// for the array's elements `inner`, and the unknown type when there are none.
    pub(crate) fn ty_or(&self, inner: Option<Type<'db>>) -> Type<'db> {
        match inner {
            Some(element) => Type::new(self.db, TypeKind::Optional(element)),
            None => Type::new(self.db, TypeKind::Unknown),
        }
    }

    /// The element at the position `at` of the array of numbers or `bool`s with missing
    /// values at `array`, for a `for` loop whose variable is an `element`, a `T | None`: a
    /// temporary the library's `optional_element` fills. The loop keeps `at` inside the
    /// array. Cost: one call that reads the bitmap and the values on each pass.
    pub(crate) fn optional_loop_element(
        &mut self,
        array: &Place,
        at: Local,
        element: Type<'db>,
        node: NodeId,
    ) -> Place {
        let inner = match element.kind(self.db) {
            TypeKind::Optional(inner) => *inner,
            _ => element,
        };
        let values = array.clone().project(Projection::Field(VALUES));
        let bits = array.clone().project(Projection::Field(VALIDITY));
        if matches!(inner.kind(self.db), TypeKind::Str) {
            return self.optional_text_loop_element((values, bits), at, element, node);
        }
        let arguments = vec![values, bits, Place::from(at)];
        let read = self.frame_call("optional_element", vec![inner], arguments, element, node);
        // After a report the program is not built, and the loop still needs a place to name.
        Place::from(read.unwrap_or_else(|| self.temporary(element, node)))
    }

    /// The element at the position `at` of the array of text with missing values stored as
    /// `values` and `bits`, for a `for` loop whose variable is an `element`, a `str | None`.
    /// Where the element is there it holds a `str` that borrows the array's bytes, as a
    /// loop over a text array gives, so nothing is copied and nothing is dropped; where it
    /// is not, `None`. Cost: one call that reads the bitmap on each pass.
    fn optional_text_loop_element(
        &mut self,
        (values, bits): (Place, Place),
        at: Local,
        element: Type<'db>,
        node: NodeId,
    ) -> Place {
        let item = self.temporary(element, node);
        let boolean = self.boolean_type();
        let arguments = vec![bits, Place::from(at)];
        let Some(there) = self.frame_call("_valid", Vec::new(), arguments, boolean, node) else {
            return Place::from(item);
        };
        let (present, absent, after) = (self.new_block(), self.new_block(), self.new_block());
        self.terminate(
            TerminatorKind::Branch {
                condition: Operand::Copy(Place::from(there)),
                then: present,
                otherwise: absent,
            },
            node,
        );
        self.start(present);
        let text = self.text_array_element(&values, at, node);
        let held = Rvalue::Aggregate(element, vec![Operand::Move(text)]);
        self.assign(Place::from(item), held, node);
        self.terminate(TerminatorKind::Goto(after), node);
        self.start(absent);
        let nothing = Operand::Constant(Constant {
            ty: element,
            value: ConstantValue::None,
        });
        self.assign(Place::from(item), Rvalue::Use(nothing), node);
        self.terminate(TerminatorKind::Goto(after), node);
        self.start(after);
        Place::from(item)
    }

    /// Whether `node` is `values[index]` naming one element of an array with missing values.
    pub(crate) fn is_optional_element(&self, node: NodeId) -> bool {
        self.tree.kind(node) == SyntaxKind::Subscript
            && !self.is_selection(node)
            && self
                .tree
                .child(node, Role::Value)
                .is_some_and(|value| self.optional_array_element(value).is_some())
    }

    /// `values[index] = value`, where `target` names one element of an array of numbers or
    /// `bool`s with missing values: the element becomes `value`, a number or `None`. The
    /// value is computed first, then `index` is checked to be in range, and the library's
    /// `set_optional_element` writes the stored value and its validity bit in place. Under
    /// a `None` it stores the filler, so the stored values never depend on what was there.
    pub(crate) fn optional_element_store(&mut self, target: NodeId, value: NodeId) {
        let (Some(array), Some(index)) = (
            self.tree.child(target, Role::Value),
            self.tree.child(target, Role::Index),
        ) else {
            return;
        };
        let Some(element) = self.optional_array_element(array) else {
            return;
        };
        let Some(filler) = self.filler(element) else {
            return self.unsupported(target, "assigning an element of this array");
        };
        let optional = Type::new(self.db, TypeKind::Optional(element));
        let item = self.temporary(optional, value);
        self.write(Place::from(item), optional, value);
        let Some(base) = self.changed_place(array) else {
            return;
        };
        let length = self.optional_array_length(&base, element, target);
        let position = Place::from(self.index_within(Some(length), 0, index, false));
        let filler = Place::from(self.held(element, Operand::Constant(filler), target));
        let changed = vec![
            base.clone().project(Projection::Field(VALUES)),
            base.project(Projection::Field(VALIDITY)),
        ];
        let read = vec![position, Place::from(item), filler];
        self.frame_update("set_optional_element", element, (changed, read), target);
    }

    /// Whether `node` is `names[index]` on an array of text with missing values, which
    /// names no place: each read copies the text into a new `str | None`.
    pub(crate) fn is_optional_text_item(&self, node: NodeId) -> bool {
        self.tree.kind(node) == SyntaxKind::Subscript
            && self.tree.child(node, Role::Value).is_some_and(|value| {
                self.optional_array_element(value)
                    .is_some_and(|element| matches!(element.kind(self.db), TypeKind::Str))
            })
            && self
                .typed
                .expression(node)
                .is_some_and(|ty| matches!(ty.kind(self.db), TypeKind::Optional(_)))
    }

    /// `names[index]`, the subscript `node` on an array of text with missing values, into
    /// `place`: a `str | None` that owns a copy of the text. Cost: one allocation, of the
    /// size of the text.
    pub(crate) fn optional_text_item(&mut self, place: Place, node: NodeId) {
        let (Some(value), Some(index)) = (
            self.tree.child(node, Role::Value),
            self.tree.child(node, Role::Index),
        ) else {
            return;
        };
        if let Some(read) = self.optional_array_read(value, index, node) {
            self.assign(place, Rvalue::Use(Operand::Move(read)), node);
        }
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
        let Some(element) = self.optional_array_element(array) else {
            return self.unsupported(node, "`isna` of this value");
        };
        let base = self.borrow(array);
        let length = self.optional_array_length(&base, element, node);
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
