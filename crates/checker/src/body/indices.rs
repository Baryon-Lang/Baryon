//! Indices that are values but not integers (roadmap 1.9.53). The `index` check lets such a
//! value through, since a rank-1 `Array` takes one as a choice of elements, `a[mask]` or
//! `a[positions]` (the `selection` module). Everywhere else, a list, a tuple, a struct or an
//! array as an index would reach the backend, so it is reported here:
//!
//! ```text
//! error: an index must be an integer, found `list[i64]`
//! ```

use super::{Body, relations};
use syntax::NodeId;
use types::{Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// The error for the first of `indices` that is a value but not an integer, such as a list
    /// in `xs[[0, 2]]` or a struct in `grid[p, 0]`, after `index` let it through for a choice of
    /// elements; `None` when each is an integer, a slice, a `None` new axis or of unknown type.
    pub(super) fn whole_indices(&mut self, indices: &[NodeId]) -> Option<Type<'db>> {
        let found = indices.iter().find_map(|&index| {
            let ty = self.typed.expression(index)?;
            let integer = matches!(ty.kind(self.db), TypeKind::Primitive(Primitive::Integer(_)));
            (!integer && !relations::is_open(self.db, ty) && !self.is_scalar(ty))
                .then_some((index, ty))
        })?;
        let (index, ty) = found;
        let message = format!("an index must be an integer, found `{}`", self.show(ty));
        Some(self.lowerer.error(
            index,
            &message,
            "not an integer",
            Some("only a rank-1 `Array` takes a choice of elements, `a[mask]` or `a[positions]`"),
        ))
    }
}
