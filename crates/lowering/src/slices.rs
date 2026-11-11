//! Slices (language reference, "Arrays", "Slicing", and "Values and references", "Slices"):
//! `a[i:j]`, `a[i:j:k]` and `grid[:, k]` as views, places that end in a
//! [`Slice`](mir::Projection::Slice) step, with the checks that keep each range inside its
//! dimension.
//!
//! A view is a borrow, so it is used where a borrow can be:
//!
//! - **Read**: borrowed by a call, read element by element as a leaf of a whole-array
//!   expression, or copied into a new array where a value is needed, as `a[1:n].copy()` would.
//! - **Passed `InOut`**: the elements are copied into a new array, the call changes that, and
//!   the elements are copied back into the view after it returns or raises. The callee may
//!   assign its parameter a new array, which a view could not take without freeing memory it
//!   does not own; copying back checks the new array has the view's shape, so a change of shape
//!   panics. The copies cost one pass over the view each way.
//! - **Assigned into**: `a[1:n] = value` and `a[1:n] += value` write the view's elements in
//!   place, as one whole-array expression whose destination is the view. A number on the right
//!   goes to every element.
//!
//! A range is lowered to its first index, its number of elements and its step. Python's rules
//! give the defaults: from 0 to the length, or from the last element back through the first for
//! a negative step. A negative literal counts from the end. Each bound the program writes must
//! lie from 0 to the length, the range's first element must be inside the dimension when it
//! takes any, and a step that is not a constant must not be 0.

use crate::builder::Builder;
use mir::{
    Argument, BinaryOperator, Check, Constant, Elementwise, Local, Operand, Place, Projection,
    Rvalue, SliceIndex, StatementKind, TerminatorKind,
};
use syntax::{NodeId, Role, SyntaxKind};
use types::Type;

/// The bounds of a range the program wrote, each an `i64` operand; `None` for one it left out.
struct Given<'db> {
    /// Where the range starts.
    start: Option<Operand<'db>>,
    /// Where it stops.
    stop: Option<Operand<'db>>,
}

/// One range's bounds as lowering works them out, each an `i64` local.
struct Bounds {
    /// The first index taken.
    start: Local,
    /// Where the range stops: the first index not taken.
    stop: Local,
    /// The number of elements taken.
    length: Local,
    /// The distance from one element taken to the next.
    step: Local,
}

impl<'db> Builder<'db> {
    /// The view `node` names, `value[indices]` with at least one index a slice: the array's
    /// place followed by a [`Slice`](Projection::Slice) step, after the checks of every index
    /// and range. `None` after an error.
    pub(crate) fn view(
        &mut self,
        node: NodeId,
        value: NodeId,
        indices: &[NodeId],
    ) -> Option<Place> {
        let ty = self.ty(value);
        if !self.is_array(ty) {
            self.unsupported(node, "slices of lists and strings");
            return None;
        }
        let base = self.borrow(value);
        let mut dimensions = Vec::with_capacity(indices.len());
        for (dimension, &index) in (0u32..).zip(indices) {
            dimensions.push(if self.tree.kind(index) == SyntaxKind::Slice {
                self.slice_range(&base, ty, dimension, index)
            } else {
                SliceIndex::Index(self.index(&base, ty, dimension, index, false))
            });
        }
        Some(base.project(Projection::Slice(dimensions)))
    }

    /// The range `slice`, `start:stop:step`, of dimension `dimension` of the array at `array`
    /// of type `ty`, checked to lie within the dimension.
    pub(crate) fn slice_range(
        &mut self,
        array: &Place,
        ty: Type<'db>,
        dimension: u32,
        slice: NodeId,
    ) -> SliceIndex {
        let length = self.length(array, ty, dimension, slice);
        self.range_within(&length, dimension, slice)
    }

    /// The range `slice`, `start:stop:step`, of dimension `dimension` of something of length
    /// `length`, an array or a `str`, checked to lie within it.
    pub(crate) fn range_within(
        &mut self,
        length: &Operand<'db>,
        dimension: u32,
        slice: NodeId,
    ) -> SliceIndex {
        let int = self.int_type();
        let child = |role| self.tree.child(slice, role);
        let (lower, upper, step_node) = (child(Role::Lower), child(Role::Upper), child(Role::Step));
        let step = self.temporary(int, slice);
        let constant_step = step_node.map_or(Some(1), |node| self.signed_constant(node));
        let step_value = match (step_node, constant_step) {
            (_, Some(constant)) => Operand::Constant(Constant::integer(int, constant)),
            (Some(node), None) => self.operand_as(node, int),
            (None, None) => Operand::Constant(Constant::integer(int, 1)),
        };
        self.assign(Place::from(step), Rvalue::Use(step_value), slice);
        if constant_step.is_none_or(|constant| constant == 0) {
            let nonzero = self.compare_locals(BinaryOperator::NotEqual, step, None, slice);
            self.then(slice, |target| TerminatorKind::Assert {
                condition: Operand::Copy(Place::from(nonzero)),
                check: Check::NonZeroStep { slice: true },
                target,
            });
        }
        let bounds = Bounds {
            start: self.temporary(int, slice),
            stop: self.temporary(int, slice),
            length: self.temporary(int, slice),
            step,
        };
        let given = Given {
            start: lower.map(|node| self.bound(node, length)),
            stop: upper.map(|node| self.bound(node, length)),
        };
        if let Some(constant) = constant_step {
            self.directed(
                constant < 0,
                &bounds,
                &given,
                length,
                constant.abs() == 1,
                slice,
            );
        } else {
            let backwards = self.compare_locals(BinaryOperator::Less, step, None, slice);
            let (back, forth, join) = (self.new_block(), self.new_block(), self.new_block());
            self.terminate(
                TerminatorKind::Branch {
                    condition: Operand::Copy(Place::from(backwards)),
                    then: back,
                    otherwise: forth,
                },
                slice,
            );
            for (block, backwards) in [(back, true), (forth, false)] {
                self.start(block);
                self.directed(backwards, &bounds, &given, length, false, slice);
                self.terminate(TerminatorKind::Goto(join), slice);
            }
            self.start(join);
        }
        let may_go_back = constant_step.is_none_or(|constant| constant < 0);
        self.check_range(dimension, &bounds, &given, length, may_go_back, slice);
        SliceIndex::Range {
            start: bounds.start,
            length: bounds.length,
            step: bounds.step,
        }
    }

    /// The value of the bound `node` of a range as an `i64` operand, a negative literal
    /// counted back from `length`.
    fn bound(&mut self, node: NodeId, length: &Operand<'db>) -> Operand<'db> {
        let int = self.int_type();
        let Some(from_end) = self.negative_literal(node) else {
            return self.operand_as(node, int);
        };
        let local = self.temporary(int, node);
        self.assign(
            Place::from(local),
            Rvalue::Binary(
                BinaryOperator::Subtract,
                length.clone(),
                Operand::Constant(Constant::integer(int, from_end)),
            ),
            node,
        );
        Operand::Copy(Place::from(local))
    }

    /// The value of the integer literal `node`, or of `-k` for a literal `k`.
    fn signed_constant(&self, node: NodeId) -> Option<i128> {
        self.negative_literal(node)
            .map(|value| -value)
            .or_else(|| self.constant_integer(node))
    }

    /// Fills `bounds` for a range that walks forwards, or `backwards`: the defaults of the
    /// bounds the program left out, and the number of elements, `ceil(distance / |step|)` or 0.
    /// A `unit` step, 1 or -1 as a constant, takes every element, so the count is the distance.
    fn directed(
        &mut self,
        backwards: bool,
        bounds: &Bounds,
        given: &Given<'db>,
        length: &Operand<'db>,
        unit: bool,
        node: NodeId,
    ) {
        let int = self.int_type();
        let constant = |value| Operand::Constant(Constant::integer(int, value));
        let last = || {
            Rvalue::Binary(
                BinaryOperator::Subtract,
                length.clone(),
                Operand::Constant(Constant::integer(int, 1)),
            )
        };
        let start_value = match (&given.start, backwards) {
            (Some(written), _) => Rvalue::Use(written.clone()),
            (None, false) => Rvalue::Use(constant(0)),
            (None, true) => last(),
        };
        self.assign(Place::from(bounds.start), start_value, node);
        let stop_value = match (&given.stop, backwards) {
            (Some(written), _) => written.clone(),
            (None, false) => length.clone(),
            // Before the first element, which no bound the program writes can name.
            (None, true) => constant(-1),
        };
        self.assign(Place::from(bounds.stop), Rvalue::Use(stop_value), node);
        // distance = stop - start forwards, start - stop backwards; magnitude = |step|.
        let (from, to) = if backwards {
            (bounds.stop, bounds.start)
        } else {
            (bounds.start, bounds.stop)
        };
        let copy = |local: Local| Operand::Copy(Place::from(local));
        self.assign(
            Place::from(bounds.length),
            Rvalue::Binary(BinaryOperator::Subtract, copy(to), copy(from)),
            node,
        );
        if !unit {
            let magnitude = if backwards {
                let magnitude = self.temporary(int, node);
                self.assign(
                    Place::from(magnitude),
                    Rvalue::Binary(BinaryOperator::Subtract, constant(0), copy(bounds.step)),
                    node,
                );
                magnitude
            } else {
                bounds.step
            };
            // ceil(distance / magnitude) = floor((distance + magnitude - 1) / magnitude), for a
            // magnitude above 0; the step check has run.
            let rounded = self.temporary(int, node);
            self.assign(
                Place::from(rounded),
                Rvalue::Binary(BinaryOperator::Add, copy(bounds.length), copy(magnitude)),
                node,
            );
            self.assign(
                Place::from(rounded),
                Rvalue::Binary(BinaryOperator::Subtract, copy(rounded), constant(1)),
                node,
            );
            self.assign(
                Place::from(bounds.length),
                Rvalue::Binary(BinaryOperator::FloorDivide, copy(rounded), copy(magnitude)),
                node,
            );
        }
        // A range that ends before it starts takes nothing.
        let negative = self.compare_locals(BinaryOperator::Less, bounds.length, None, node);
        let (empty, done) = (self.new_block(), self.new_block());
        self.terminate(
            TerminatorKind::Branch {
                condition: Operand::Copy(Place::from(negative)),
                then: empty,
                otherwise: done,
            },
            node,
        );
        self.start(empty);
        self.assign(Place::from(bounds.length), Rvalue::Use(constant(0)), node);
        self.terminate(TerminatorKind::Goto(done), node);
        self.start(done);
    }

    /// Asserts that each bound the program wrote lies from 0 to `length`, and, when the step
    /// may be negative and the start was written, that a range taking any element starts
    /// inside the dimension. The defaults need no check: they lie inside by construction.
    fn check_range(
        &mut self,
        dimension: u32,
        bounds: &Bounds,
        given: &Given<'db>,
        length: &Operand<'db>,
        may_go_back: bool,
        node: NodeId,
    ) {
        let mut conditions = Vec::new();
        for (bound, local) in [(&given.start, bounds.start), (&given.stop, bounds.stop)] {
            if bound.is_some() {
                conditions.push(self.compare_locals(
                    BinaryOperator::GreaterEqual,
                    local,
                    None,
                    node,
                ));
                conditions.push(self.compare_locals(
                    BinaryOperator::LessEqual,
                    local,
                    Some(length.clone()),
                    node,
                ));
            }
        }
        if given.start.is_some() && may_go_back {
            // Taking nothing, or starting below the length.
            let empty = self.compare_locals(BinaryOperator::Equal, bounds.length, None, node);
            let inside = self.compare_locals(
                BinaryOperator::Less,
                bounds.start,
                Some(length.clone()),
                node,
            );
            let either = self.temporary(self.boolean_type(), node);
            self.assign(
                Place::from(either),
                Rvalue::Binary(
                    BinaryOperator::BitOr,
                    Operand::Copy(Place::from(empty)),
                    Operand::Copy(Place::from(inside)),
                ),
                node,
            );
            conditions.push(either);
        }
        let Some((&first, rest)) = conditions.split_first() else {
            return;
        };
        for &condition in rest {
            self.assign(
                Place::from(first),
                Rvalue::Binary(
                    BinaryOperator::BitAnd,
                    Operand::Copy(Place::from(first)),
                    Operand::Copy(Place::from(condition)),
                ),
                node,
            );
        }
        self.then(node, |target| TerminatorKind::Assert {
            condition: Operand::Copy(Place::from(first)),
            check: Check::SliceInBounds {
                dimension,
                start: Operand::Copy(Place::from(bounds.start)),
                stop: Operand::Copy(Place::from(bounds.stop)),
                length: length.clone(),
            },
            target,
        });
    }

    /// A new `bool` local holding `local operator right`, `right` being 0 when `None`.
    fn compare_locals(
        &mut self,
        operator: BinaryOperator,
        local: Local,
        right: Option<Operand<'db>>,
        node: NodeId,
    ) -> Local {
        let right =
            right.unwrap_or_else(|| Operand::Constant(Constant::integer(self.int_type(), 0)));
        let result = self.temporary(self.boolean_type(), node);
        self.assign(
            Place::from(result),
            Rvalue::Binary(operator, Operand::Copy(Place::from(local)), right),
            node,
        );
        result
    }

    /// A new array of type `ty` holding a copy of the elements of `view`, for a place that
    /// needs a value. The caller drops or moves it.
    pub(crate) fn copy_view(&mut self, view: Place, ty: Type<'db>, node: NodeId) -> Local {
        let copy = self.temporary(ty, node);
        self.assign(
            Place::from(copy),
            Rvalue::Elementwise(Box::new(Elementwise::Leaf(Operand::Copy(view)))),
            node,
        );
        copy
    }

    /// `view = value`: writes `value`, an array or a number for every element, into the
    /// elements of `view` in place.
    pub(crate) fn store_view(&mut self, view: Place, ty: Type<'db>, value: NodeId) {
        if let Some(tree) = self.tree_of(value, self.element(ty)) {
            self.assign(view, Rvalue::Elementwise(Box::new(tree)), value);
        }
    }

    /// `view op= value`: `view = view op value`, written in place.
    pub(crate) fn augmented_view(
        &mut self,
        view: Place,
        ty: Type<'db>,
        operator: BinaryOperator,
        value: NodeId,
        node: NodeId,
    ) {
        if let Some(right) = self.tree_of(value, self.element(ty)) {
            self.update_view(view, operator, right, node);
        }
    }

    /// `view = view op right`, written in place.
    pub(crate) fn update_view(
        &mut self,
        view: Place,
        operator: BinaryOperator,
        right: Elementwise<'db>,
        node: NodeId,
    ) {
        let tree = Elementwise::Binary(
            operator,
            Box::new(Elementwise::Leaf(Operand::Copy(view.clone()))),
            Box::new(right),
        );
        self.assign(view, Rvalue::Elementwise(Box::new(tree)), node);
    }

    /// The argument for passing `place` as an `InOut` parameter of type `ty`: the place itself,
    /// or for a view a new array holding a copy of its elements, which
    /// [`copy_back`](Self::copy_back) writes back after the call. The view, when there is one,
    /// comes back with the argument.
    pub(crate) fn in_out(
        &mut self,
        place: Place,
        ty: Type<'db>,
        node: NodeId,
    ) -> (Argument<'db>, Option<(Place, Local)>) {
        if !place.is_view() {
            return (Argument::InOut(place), None);
        }
        let copy = self.copy_view(place.clone(), ty, node);
        (Argument::InOut(Place::from(copy)), Some((place, copy)))
    }

    /// Writes each copy an `InOut` view argument was passed as back into its view, and drops
    /// the copy. Writing back checks the copy still has the view's shape.
    pub(crate) fn copy_back(&mut self, views: &[(Place, Local)], node: NodeId) {
        for (view, copy) in views {
            let leaf = Elementwise::Leaf(Operand::Copy(Place::from(*copy)));
            self.assign(view.clone(), Rvalue::Elementwise(Box::new(leaf)), node);
            self.push(StatementKind::Drop(Place::from(*copy)), node);
        }
    }
}
