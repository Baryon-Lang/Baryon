//! `*args` in a call of a `Callable` value (the decision 0026 amendment): a tuple's elements
//! passed one by one, so a callback takes `SciPy`'s `args=` as parameters of its own.
//!
//! A tuple whose type is known, `t: tuple[f64, f64]`, is spread here, one borrowed field per
//! argument. A `tuple[*Ts]` in a generic body is not: its length is known only per instance,
//! so it becomes an [`Argument::Spread`], which monomorphization expands.

use crate::builder::Builder;
use mir::{Argument, Projection};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Type, TypeDb, TypeKind};

/// Whether `ty` is `tuple[*Ts]`: a tuple whose only element is a variadic type parameter.
pub(crate) fn is_pack_tuple(db: &dyn TypeDb, ty: Type<'_>) -> bool {
    let TypeKind::Tuple(items) = ty.kind(db) else {
        return false;
    };
    matches!(items.as_slice(), [only] if matches!(only.kind(db), TypeKind::Unpacked(_)))
}

impl Builder<'_> {
    /// Pushes onto `passed` what `argument` passes to a `Callable` value: the argument
    /// borrowed, or for `*args` each element of the tuple, borrowed or spread.
    pub(crate) fn pass_to_value(&mut self, argument: NodeId, passed: &mut Vec<Argument>) {
        let tuple = (self.tree.kind(argument) == SyntaxKind::Starred)
            .then(|| self.tree.child(argument, Role::Value))
            .flatten();
        let Some(tuple) = tuple else {
            passed.push(Argument::Borrow(self.borrow(argument)));
            return;
        };
        let place = self.borrow(tuple);
        match self.ty(tuple).kind(self.db) {
            TypeKind::Tuple(items)
                if !items
                    .iter()
                    .any(|item| matches!(item.kind(self.db), TypeKind::Unpacked(_))) =>
            {
                let count = u32::try_from(items.len()).unwrap_or(u32::MAX);
                passed.extend((0..count).map(|field| {
                    Argument::Borrow(place.clone().project(Projection::Field(field)))
                }));
            }
            _ => passed.push(Argument::Spread(place)),
        }
    }
}
