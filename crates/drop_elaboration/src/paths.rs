//! Move paths, and what each statement and terminator does to them.
//!
//! A *move path* is a place reached only through fields, such as `_1` or `_1.0`: the parts of a
//! value that can be moved out and dropped separately. Only the paths a body moves or drops are
//! tracked; every other part of a value lives and dies with its nearest tracked ancestor.

use mir::{
    Argument, BlockId, Callee, Elementwise, Operand, Place, Projection, Rvalue, StatementKind,
    TerminatorKind,
};

/// What one step does to the move paths under a place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Effect {
    /// The place, and every part of it, now holds a value.
    Init(Place),
    /// The place, and every part of it, holds nothing: it was moved out or dropped.
    Uninit(Place),
}

/// The move paths of one body, each identified by its position.
pub(crate) struct Paths {
    /// Every tracked path, each a local followed only by field projections.
    places: Vec<Place>,
}

impl Paths {
    /// The paths of `blocks`: every place dropped, every place moved out, and the ancestors of
    /// each moved place, so a whole value and its moved part are related.
    pub(crate) fn collect(blocks: &[mir::BasicBlock<'_>]) -> Self {
        let mut paths = Self { places: Vec::new() };
        for block in blocks {
            for statement in &block.statements {
                if let StatementKind::Drop(place) = &statement.kind
                    && is_field_path(place)
                {
                    paths.add(place.clone());
                }
                for effect in statement_effects(&statement.kind) {
                    if let Effect::Uninit(place) = effect {
                        paths.add_with_ancestors(&place);
                    }
                }
            }
            for effect in terminator_effects(&block.terminator.kind) {
                if let Effect::Uninit(place) = effect {
                    paths.add_with_ancestors(&place);
                }
            }
        }
        paths
    }

    /// How many paths there are.
    pub(crate) fn len(&self) -> usize {
        self.places.len()
    }

    /// Whether the body moves and drops nothing, so there is nothing to elaborate.
    pub(crate) fn is_empty(&self) -> bool {
        self.places.is_empty()
    }

    /// The path at `index`.
    pub(crate) fn place(&self, index: usize) -> &Place {
        &self.places[index]
    }

    /// The position of `place`, if it is tracked.
    pub(crate) fn index(&self, place: &Place) -> Option<usize> {
        self.places.iter().position(|tracked| tracked == place)
    }

    /// The paths at or under `place`: those it is a prefix of.
    pub(crate) fn under<'a>(&'a self, place: &'a Place) -> impl Iterator<Item = usize> + 'a {
        (0..self.places.len()).filter(move |&index| is_prefix(place, &self.places[index]))
    }

    /// The paths strictly under `place`.
    pub(crate) fn strictly_under<'a>(
        &'a self,
        place: &'a Place,
    ) -> impl Iterator<Item = usize> + 'a {
        self.under(place)
            .filter(move |&index| self.places[index].projections.len() > place.projections.len())
    }

    /// Adds `place` unless it is already tracked.
    fn add(&mut self, place: Place) {
        if self.index(&place).is_none() {
            self.places.push(place);
        }
    }

    /// Adds `place` and each shorter field path it extends, down to its local.
    fn add_with_ancestors(&mut self, place: &Place) {
        for length in 0..=place.projections.len() {
            self.add(Place {
                local: place.local,
                projections: place.projections[..length].to_vec(),
            });
        }
    }
}

/// Whether `place` is a move path: a local followed only by fields.
pub(crate) fn is_field_path(place: &Place) -> bool {
    place
        .projections
        .iter()
        .all(|projection| matches!(projection, Projection::Field(_)))
}

/// Whether `prefix` names `place` or a value that contains it.
fn is_prefix(prefix: &Place, place: &Place) -> bool {
    prefix.local == place.local && place.projections.starts_with(&prefix.projections)
}

/// What a statement does to move paths, in order.
pub(crate) fn statement_effects(statement: &StatementKind<'_>) -> Vec<Effect> {
    let mut effects = Vec::new();
    match statement {
        StatementKind::Assign(place, rvalue) => {
            rvalue_moves(rvalue, &mut effects);
            if is_field_path(place) {
                effects.push(Effect::Init(place.clone()));
            }
        }
        StatementKind::Drop(place) => {
            if is_field_path(place) {
                effects.push(Effect::Uninit(place.clone()));
            }
        }
        StatementKind::Accumulate { value, .. } => operand_moves(value, &mut effects),
    }
    effects
}

/// What a terminator does to move paths before control leaves the block, on every edge: the
/// values its operands move out.
pub(crate) fn terminator_effects(terminator: &TerminatorKind<'_>) -> Vec<Effect> {
    let mut effects = Vec::new();
    match terminator {
        TerminatorKind::Goto(_) | TerminatorKind::Return | TerminatorKind::Unreachable => {}
        TerminatorKind::Branch { condition, .. } => operand_moves(condition, &mut effects),
        TerminatorKind::Switch { value, .. } => operand_moves(value, &mut effects),
        TerminatorKind::Raise(error) => operand_moves(error, &mut effects),
        TerminatorKind::Call {
            callee, arguments, ..
        } => {
            if let Callee::Value(function) = callee {
                operand_moves(function, &mut effects);
            }
            argument_moves(arguments, &mut effects);
        }
        TerminatorKind::ParallelFor {
            count, arguments, ..
        } => {
            operand_moves(count, &mut effects);
            argument_moves(arguments, &mut effects);
        }
        TerminatorKind::Assert {
            condition, check, ..
        } => {
            operand_moves(condition, &mut effects);
            for operand in check.operands() {
                operand_moves(operand, &mut effects);
            }
        }
    }
    effects
}

/// Where a terminator's edges go, each with what taking that edge does to move paths: a call
/// fills its destination on success and its error local on failure.
pub(crate) fn successors(terminator: &TerminatorKind<'_>) -> Vec<(BlockId, Vec<Effect>)> {
    match terminator {
        TerminatorKind::Goto(target)
        | TerminatorKind::Assert { target, .. }
        | TerminatorKind::ParallelFor { target, .. } => vec![(*target, Vec::new())],
        TerminatorKind::Branch {
            then, otherwise, ..
        } => vec![(*then, Vec::new()), (*otherwise, Vec::new())],
        TerminatorKind::Switch {
            cases, otherwise, ..
        } => cases
            .iter()
            .map(|&(_, target)| (target, Vec::new()))
            .chain([(*otherwise, Vec::new())])
            .collect(),
        TerminatorKind::Return | TerminatorKind::Raise(_) | TerminatorKind::Unreachable => {
            Vec::new()
        }
        TerminatorKind::Call {
            destination,
            target,
            error,
            ..
        } => {
            let filled = if is_field_path(destination) {
                vec![Effect::Init(destination.clone())]
            } else {
                Vec::new()
            };
            let mut edges = vec![(*target, filled)];
            if let Some(edge) = error {
                edges.push((edge.target, vec![Effect::Init(Place::from(edge.error))]));
            }
            edges
        }
    }
}

/// The moves call arguments make: each `Owned` one moves its operand, in order.
fn argument_moves(arguments: &[Argument<'_>], effects: &mut Vec<Effect>) {
    for argument in arguments {
        if let Argument::Owned(operand) = argument {
            operand_moves(operand, effects);
        }
    }
}

/// The moves an rvalue makes, in evaluation order.
fn rvalue_moves(rvalue: &Rvalue<'_>, effects: &mut Vec<Effect>) {
    match rvalue {
        Rvalue::Use(operand) | Rvalue::Unary(_, operand) | Rvalue::Convert(operand, _) => {
            operand_moves(operand, effects);
        }
        Rvalue::Binary(_, left, right) | Rvalue::CheckedBinary(_, left, right) => {
            operand_moves(left, effects);
            operand_moves(right, effects);
        }
        Rvalue::Aggregate(_, operands) | Rvalue::NewArray(_, operands) => {
            for operand in operands {
                operand_moves(operand, effects);
            }
        }
        Rvalue::Length(..)
        | Rvalue::IsPresent(_)
        | Rvalue::ErrorIndex(_)
        | Rvalue::MemberIndex(_)
        | Rvalue::Format(_) => {}
        Rvalue::Elementwise(tree) => elementwise_moves(tree, effects),
    }
}

/// The moves the leaves of a whole-array expression make.
fn elementwise_moves(tree: &Elementwise<'_>, effects: &mut Vec<Effect>) {
    match tree {
        Elementwise::Leaf(operand) => operand_moves(operand, effects),
        Elementwise::Unary(_, operand) => elementwise_moves(operand, effects),
        Elementwise::Binary(_, left, right) => {
            elementwise_moves(left, effects);
            elementwise_moves(right, effects);
        }
    }
}

/// The move an operand makes, if it moves.
///
/// Moving out of an optional's value or an error value consumes the whole optional or error
/// value, so the move path stops before that step. Array elements are copy types and are never
/// moved out (the reference, "Moves"), so a move through an index moves nothing, and a view is
/// a borrow, never moved (the `Slice` step).
fn operand_moves(operand: &Operand<'_>, effects: &mut Vec<Effect>) {
    let Operand::Move(place) = operand else {
        return;
    };
    let mut path = Place::from(place.local);
    for projection in &place.projections {
        match projection {
            Projection::Field(_) => path.projections.push(projection.clone()),
            Projection::Value | Projection::Error(_) | Projection::Member(_) => break,
            Projection::Index(_) | Projection::Slice(_) => return,
        }
    }
    effects.push(Effect::Uninit(path));
}
