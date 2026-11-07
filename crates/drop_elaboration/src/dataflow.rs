//! Which move paths may hold a value, and which may hold nothing, at the start of each block.
//!
//! A forward dataflow analysis over the control-flow graph. Each path carries two bits: *maybe
//! initialized* (some path from the entry leaves it holding a value) and *maybe uninitialized*
//! (some path leaves it empty). Where blocks meet, the bits are joined with "or". A path with only
//! the first bit is certainly initialized, with only the second certainly empty, and with both it
//! needs a drop flag.

use crate::paths::{Effect, Paths, statement_effects, successors, terminator_effects};
use mir::{BasicBlock, BlockId, Place};

/// The two bits of every move path at one point in a body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct State {
    /// Whether each path may hold a value.
    pub(crate) maybe_init: Vec<bool>,
    /// Whether each path may hold nothing.
    pub(crate) maybe_uninit: Vec<bool>,
}

impl State {
    /// The state where no path has been reached: both bits clear.
    fn unreached(paths: &Paths) -> Self {
        Self {
            maybe_init: vec![false; paths.len()],
            maybe_uninit: vec![false; paths.len()],
        }
    }

    /// The state on entry: paths of the first `parameter_count` locals after the return local hold
    /// their arguments, and every other path holds nothing.
    pub(crate) fn entry(paths: &Paths, parameter_count: u32) -> Self {
        let mut state = Self::unreached(paths);
        for index in 0..paths.len() {
            let local = paths.place(index).local.index();
            let parameter = (1..=parameter_count as usize).contains(&local);
            state.maybe_init[index] = parameter;
            state.maybe_uninit[index] = !parameter;
        }
        state
    }

    /// Applies one effect to every path at or under its place.
    pub(crate) fn apply(&mut self, paths: &Paths, effect: &Effect) {
        let (place, init): (&Place, bool) = match effect {
            Effect::Init(place) => (place, true),
            Effect::Uninit(place) => (place, false),
        };
        for index in paths.under(place) {
            self.maybe_init[index] = init;
            self.maybe_uninit[index] = !init;
        }
    }

    /// Joins `other` into this state, returning whether anything changed.
    fn join(&mut self, other: &Self) -> bool {
        let mut changed = false;
        for (mine, theirs) in [
            (&mut self.maybe_init, &other.maybe_init),
            (&mut self.maybe_uninit, &other.maybe_uninit),
        ] {
            for (bit, &other_bit) in mine.iter_mut().zip(theirs) {
                if other_bit && !*bit {
                    *bit = true;
                    changed = true;
                }
            }
        }
        changed
    }
}

/// The state at the start of every block. A block no path reaches has both bits clear for every
/// path.
///
/// Cost: each block is revisited only when its entry state grows, and a state can grow at most
/// twice per path, so the work is bounded by blocks times paths.
pub(crate) fn block_entries(
    blocks: &[BasicBlock<'_>],
    paths: &Paths,
    parameter_count: u32,
) -> Vec<State> {
    let mut entries = vec![State::unreached(paths); blocks.len()];
    if blocks.is_empty() {
        return entries;
    }
    entries[BlockId::ENTRY.index()] = State::entry(paths, parameter_count);
    let mut pending = vec![BlockId::ENTRY];
    while let Some(block) = pending.pop() {
        let mut state = entries[block.index()].clone();
        let data = &blocks[block.index()];
        for statement in &data.statements {
            for effect in statement_effects(&statement.kind) {
                state.apply(paths, &effect);
            }
        }
        for effect in terminator_effects(&data.terminator.kind) {
            state.apply(paths, &effect);
        }
        for (target, effects) in successors(&data.terminator.kind) {
            let mut on_edge = state.clone();
            for effect in &effects {
                on_edge.apply(paths, effect);
            }
            if entries[target.index()].join(&on_edge) && !pending.contains(&target) {
                pending.push(target);
            }
        }
    }
    entries
}
