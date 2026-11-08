//! Unpacking assignment: `a, b = value`, where `value` is a tuple or a rank-1 `Fixed` array.
//!
//! The value is computed once, into a temporary, before any target is assigned, then each target
//! takes its part from left to right, as Python does. So `u, out = out, u` swaps two arrays: the
//! tuple display moves both into the temporary, and each moves out into its new owner, so nothing
//! is copied (see tuple assignment in the reference's values and references chapter).

use crate::builder::Builder;
use mir::{Constant, Local, Place, Projection, Rvalue};
use syntax::{NodeId, Role};
use types::{Type, TypeKind};

impl<'db> Builder<'db> {
    /// `a, b, … = value`, with the tuple of targets `targets`.
    pub(crate) fn unpack(&mut self, targets: NodeId, value: NodeId) {
        let names: Vec<NodeId> = self.tree.children_with(targets, Role::Item).collect();
        let ty = self.ty(value);
        let Some(parts) = self.parts(ty) else {
            return self.unsupported(value, "unpacking a value that is not a tuple or a `Fixed`");
        };
        if parts.len() != names.len() {
            // The checker reports the count; nothing sensible lowers.
            return self.unsupported(targets, "unpacking into a different number of targets");
        }
        let whole = self.temporary(ty, value);
        self.write(Place::from(whole), ty, value);
        for (target, (projection, from)) in names.into_iter().zip(parts) {
            let source = match projection {
                Part::Field(field) => Place::from(whole).project(Projection::Field(field)),
                Part::Element(position) => {
                    let int = self.int_type();
                    let index = self.temporary(int, target);
                    let constant = Constant::integer(int, i128::from(position));
                    self.assign(
                        Place::from(index),
                        Rvalue::Use(mir::Operand::Constant(constant)),
                        target,
                    );
                    Place::from(whole).project(Projection::Index(vec![index]))
                }
            };
            self.assign_part(target, source, from);
        }
        self.drop_later(whole);
    }

    /// Assigns the part of the unpacked value at `source`, of type `from`, to `target`, which
    /// may itself be a tuple of targets.
    fn assign_part(&mut self, target: NodeId, source: Place, from: Type<'db>) {
        if self.tree.kind(target) == syntax::SyntaxKind::Tuple {
            return self.unsupported(target, "nested unpacking");
        }
        let Some((place, to)) = self.target(target) else {
            return;
        };
        let rvalue = self.coerce(self.take(source, from), from, to);
        if !self.needs_drop(to) {
            return self.assign(place, rvalue, target);
        }
        let temporary: Local = self.temporary(to, target);
        self.assign(Place::from(temporary), rvalue, target);
        self.replace(place, mir::Operand::Move(Place::from(temporary)), target);
    }

    /// Where each part of a value of type `ty` lives, and its type: the fields of a tuple, or
    /// the elements of a rank-1 `Fixed` array. `None` for any other type.
    fn parts(&self, ty: Type<'db>) -> Option<Vec<(Part, Type<'db>)>> {
        match ty.kind(self.db) {
            TypeKind::Tuple(items) => Some(
                (0u32..)
                    .zip(items.iter())
                    .map(|(field, &item)| (Part::Field(field), item))
                    .collect(),
            ),
            TypeKind::Fixed { element, shape } => match shape.lengths() {
                &[length] => Some(
                    (0..length)
                        .map(|position| (Part::Element(position), *element))
                        .collect(),
                ),
                _ => None,
            },
            _ => None,
        }
    }
}

/// Where one part of an unpacked value lives.
#[derive(Clone, Copy)]
enum Part {
    /// A tuple's field, by position.
    Field(u32),
    /// A rank-1 `Fixed` array's element, by position.
    Element(u64),
}
