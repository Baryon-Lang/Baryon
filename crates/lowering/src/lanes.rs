//! `simd` loops that sum or multiply floats (roadmap 1.9.14; decision 0011).
//!
//! A float sum gives the same bits only when its additions happen in the same order, and a
//! vectorized loop regroups them by the machine's vector width. So a `simd` loop that sums or
//! multiplies into a shared float does not reduce into the variable directly. It keeps
//! [`LANES`] private *lanes*, starting at the operator's identity, and iteration `i`, counted
//! from 0, combines into lane `i % 8`. After the loop the lanes combine in a fixed pairwise
//! tree, `((l0 ⊕ l1) ⊕ (l2 ⊕ l3)) ⊕ ((l4 ⊕ l5) ⊕ (l6 ⊕ l7))`, and the variable's value from
//! before the loop comes in last, as a `prange` reduction's partials do.
//!
//! The lanes are one `Fixed[T, (8,)]` local per variable, and the loop of `n` iterations runs
//! as whole blocks of 8, then the rest:
//!
//! ```text
//! k = 0
//! while k < n - 7:                # whole blocks
//!     for j in range(8):          # marked vectorized
//!         i = start + (k + j) * step
//!         body, with `total += x` written as `lanes[j] += x`
//!     k += 8
//! rest = lanes                    # a copy
//! for j in range(n - k):          # marked vectorized: the last iterations, fewer than 8
//!     i = start + (k + j) * step
//!     body, with `total += x` written as `rest[j] += x`
//! total = total + tree(rest)
//! ```
//!
//! The result is the one the IR spells out, on every machine, whether LLVM vectorizes the loop
//! or not. Its speed comes from LLVM: it unrolls the inner loop of 8, since the count is a
//! constant, which leaves `lanes` indexed by constants, so they live in eight registers and the
//! eight additions of a block do not wait on each other; it can then pack them into vector
//! instructions. The rest goes into a copy so that its index, not a constant, does not keep
//! `lanes` in memory. The body is lowered twice, once per loop.

use crate::builder::{Builder, Loop};
use mir::{
    BinaryOperator, Constant, ConstantValue, Local, Operand, Place, Projection, ReductionOperator,
    Rvalue, TerminatorKind,
};
use syntax::{NodeId, Role};
use types::{Shape, Type, TypeKind};

/// How many lanes a `simd` float reduction keeps: 8, fixed by the language rather than by the
/// machine's vector width, so the grouping of the additions is the same everywhere. Eight `f64`
/// fill one 512-bit vector, two 256-bit ones or four 128-bit ones.
const LANES: u64 = 8;

impl<'db> Builder<'db> {
    /// The lane that `place` reduces into in the current iteration, when `place` is a whole
    /// variable the enclosing `simd` loop keeps lanes for.
    pub(crate) fn lane(&self, place: &Place) -> Option<Place> {
        if !place.projections.is_empty() {
            return None;
        }
        self.lanes
            .iter()
            .find(|(local, _)| *local == place.local)
            .map(|(_, lane)| lane.clone())
    }

    /// The `simd` loop `statement` over `range`, its start, stop and step, whose variable is the
    /// `i64` `variable` and whose body sums or multiplies into each of `reductions`: the body
    /// runs once per iteration in order, each reduction combining into its lane, then the lanes
    /// combine into the variables (the module documentation).
    pub(crate) fn laned_for(
        &mut self,
        statement: NodeId,
        variable: Local,
        (first, bound, step): (Option<NodeId>, NodeId, i128),
        reductions: &[(Local, ReductionOperator)],
    ) {
        let (Some(iterator), Some(shape)) = (
            self.tree.child(statement, Role::Iterator),
            Shape::new(vec![LANES]),
        ) else {
            return;
        };
        let int = self.int_type();
        let integer = |value: i128| Operand::Constant(Constant::integer(int, value));
        let copy = |local: Local| Operand::Copy(Place::from(local));
        let start = self.temporary(int, iterator);
        match first {
            Some(first) => self.write(Place::from(start), int, first),
            None => self.assign(Place::from(start), Rvalue::Use(integer(0)), iterator),
        }
        let count = self.count(start, bound, step, iterator);

        let lanes: Vec<(Local, Local, ReductionOperator)> = reductions
            .iter()
            .map(|&(local, operator)| {
                let ty = self.local_type(local);
                let fixed = Type::new(
                    self.db,
                    TypeKind::Fixed {
                        element: ty,
                        shape: shape.clone(),
                    },
                );
                let lanes = self.temporary(fixed, iterator);
                let identity = Operand::Constant(identity(ty, operator));
                let filled = (0..LANES).map(|_| identity.clone()).collect();
                self.assign(
                    Place::from(lanes),
                    Rvalue::Aggregate(fixed, filled),
                    iterator,
                );
                (local, lanes, operator)
            })
            .collect();

        // Whole blocks of 8 iterations, while `block + 8 <= count`.
        let block = self.temporary(int, iterator);
        self.assign(Place::from(block), Rvalue::Use(integer(0)), iterator);
        let last = self.temporary(int, iterator);
        let whole = Rvalue::Binary(BinaryOperator::Subtract, copy(count), integer(7));
        self.assign(Place::from(last), whole, iterator);
        let (outer, outer_body, tail) = (self.new_block(), self.new_block(), self.new_block());
        self.terminate(TerminatorKind::Goto(outer), statement);
        self.start(outer);
        let more = self.temporary(self.boolean_type(), iterator);
        let below = Rvalue::Binary(BinaryOperator::Less, copy(block), copy(last));
        self.assign(Place::from(more), below, iterator);
        self.terminate(
            TerminatorKind::Branch {
                condition: copy(more),
                then: outer_body,
                otherwise: tail,
            },
            iterator,
        );
        self.start(outer_body);
        let whole: Vec<(Local, Local)> = lanes
            .iter()
            .map(|&(local, lanes, _)| (local, lanes))
            .collect();
        let reported = self.reported();
        let full = integer(LANES.into());
        self.lane_loop(statement, variable, (start, step, block), full, &whole);
        let next = Rvalue::Binary(BinaryOperator::Add, copy(block), integer(LANES.into()));
        self.assign(Place::from(block), next, iterator);
        self.terminate(TerminatorKind::Goto(outer), statement);

        // The last `count - block` iterations, fewer than 8, into a copy of the lanes. The
        // copy leaves the whole blocks' lanes indexed only by constants once LLVM unrolls
        // their loop, so they can live in registers.
        self.start(tail);
        let left = self.temporary(int, iterator);
        let rest = Rvalue::Binary(BinaryOperator::Subtract, copy(count), copy(block));
        self.assign(Place::from(left), rest, iterator);
        let copies: Vec<(Local, Local)> = lanes
            .iter()
            .map(|&(local, lanes, _)| {
                let copied = self.temporary(self.local_type(lanes), iterator);
                self.assign(Place::from(copied), Rvalue::Use(copy(lanes)), iterator);
                (local, copied)
            })
            .collect();
        if self.reported() == reported {
            // A body that reported an error fails to lower; once is enough to say so.
            self.lane_loop(
                statement,
                variable,
                (start, step, block),
                copy(left),
                &copies,
            );
        }
        for (&(local, _, operator), &(_, copied)) in lanes.iter().zip(&copies) {
            self.combine_lanes(local, copied, operator, iterator);
        }
    }

    /// `for lane in range(length):` over the iterations from `block`: sets `variable` to
    /// `start + (block + lane) * step`, then runs the body of `statement` with each variable of
    /// `lanes` reducing into element `lane` of its lanes. Continues after the loop.
    fn lane_loop(
        &mut self,
        statement: NodeId,
        variable: Local,
        (start, step, block): (Local, i128, Local),
        length: Operand<'db>,
        lanes: &[(Local, Local)],
    ) {
        let Some(iterator) = self.tree.child(statement, Role::Iterator) else {
            return;
        };
        let int = self.int_type();
        let integer = |value: i128| Operand::Constant(Constant::integer(int, value));
        let copy = |local: Local| Operand::Copy(Place::from(local));
        let lane = self.temporary(int, iterator);
        self.assign(Place::from(lane), Rvalue::Use(integer(0)), iterator);
        let (header, body, next, end) = (
            self.new_block(),
            self.new_block(),
            self.new_block(),
            self.new_block(),
        );
        self.terminate(TerminatorKind::Goto(header), statement);
        self.start(header);
        self.body.vectorized.push(header);
        let inside = self.temporary(self.boolean_type(), iterator);
        let below = Rvalue::Binary(BinaryOperator::Less, copy(lane), length);
        self.assign(Place::from(inside), below, iterator);
        self.terminate(
            TerminatorKind::Branch {
                condition: copy(inside),
                then: body,
                otherwise: end,
            },
            iterator,
        );
        self.start(body);
        let iteration = self.temporary(int, iterator);
        let sum = Rvalue::Binary(BinaryOperator::Add, copy(block), copy(lane));
        self.assign(Place::from(iteration), sum, iterator);
        let offset = self.temporary(int, iterator);
        let scaled = Rvalue::Binary(BinaryOperator::Multiply, copy(iteration), integer(step));
        self.assign(Place::from(offset), scaled, iterator);
        let value = Rvalue::Binary(BinaryOperator::Add, copy(start), copy(offset));
        self.assign(Place::from(variable), value, iterator);

        let enclosing = std::mem::take(&mut self.lanes);
        self.lanes = lanes
            .iter()
            .map(|&(local, lanes)| {
                let place = Place::from(lanes).project(Projection::Index(vec![lane]));
                (local, place)
            })
            .chain(enclosing.iter().cloned())
            .collect();
        self.loops.push(Loop { next, end });
        self.block(statement, Role::Body);
        self.loops.pop();
        self.lanes = enclosing;
        self.jump(&mut Some(next), statement);
        self.start(next);
        let step_lane = Rvalue::Binary(BinaryOperator::Add, copy(lane), integer(1));
        self.assign(Place::from(lane), step_lane, iterator);
        self.terminate(TerminatorKind::Goto(header), statement);
        self.start(end);
    }

    /// Combines the 8 `lanes` of the variable `local` in the fixed pairwise tree, then the
    /// variable's value from before the loop with the result: `local = local ⊕ tree`.
    fn combine_lanes(
        &mut self,
        local: Local,
        lanes: Local,
        operator: ReductionOperator,
        node: NodeId,
    ) {
        let int = self.int_type();
        let ty = self.local_type(local);
        let operator = match operator {
            ReductionOperator::Multiply => BinaryOperator::Multiply,
            _ => BinaryOperator::Add,
        };
        let mut level: Vec<Operand<'db>> = (0..i128::from(LANES))
            .map(|index| {
                let position = self.temporary(int, node);
                let constant = Operand::Constant(Constant::integer(int, index));
                self.assign(Place::from(position), Rvalue::Use(constant), node);
                let lane = Place::from(lanes).project(Projection::Index(vec![position]));
                Operand::Copy(lane)
            })
            .collect();
        while level.len() > 1 {
            level = level
                .chunks(2)
                .map(|pair| {
                    let combined = self.temporary(ty, node);
                    let rvalue = Rvalue::Binary(operator, pair[0].clone(), pair[1].clone());
                    self.assign(Place::from(combined), rvalue, node);
                    Operand::Copy(Place::from(combined))
                })
                .collect();
        }
        let before = Operand::Copy(Place::from(local));
        let tree = level.swap_remove(0);
        self.assign(
            Place::from(local),
            Rvalue::Binary(operator, before, tree),
            node,
        );
    }
}

/// The identity of `operator` at the float type `ty`: `-0.0` for a sum, since `-0.0 + x` is `x`
/// for every `x`, `+0.0` and `-0.0` included, and `1.0` for a product.
fn identity(ty: Type<'_>, operator: ReductionOperator) -> Constant<'_> {
    let value = match operator {
        ReductionOperator::Multiply => 1.0_f64,
        _ => -0.0_f64,
    };
    Constant {
        ty,
        value: ConstantValue::Float(value.to_bits()),
    }
}
