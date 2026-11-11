//! `x op= value` on a whole array (language reference, "Arrays", "Broadcasting"): the result
//! keeps `x`'s shape, as `NumPy`'s in-place operators do, so it is written into `x`'s own array
//! through a view of all of it, which array expansion fills in place. An operand that would
//! broadcast `x` to a larger shape panics, where `x = x op value` would give a new, larger
//! array (roadmap 3.9).

use crate::builder::Builder;
use mir::{BinaryOperator, Constant, Elementwise, Operand, Place, Projection, Rvalue, SliceIndex};
use syntax::NodeId;
use types::{Type, TypeKind};

impl<'db> Builder<'db> {
    /// `target op= value` on an array. On an `Array` reached through fields alone, it is
    /// written in place through a view of all of `target`, so the result keeps `target`'s
    /// shape, as `NumPy`'s `+=` does: an operand that would broadcast it larger panics instead
    /// (roadmap 3.9). Otherwise it is `target = target op value` over every element.
    pub(crate) fn augmented_array(
        &mut self,
        place: Place,
        ty: Type<'db>,
        operator: BinaryOperator,
        value: NodeId,
        node: NodeId,
    ) {
        let Some(mut right) = self.tree_of(value, self.element(ty)) else {
            return;
        };
        let through_fields = place
            .projections
            .iter()
            .all(|projection| matches!(projection, Projection::Field(_)));
        if let TypeKind::Array { rank, .. } = ty.kind(self.db)
            && through_fields
        {
            let view = self.whole_view(&place, *rank, node);
            read_through(&mut right, &place, &view);
            return self.update_view(view, operator, right, node);
        }
        let tree = Elementwise::Binary(
            operator,
            Box::new(Elementwise::Leaf(Operand::Copy(place.clone()))),
            Box::new(right),
        );
        let temporary = self.temporary(ty, node);
        self.assign(
            Place::from(temporary),
            Rvalue::Elementwise(Box::new(tree)),
            node,
        );
        if self.needs_drop(ty) {
            self.push(mir::StatementKind::Drop(place.clone()), node);
        }
        let moved = self.take(Place::from(temporary), ty);
        self.assign(place, Rvalue::Use(moved), node);
    }

    /// A view of all of the `Array` at `place`, of rank `rank`: every dimension from 0, its
    /// whole length, by step 1.
    fn whole_view(&mut self, place: &Place, rank: u32, node: NodeId) -> Place {
        let int = self.int_type();
        let constant = |this: &mut Self, value| {
            let local = this.temporary(int, node);
            let value = Operand::Constant(Constant::integer(int, value));
            this.assign(Place::from(local), Rvalue::Use(value), node);
            local
        };
        let (start, step) = (constant(self, 0), constant(self, 1));
        let mut ranges = Vec::new();
        for dimension in 0..rank {
            let length = self.temporary(int, node);
            let measured = Rvalue::Length(place.clone(), dimension);
            self.assign(Place::from(length), measured, node);
            ranges.push(SliceIndex::Range {
                start,
                length,
                step,
            });
        }
        place.clone().project(Projection::Slice(ranges))
    }
}

/// Makes every leaf of `tree` that reads the whole of `place` read `view` instead, so array
/// expansion sees that the expression reads the array it writes only at the element it writes,
/// and needs no copy of it.
fn read_through(tree: &mut Elementwise<'_>, place: &Place, view: &Place) {
    match tree {
        Elementwise::Leaf(Operand::Copy(read)) if read == place => *read = view.clone(),
        Elementwise::Leaf(_) => {}
        Elementwise::Unary(_, operand) | Elementwise::Fused(operand, _) => {
            read_through(operand, place, view);
        }
        Elementwise::Binary(_, left, right) => {
            read_through(left, place, view);
            read_through(right, place, view);
        }
    }
}
