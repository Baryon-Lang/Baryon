//! `y[i]` on a generic state (card 60, an amendment to decision 0065 point 1).
//!
//! Inside a body generic in `S: VectorSpace`, `y[i]` reads element `i` of the state as an
//! `f64`, and `len(y)` lowers to [`Rvalue::Length`] as it does for an `Array`. The element is
//! copied into an `f64` temporary at once, so the indexed place appears only on the right of
//! that one assignment; monomorphization then writes it for each instance, and for an `f64`
//! state, whose length is 1, it reads the number itself.

use crate::builder::Builder;
use mir::{Operand, Place, Projection, Rvalue};
use syntax::NodeId;
use types::{Float, Primitive, Type, TypeKind};

impl<'db> Builder<'db> {
    /// The `f64` temporary holding element `index` of the state `array`, after the bounds
    /// check unless `unchecked`.
    pub(crate) fn state_element(
        &mut self,
        array: NodeId,
        index: NodeId,
        unchecked: bool,
        node: NodeId,
    ) -> Place {
        let ty = self.ty(array);
        let base = self.borrow(array);
        let position = self.index(&base, ty, 0, index, unchecked);
        let element = Type::new(self.db, TypeKind::Primitive(Primitive::Float(Float::F64)));
        let temporary = self.temporary(element, node);
        let read = Operand::Copy(base.project(Projection::Index(vec![position])));
        self.assign(Place::from(temporary), Rvalue::Use(read), node);
        Place::from(temporary)
    }

    /// Whether `len` of a value of type `ty` lowers to [`Rvalue::Length`]: an array, a list, a
    /// dict, or a generic state, whose length monomorphization writes for each instance.
    pub(crate) fn has_length(&self, ty: Type<'db>) -> bool {
        self.is_array(ty)
            || matches!(
                ty.kind(self.db),
                TypeKind::List(_) | TypeKind::Dict { .. } | TypeKind::Parameter { .. }
            )
    }
}
