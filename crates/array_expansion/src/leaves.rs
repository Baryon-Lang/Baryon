//! The array leaves of a whole-array expression, and the fused subexpressions whose leaves are
//! checked where their statements were.

use crate::place_types::{Shape, Types};
use mir::{Elementwise, Operand, Place};
use source::Span;
use types::Type;

/// One leaf of a whole-array expression that is an array.
pub(crate) struct ArrayLeaf<'db> {
    /// Where the array is.
    pub(crate) place: Place,
    /// Its element type.
    pub(crate) element: Type<'db>,
    /// Its shape.
    pub(crate) shape: Shape,
    /// Whether the expression moves it, so expansion drops it after the loop.
    pub(crate) moved: bool,
}

/// Every array leaf of `tree`, in order from the left, fused subexpressions included. The first
/// sets the shape.
pub(crate) fn array_leaves<'db>(
    types: &Types<'_, 'db>,
    locals: &[mir::LocalDeclaration<'db>],
    tree: &Elementwise<'db>,
) -> Vec<ArrayLeaf<'db>> {
    let mut leaves = Vec::new();
    collect_leaves(types, locals, tree, true, &mut leaves);
    leaves
}

/// The array leaves whose lengths `tree` checks against each other itself: its own leaves, and
/// for each [`Fused`](Elementwise::Fused) subexpression only its first array leaf, which has the
/// shape of the subexpression's result. The subexpression checks the rest of its leaves itself.
pub(crate) fn shape_leaves<'db>(
    types: &Types<'_, 'db>,
    locals: &[mir::LocalDeclaration<'db>],
    tree: &Elementwise<'db>,
) -> Vec<ArrayLeaf<'db>> {
    let mut leaves = Vec::new();
    collect_leaves(types, locals, tree, false, &mut leaves);
    leaves
}

/// Appends the array leaves of `tree` to `leaves`, from the left: every leaf when `inside_fused`
/// is set, and otherwise one leaf for each fused subexpression, as [`shape_leaves`] describes.
fn collect_leaves<'db>(
    types: &Types<'_, 'db>,
    locals: &[mir::LocalDeclaration<'db>],
    tree: &Elementwise<'db>,
    inside_fused: bool,
    leaves: &mut Vec<ArrayLeaf<'db>>,
) {
    match tree {
        Elementwise::Leaf(operand) => {
            let (place, moved) = match operand {
                Operand::Copy(place) => (place, false),
                Operand::Move(place) => (place, true),
                Operand::Constant(_) => return,
            };
            let array = locals
                .get(place.local.index())
                .and_then(|local| types.of(local.ty, place))
                .and_then(|ty| types.array(ty));
            if let Some((element, shape)) = array {
                leaves.push(ArrayLeaf {
                    place: place.clone(),
                    element,
                    shape,
                    moved,
                });
            }
        }
        Elementwise::Unary(_, operand) => {
            collect_leaves(types, locals, operand, inside_fused, leaves);
        }
        Elementwise::Binary(_, left, right) => {
            collect_leaves(types, locals, left, inside_fused, leaves);
            collect_leaves(types, locals, right, inside_fused, leaves);
        }
        Elementwise::Fused(subexpression, _) if inside_fused => {
            collect_leaves(types, locals, subexpression, true, leaves);
        }
        Elementwise::Fused(subexpression, _) => {
            let mut own = Vec::new();
            collect_leaves(types, locals, subexpression, false, &mut own);
            leaves.extend(own.into_iter().next());
        }
    }
}

/// The fused subexpressions of `tree`, nested ones included, with their spans, in the order
/// their statements ran. Fusion takes them from the statements of one block, which run in the
/// order they are written, so that is the order of their spans.
pub(crate) fn fused<'a, 'db>(tree: &'a Elementwise<'db>) -> Vec<(&'a Elementwise<'db>, Span)> {
    let mut found = Vec::new();
    let mut pending = vec![tree];
    while let Some(node) = pending.pop() {
        match node {
            Elementwise::Leaf(_) => {}
            Elementwise::Unary(_, operand) => pending.push(operand),
            Elementwise::Binary(_, left, right) => pending.extend([&**right, &**left]),
            Elementwise::Fused(subexpression, span) => {
                found.push((&**subexpression, *span));
                pending.push(subexpression);
            }
        }
    }
    found.sort_by_key(|(_, span)| span.start());
    found
}
