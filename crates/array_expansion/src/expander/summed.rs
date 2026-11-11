//! Sums a whole-array expression without storing it (decision 0041): `(a * b).sum()` of floats.
//!
//! Lowering writes `e.sum()` as `t = e`, a call of [`Builtin::Sum`] on `t`, and a drop of `t`.
//! When nothing else names `t`, the expression is expanded as any other, with the same length
//! checks and broadcasting, but each element goes into a buffer of 128 elements instead of `t`.
//! The loop nest walks the elements in row-major order, the order `Sum` takes `t`'s, and at the
//! end of each leaf of `NumPy`'s pairwise tree the buffer is summed by `Sum` and the leaf's sum
//! kept. The runtime then adds the leaf sums in the tree's order, so the result has the bits
//! `Sum` of `t` would have.
//!
//! Cost: one pass over the operands, one runtime call per leaf of 64 to 128 elements, and one
//! array of one leaf sum per leaf, where storing `t` writes and reads every element once more.

use super::{Broadcast, Expander};
use crate::leaves::{ArrayLeaf, fused};
use mir::{
    Argument, BinaryOperator, BlockId, Body, Builtin, Callee, Elementwise, Local, LocalKind,
    Operand, Place, Projection, Rvalue, SliceIndex, StatementKind, TerminatorKind,
};
use types::{Primitive, Shape, Type, TypeKind};

/// The number of elements the buffer holds: the largest leaf of the pairwise tree.
const LEAF: u64 = 128;

/// A whole-array expression whose result is only summed: what [`summed`] found.
pub(crate) struct Summed<'db> {
    /// Where the sum goes: the destination of the call of [`Builtin::Sum`].
    pub(crate) destination: Place,
    /// The block the call continues to, which drops the temporary.
    pub(crate) target: BlockId,
    /// The position of that drop in the target block.
    pub(crate) drop: usize,
    /// The type of the buffer one leaf's values are written into: a `Fixed` vector of 128
    /// elements of the expression's float type.
    pub(crate) buffer: Type<'db>,
    /// The expression's float element type.
    pub(crate) element: Type<'db>,
}

/// Whether the statement at `position`, the last of `block`, assigns a whole-array expression
/// of floats to a temporary that the block's call of [`Builtin::Sum`] sums and its target drops,
/// with nothing else naming it.
pub(crate) fn summed<'db>(
    db: &'db dyn salsa::Database,
    body: &Body<'db>,
    block: usize,
    position: usize,
) -> Option<Summed<'db>> {
    let data = &body.blocks[block];
    if position + 1 != data.statements.len() {
        return None;
    }
    let StatementKind::Assign(stored, Rvalue::Elementwise(_)) = &data.statements[position].kind
    else {
        return None;
    };
    let declaration = body.locals.get(stored.local.index())?;
    let TypeKind::Array { element, .. } = declaration.ty.kind(db) else {
        return None;
    };
    let float = matches!(element.kind(db), TypeKind::Primitive(Primitive::Float(_)));
    let shape = Shape::new(vec![LEAF])?;
    let buffer = Type::new(
        db,
        TypeKind::Fixed {
            element: *element,
            shape,
        },
    );
    if !float || declaration.kind != LocalKind::Temporary || !stored.projections.is_empty() {
        return None;
    }
    let TerminatorKind::Call {
        callee: Callee::Builtin(Builtin::Sum),
        arguments,
        destination,
        target,
        error: None,
    } = &data.terminator.kind
    else {
        return None;
    };
    if arguments.as_slice() != [Argument::Borrow(stored.clone())] {
        return None;
    }
    let drop = body.blocks[target.index()]
        .statements
        .iter()
        .position(|statement| statement.kind == StatementKind::Drop(stored.clone()))?;
    let mentions: usize = body
        .blocks
        .iter()
        .map(|data| {
            data.statements
                .iter()
                .flat_map(|statement| statement.kind.locals())
                .chain(data.terminator.kind.locals())
                .filter(|local| *local == stored.local)
                .count()
        })
        .sum();
    (mentions == 3).then(|| Summed {
        destination: destination.clone(),
        target: *target,
        drop,
        buffer,
        element: *element,
    })
}

impl<'db> Expander<'_, '_, 'db> {
    /// Writes the sum of `tree` in one pass, as the module documentation describes, for the call
    /// as `summed` found it. `leaves` are the expression's array leaves.
    pub(crate) fn sum(
        mut self,
        tree: &Elementwise<'db>,
        leaves: &[ArrayLeaf<'db>],
        summed: Summed<'db>,
    ) {
        let Summed {
            destination,
            target,
            buffer,
            element,
            ..
        } = summed;
        let mut broadcast = Broadcast::default();
        for (subexpression, span) in fused(tree) {
            self.fused_lengths(subexpression, span, &mut broadcast);
        }
        let lengths = self.broadcast(tree, None, &mut broadcast);
        let count = self.local(self.i64);
        self.assign(Place::from(count), Rvalue::Use(self.constant(1)));
        for length in &lengths {
            self.assign(
                Place::from(count),
                Rvalue::Binary(
                    BinaryOperator::Multiply,
                    Operand::Copy(Place::from(count)),
                    length.clone(),
                ),
            );
        }
        let copy = |local: Local| Operand::Copy(Place::from(local));
        let number = self.local(self.i64);
        self.call(
            Builtin::PairwiseLeaves,
            vec![Argument::Owned(copy(count))],
            Place::from(number),
        );
        let sums_type = Type::new(self.db, TypeKind::Array { element, rank: 1 });
        let sums = self.local(sums_type);
        self.assign(
            Place::from(sums),
            Rvalue::NewArray(sums_type, vec![copy(number)]),
        );
        let buffer = self.local(buffer);
        let [filled, done, leaf, end, zero, one] = [0; 6].map(|_| self.local(self.i64));
        for (local, value) in [(filled, 0), (done, 0), (leaf, 0), (zero, 0), (one, 1)] {
            self.assign(Place::from(local), Rvalue::Use(self.constant(value)));
        }
        let arguments = vec![Argument::Owned(copy(count)), Argument::Owned(copy(done))];
        self.call(Builtin::PairwiseLeafEnd, arguments, Place::from(end));
        let state = OnePass {
            buffer,
            sums,
            filled,
            done,
            leaf,
            end,
            count,
            zero,
            one,
            element,
        };
        self.versioned(&broadcast, |this, scaled| {
            let Some((last, outer)) = lengths.split_last() else {
                return;
            };
            this.loop_nest(outer, |this, indices| {
                this.row(&state, last.clone(), |this, column| {
                    let mut all = indices.to_vec();
                    all.push(column);
                    let (value, _) = this.element(tree, leaves, &all, (&broadcast, scaled));
                    value
                });
            });
        });
        let written = view(sums, zero, leaf, one);
        self.call(
            Builtin::PairwiseCombine,
            vec![Argument::Owned(copy(count)), Argument::Borrow(written)],
            destination,
        );
        self.push(StatementKind::Drop(Place::from(sums)));
        self.drop_moved(leaves);
        self.finish(TerminatorKind::Goto(target), target);
    }

    /// Writes one row of `length` values into the buffer, `value` giving the value in each
    /// column, in runs that end at the row's end or a leaf's, whichever comes first. A run's loop
    /// has no branch, so it vectorizes as a stored expression's does. When a run ends a leaf,
    /// the buffer is summed into the next leaf sum and emptied.
    fn row(
        &mut self,
        state: &OnePass<'db>,
        length: Operand<'db>,
        value: impl FnOnce(&mut Self, Local) -> Operand<'db>,
    ) {
        let copy = |local: Local| Operand::Copy(Place::from(local));
        let column = self.local(self.i64);
        let (header, exit) = self.open_loop(column, length.clone());
        // The run: up to the row's end, or the leaf's if that comes first.
        let run = self.local(self.i64);
        let room = self.local(self.i64);
        self.assign(
            Place::from(run),
            Rvalue::Binary(BinaryOperator::Subtract, length, copy(column)),
        );
        self.assign(
            Place::from(room),
            Rvalue::Binary(BinaryOperator::Subtract, copy(state.end), copy(state.done)),
        );
        let shorter = self.local(self.bool());
        self.assign(
            Place::from(shorter),
            Rvalue::Binary(BinaryOperator::Less, copy(room), copy(run)),
        );
        let (cut, start) = (self.new_block(), self.new_block());
        self.finish(
            TerminatorKind::Branch {
                condition: copy(shorter),
                then: cut,
                otherwise: start,
            },
            cut,
        );
        self.assign(Place::from(run), Rvalue::Use(copy(room)));
        self.finish(TerminatorKind::Goto(start), start);
        self.loop_nest(&[copy(run)], |this, step| {
            let at = this.local(this.i64);
            this.assign(
                Place::from(at),
                Rvalue::Binary(BinaryOperator::Add, copy(column), copy(step[0])),
            );
            let slot = this.local(this.i64);
            this.assign(
                Place::from(slot),
                Rvalue::Binary(BinaryOperator::Add, copy(state.filled), copy(step[0])),
            );
            let element = value(this, at);
            let place = Place::from(state.buffer).project(Projection::Index(vec![slot]));
            this.assign(place, Rvalue::Use(element));
        });
        for counter in [state.filled, state.done, column] {
            self.assign(
                Place::from(counter),
                Rvalue::Binary(BinaryOperator::Add, copy(counter), copy(run)),
            );
        }
        let full = self.local(self.bool());
        self.assign(
            Place::from(full),
            Rvalue::Binary(BinaryOperator::Equal, copy(state.done), copy(state.end)),
        );
        let flush = self.new_block();
        self.finish(
            TerminatorKind::Branch {
                condition: copy(full),
                then: flush,
                otherwise: header,
            },
            flush,
        );
        let total = self.local(state.element);
        let values = view(state.buffer, state.zero, state.filled, state.one);
        self.call(
            Builtin::Sum,
            vec![Argument::Borrow(values)],
            Place::from(total),
        );
        let slot = Place::from(state.sums).project(Projection::Index(vec![state.leaf]));
        self.assign(slot, Rvalue::Use(copy(total)));
        self.assign(
            Place::from(state.leaf),
            Rvalue::Binary(BinaryOperator::Add, copy(state.leaf), self.constant(1)),
        );
        self.assign(Place::from(state.filled), Rvalue::Use(self.constant(0)));
        let arguments = vec![
            Argument::Owned(copy(state.count)),
            Argument::Owned(copy(state.done)),
        ];
        self.call(Builtin::PairwiseLeafEnd, arguments, Place::from(state.end));
        self.finish(TerminatorKind::Goto(header), exit);
    }

    /// Ends the block being written with a call of `builtin`, which cannot fail, and continues
    /// in a new block after it.
    fn call(&mut self, builtin: Builtin, arguments: Vec<Argument<'db>>, destination: Place) {
        let next = self.new_block();
        self.finish(
            TerminatorKind::Call {
                callee: Callee::Builtin(builtin),
                arguments,
                destination,
                target: next,
                error: None,
            },
            next,
        );
    }
}

/// The locals a one-pass sum keeps its state in.
struct OnePass<'db> {
    /// The `Fixed` vector of 128 elements the current leaf's values are written into.
    buffer: Local,
    /// The new array of leaf sums, one per leaf.
    sums: Local,
    /// How many values the buffer holds.
    filled: Local,
    /// How many values have been written in all.
    done: Local,
    /// How many leaf sums have been kept.
    leaf: Local,
    /// Where the current leaf ends, counted in values.
    end: Local,
    /// How many values there are.
    count: Local,
    /// The constant 0, a view's start.
    zero: Local,
    /// The constant 1, a view's step.
    one: Local,
    /// The float type of the values.
    element: Type<'db>,
}

/// The view of `length` elements of the rank-1 array or `Fixed` vector `local`, from `start` by
/// `step`.
fn view(local: Local, start: Local, length: Local, step: Local) -> Place {
    Place::from(local).project(Projection::Slice(vec![SliceIndex::Range {
        start,
        length,
        step,
    }]))
}
