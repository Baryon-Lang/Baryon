//! Broadcasting (roadmap 3.9): the lengths of a whole-array expression's result, worked out from
//! its operands' lengths by `NumPy`'s rules, the checks that every operand fits them, and how each
//! operand is indexed from the loop's indices.
//!
//! `NumPy`'s rules, which the language reference's "Broadcasting" states for programs:
//!
//! 1. The operands' dimensions line up from the last: an operand of lower rank has the missing
//!    leading dimensions, of length 1.
//! 2. In each dimension, the result's length is the one length other than 1 that the operands
//!    have, or 1 when they all have 1.
//! 3. An operand of length 1 in a dimension repeats its one element along it.
//!
//! Each level of an expression broadcasts its own operands: the whole expression, and each
//! [`Fused`](mir::Elementwise::Fused) subexpression, whose operands are checked where its
//! statement was and whose result is then one operand of the level around it. An operand is an
//! array leaf or a fused subexpression; numbers apply to every element and take no part.
//!
//! The lengths of an `Array` are known only when the line runs, so whether an operand repeats in
//! a dimension is known only then, too. Indexing every operand by `index * repeat`, with
//! `repeat` 0 or 1, works for every case but hides from the optimizer that the common case
//! walks memory in order, which costs vectorized loops. So expansion writes the loop nest twice:
//! the plain nest, which indexes every operand directly, exactly as before broadcasting, and the
//! broadcasting nest, which multiplies. A flag computed once before the loops picks one. Only an
//! operand that shares a dimension with another one can repeat in it: an expression of one array
//! and numbers, the most common kind, gets the plain nest alone.

use super::Expander;
use mir::{BinaryOperator, Check, Constant, Elementwise, Local, Operand, Place, Rvalue};
use std::collections::HashMap;
use types::{Primitive, Type, TypeKind};

use crate::place_types::Shape;

/// How one dimension of an operand is indexed, from the loop index of the result's dimension it
/// lines up with.
#[derive(Clone, Copy, Debug)]
pub(super) enum Step {
    /// By the loop index itself: the operand has the result's length in this dimension.
    Same,
    /// At 0, in the `i64` local this holds: a `Fixed` operand of length 1 against a longer
    /// result, which is known when compiling.
    Zero(Local),
    /// By the loop index times the `i64` in this local, which is 1 when the operand has the
    /// result's length in this dimension and 0 when it has length 1 and repeats. Only the
    /// broadcasting nest multiplies; the plain nest runs only when every such local is 1, and
    /// uses the loop index.
    Scaled(Local),
}

/// What broadcasting worked out for one whole-array statement, read when the loop body is
/// written.
#[derive(Default)]
pub(super) struct Broadcast<'db> {
    /// The lengths of each fused subexpression's result, by the address of its node.
    fused: HashMap<usize, Vec<Operand<'db>>>,
    /// How each operand, an array leaf or a fused subexpression, is indexed: one step per
    /// dimension of the operand, by the address of its node. Nodes do not move while a
    /// statement is expanded, so an address names one node.
    steps: HashMap<usize, Vec<Step>>,
    /// The `bool` locals that say an operand has the result's length in a dimension where only
    /// the run decides it. When every one holds, nothing repeats and the plain nest runs.
    pub(super) same: Vec<Local>,
    /// The lengths read from each array place so far, so a place read twice, as in `a * a`, or
    /// the view a statement writes, is measured once.
    measured: HashMap<Place, Vec<Operand<'db>>>,
}

impl Broadcast<'_> {
    /// The steps of the operand at `node`, or `None` for a node that is not an operand.
    pub(super) fn steps(&self, node: &Elementwise<'_>) -> Option<&[Step]> {
        self.steps.get(&address(node)).map(Vec::as_slice)
    }
}

/// The address of `node`, which names it in a [`Broadcast`].
fn address(node: &Elementwise<'_>) -> usize {
    std::ptr::from_ref(node).addr()
}

/// One operand of a level: an array leaf or a fused subexpression.
struct Part<'db> {
    /// The address of its node.
    key: usize,
    /// For an array leaf, its place, so the view a statement writes in place can be told apart.
    place: Option<Place>,
    /// The length of each of its dimensions: a constant for a `Fixed` array, otherwise a local.
    lengths: Vec<Operand<'db>>,
}

impl<'db> Expander<'_, '_, 'db> {
    /// Works out the lengths of `tree`'s result, checks every operand fits them, and records how
    /// each operand is indexed, in `broadcast`. `into` is the lengths of the view a statement
    /// writes in place, `destination`: the result must have exactly those lengths, so every
    /// operand is checked against them, as `NumPy` checks an output. The view itself, which `+=`
    /// reads, has them.
    ///
    /// Checks are written at the expander's current span, operand by operand from the left,
    /// dimension by dimension, as before broadcasting; the message names the result's dimension.
    pub(super) fn broadcast(
        &mut self,
        tree: &Elementwise<'db>,
        into: Option<(&Place, Vec<Operand<'db>>)>,
        broadcast: &mut Broadcast<'db>,
    ) -> Vec<Operand<'db>> {
        if let Some((destination, lengths)) = &into {
            broadcast
                .measured
                .insert((*destination).clone(), lengths.clone());
        }
        let mut parts = Vec::new();
        self.parts(tree, broadcast, &mut parts);
        let (lengths, destination) = match into {
            Some((destination, lengths)) => (lengths, Some(destination)),
            None => (self.combined(&parts), None),
        };
        let rank = lengths.len();
        // The first operand to have a dimension sets it unless it has length 1, so it fits.
        let mut first = vec![true; rank];
        let mut shared = vec![0usize; rank];
        // An operand of higher rank than the result does not type-check.
        parts.retain(|part| part.lengths.len() <= rank);
        for part in &parts {
            for count in &mut shared[rank - part.lengths.len()..] {
                *count += 1;
            }
        }
        for part in &parts {
            let offset = rank - part.lengths.len();
            let is_destination = part.place.is_some() && part.place.as_ref() == destination;
            for (position, length) in part.lengths.iter().enumerate() {
                let dimension = offset + position;
                let fits = destination.is_none() && std::mem::take(&mut first[dimension]);
                if !fits && !is_destination {
                    self.check_fits(dimension, length, &lengths[dimension]);
                }
            }
        }
        for part in parts {
            let offset = rank - part.lengths.len();
            let is_destination = part.place.is_some() && part.place.as_ref() == destination;
            let steps = (offset..)
                .zip(&part.lengths)
                .map(|(dimension, length)| {
                    let alone = destination.is_none() && shared[dimension] == 1;
                    if alone || is_destination {
                        Step::Same
                    } else {
                        self.step(length, &lengths[dimension], broadcast)
                    }
                })
                .collect();
            broadcast.steps.insert(part.key, steps);
        }
        lengths
    }

    /// The operands of `tree`'s own level, from the left, appended to `parts`. A fused
    /// subexpression's lengths are worked out first, at its statement's span, if they are not
    /// yet.
    fn parts(
        &mut self,
        tree: &Elementwise<'db>,
        broadcast: &mut Broadcast<'db>,
        parts: &mut Vec<Part<'db>>,
    ) {
        match tree {
            Elementwise::Leaf(Operand::Copy(place) | Operand::Move(place)) => {
                let Some(ty) = self.place_type(place) else {
                    return;
                };
                let Some((_, shape)) = self.types.array(ty) else {
                    return;
                };
                let lengths = match shape {
                    _ if broadcast.measured.contains_key(place) => {
                        broadcast.measured[place].clone()
                    }
                    Shape::Fixed(lengths) => lengths
                        .iter()
                        .map(|&length| self.constant(i128::from(length)))
                        .collect(),
                    Shape::Runtime(rank) => (0..rank)
                        .map(|dimension| {
                            let length = self.local(self.i64);
                            self.assign(
                                Place::from(length),
                                Rvalue::Length(place.clone(), dimension),
                            );
                            Operand::Copy(Place::from(length))
                        })
                        .collect(),
                };
                broadcast.measured.insert(place.clone(), lengths.clone());
                parts.push(Part {
                    key: address(tree),
                    place: Some(place.clone()),
                    lengths,
                });
            }
            Elementwise::Leaf(Operand::Constant(_)) => {}
            Elementwise::Unary(_, operand) => self.parts(operand, broadcast, parts),
            Elementwise::Binary(_, left, right) => {
                self.parts(left, broadcast, parts);
                self.parts(right, broadcast, parts);
            }
            Elementwise::Fused(subexpression, span) => {
                let lengths = self.fused_lengths(subexpression, *span, broadcast);
                parts.push(Part {
                    key: address(tree),
                    place: None,
                    lengths,
                });
            }
        }
    }

    /// The lengths of the fused subexpression `subexpression`, broadcast and checked at `span`,
    /// its statement's, the first time they are asked for.
    pub(super) fn fused_lengths(
        &mut self,
        subexpression: &Elementwise<'db>,
        span: source::Span,
        broadcast: &mut Broadcast<'db>,
    ) -> Vec<Operand<'db>> {
        if let Some(lengths) = broadcast.fused.get(&address(subexpression)) {
            return lengths.clone();
        }
        let outer = std::mem::replace(&mut self.span, span);
        let lengths = self.broadcast(subexpression, None, broadcast);
        self.span = outer;
        broadcast
            .fused
            .insert(address(subexpression), lengths.clone());
        lengths
    }

    /// The result's lengths: in each dimension, the first operand's length there, replaced by
    /// the next operand's while it is 1. Constants combine when compiling.
    fn combined(&mut self, parts: &[Part<'db>]) -> Vec<Operand<'db>> {
        let rank = parts
            .iter()
            .map(|part| part.lengths.len())
            .max()
            .unwrap_or(0);
        (0..rank)
            .map(|dimension| {
                let mut result: Option<Operand<'db>> = None;
                for part in parts {
                    let Some(position) = (dimension + part.lengths.len()).checked_sub(rank) else {
                        continue;
                    };
                    let length = part.lengths[position].clone();
                    result = Some(match result {
                        None => length,
                        Some(sofar) => self.combine(sofar, length),
                    });
                }
                result.unwrap_or_else(|| self.constant(1))
            })
            .collect()
    }

    /// `length if sofar == 1 else sofar`, the result's length so far in one dimension after one
    /// more operand: without a branch, `sofar + (length - sofar) * i64(sofar == 1)`.
    fn combine(&mut self, sofar: Operand<'db>, length: Operand<'db>) -> Operand<'db> {
        if let Some(known) = integer(&sofar) {
            return if known == 1 { length } else { sofar };
        }
        let one = self.constant(1);
        let is_one = self.compare(BinaryOperator::Equal, sofar.clone(), one);
        let repeat = self.local(self.i64);
        self.assign(
            Place::from(repeat),
            Rvalue::Convert(Operand::Copy(Place::from(is_one)), self.i64),
        );
        let difference = self.binary(BinaryOperator::Subtract, length, sofar.clone());
        let change = self.binary(
            BinaryOperator::Multiply,
            difference,
            Operand::Copy(Place::from(repeat)),
        );
        self.binary(BinaryOperator::Add, sofar, change)
    }

    /// Checks that an operand's `length` in `dimension` fits the result's `expected` there: it
    /// is the same, or 1. Two constants were matched by the checker and need no check.
    fn check_fits(&mut self, dimension: usize, length: &Operand<'db>, expected: &Operand<'db>) {
        if integer(length).is_some() && integer(expected).is_some() {
            return;
        }
        let same = self.compare(BinaryOperator::Equal, length.clone(), expected.clone());
        let one = self.constant(1);
        let repeats = self.compare(BinaryOperator::Equal, length.clone(), one);
        let fits = self.local(self.bool());
        self.assign(
            Place::from(fits),
            Rvalue::Binary(
                BinaryOperator::BitOr,
                Operand::Copy(Place::from(same)),
                Operand::Copy(Place::from(repeats)),
            ),
        );
        let next = self.new_block();
        self.finish(
            mir::TerminatorKind::Assert {
                condition: Operand::Copy(Place::from(fits)),
                check: Check::SameLength {
                    dimension: u32::try_from(dimension).unwrap_or(u32::MAX),
                    expected: expected.clone(),
                    found: length.clone(),
                },
                target: next,
            },
            next,
        );
    }

    /// How an operand of `length` in one dimension is indexed against the result's `expected`.
    fn step(
        &mut self,
        length: &Operand<'db>,
        expected: &Operand<'db>,
        broadcast: &mut Broadcast<'db>,
    ) -> Step {
        if let (Some(length), Some(expected)) = (integer(length), integer(expected)) {
            if length == expected {
                return Step::Same;
            }
            let zero = self.local(self.i64);
            let value = self.constant(0);
            self.assign(Place::from(zero), Rvalue::Use(value));
            return Step::Zero(zero);
        }
        let same = self.compare(BinaryOperator::Equal, length.clone(), expected.clone());
        broadcast.same.push(same);
        let scale = self.local(self.i64);
        self.assign(
            Place::from(scale),
            Rvalue::Convert(Operand::Copy(Place::from(same)), self.i64),
        );
        Step::Scaled(scale)
    }

    /// A new `bool` local holding `left operator right`.
    fn compare(
        &mut self,
        operator: BinaryOperator,
        left: Operand<'db>,
        right: Operand<'db>,
    ) -> Local {
        let result = self.local(self.bool());
        self.assign(Place::from(result), Rvalue::Binary(operator, left, right));
        result
    }

    /// A new `i64` local holding `left operator right`, unchecked: lengths are far from
    /// overflowing.
    fn binary(
        &mut self,
        operator: BinaryOperator,
        left: Operand<'db>,
        right: Operand<'db>,
    ) -> Operand<'db> {
        let result = self.local(self.i64);
        self.assign(Place::from(result), Rvalue::Binary(operator, left, right));
        Operand::Copy(Place::from(result))
    }

    /// The `i64` constant `value`.
    pub(super) fn constant(&self, value: i128) -> Operand<'db> {
        Operand::Constant(Constant::integer(self.i64, value))
    }

    /// The `bool` type.
    pub(super) fn bool(&self) -> Type<'db> {
        Type::new(self.db, TypeKind::Primitive(Primitive::Bool))
    }
}

/// The value of `operand` when it is an integer constant.
fn integer(operand: &Operand<'_>) -> Option<i128> {
    match operand {
        Operand::Constant(Constant {
            value: mir::ConstantValue::Integer(value),
            ..
        }) => Some(*value),
        _ => None,
    }
}
