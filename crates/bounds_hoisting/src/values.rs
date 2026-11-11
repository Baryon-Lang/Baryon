//! Works out which index checks of a counted loop hold on every pass when a few comparisons
//! hold before it starts: those whose index is the counter, a value the loop does not change,
//! or a constant, plus a constant, and whose length the loop does not change.

use crate::loops::{CountedLoop, statement_writes, terminator_writes, writes};
use mir::{
    BinaryOperator, Body, Check, ConstantValue, Local, Operand, Place, Projection, Rvalue,
    StatementKind, TerminatorKind, UnaryOperator,
};

/// The largest constant an index may add to its base, at any step of computing it. Bounding it
/// keeps every step far from `i64`'s limits once the checks before the loop pass, since an
/// array's length is far below them.
const LARGEST_OFFSET: i128 = 1 << 32;

/// How deep [`Analysis::value`] follows the assignments an index is computed through.
const DEEPEST: u32 = 16;

/// What an index adds its constant offset to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Base {
    /// The loop's counter, which runs from its value on entry to the stop, less 1.
    Counter,
    /// A local the loop does not write, which holds on every pass its value on entry.
    Invariant(Local),
    /// Nothing: the index is the offset itself.
    Constant,
}

/// An index: its base plus a constant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Affine {
    /// The base.
    pub(crate) base: Base,
    /// The constant added to it.
    pub(crate) offset: i128,
}

/// Where a checked length comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Length {
    /// A constant, a `Fixed` array's.
    Constant(i128),
    /// A local the loop does not write.
    Invariant(Local),
    /// A dimension of an array the loop does not replace, which can be read again before it.
    Read(Place, u32),
}

/// One index check of the loop that can be made before it instead.
pub(crate) struct Hoisted {
    /// The block whose terminator is the check.
    pub(crate) block: usize,
    /// The index.
    pub(crate) index: Affine,
    /// The length it must be below.
    pub(crate) length: Length,
    /// The `CheckedBinary` locals the index is computed through. Their overflow checks cannot
    /// fail when the checks before the loop pass, since each step lies near the index.
    pub(crate) computed: Vec<Local>,
}

/// Every index check of `counted` that can be made before the loop.
pub(crate) fn hoistable(body: &Body<'_>, counted: &CountedLoop<'_>) -> Vec<Hoisted> {
    let dominators = dominators(body);
    let analysis = Analysis {
        body,
        counted,
        dominators: &dominators,
    };
    counted
        .blocks
        .iter()
        .filter_map(|&block| analysis.check(block))
        .collect()
}

/// What the analysis of one loop reads.
struct Analysis<'a, 'db> {
    /// The body.
    body: &'a Body<'db>,
    /// The loop.
    counted: &'a CountedLoop<'db>,
    /// For each block, whether each block dominates it, as [`dominators`] gives them.
    dominators: &'a [Vec<bool>],
}

/// A position in a body: a block, and the number of its statements that run before it.
type Position = (usize, usize);

impl<'db> Analysis<'_, 'db> {
    /// The index check that ends block `block`, as lowering writes it, when it can be hoisted:
    ///
    /// ```text
    /// above = GreaterEqual(copy x, 0)
    /// below = Less(copy x, length)
    /// inside = BitAnd(copy above, copy below)
    /// assert(copy inside, index in bounds)
    /// ```
    fn check(&self, block: usize) -> Option<Hoisted> {
        let data = &self.body.blocks[block];
        let TerminatorKind::Assert {
            condition: Operand::Copy(condition),
            check:
                Check::IndexInBounds {
                    index: Operand::Copy(index),
                    length,
                    ..
                },
            ..
        } = &data.terminator.kind
        else {
            return None;
        };
        let ty = self.body.locals.get(index.local.index())?.ty;
        if !index.projections.is_empty() || ty != self.body.locals[self.counted.counter.index()].ty
        {
            return None;
        }
        let end = (block, data.statements.len());
        let (inside, at_inside) = self.last_assignment(condition, end)?;
        let Rvalue::Binary(BinaryOperator::BitAnd, Operand::Copy(above), Operand::Copy(below)) =
            inside
        else {
            return None;
        };
        let (above, at_above) = self.last_assignment(above, at_inside)?;
        let (below, at_below) = self.last_assignment(below, at_inside)?;
        let zero = Operand::Constant(mir::Constant::integer(ty, 0));
        let read = Operand::Copy(index.clone());
        if *above != Rvalue::Binary(BinaryOperator::GreaterEqual, read.clone(), zero)
            || *below != Rvalue::Binary(BinaryOperator::Less, read, length.clone())
        {
            return None;
        }
        let mut computed = Vec::new();
        let value = self.value(index, at_above, &mut computed, 0)?;
        if self.value(index, at_below, &mut computed, 0)? != value
            || (value.base == Base::Constant && value.offset < 0)
        {
            return None;
        }
        Some(Hoisted {
            block,
            index: value,
            length: self.length(length, at_below)?,
            computed,
        })
    }

    /// The rvalue the last statement before `(block, position)` that writes `place` assigns
    /// it, when that statement is in the same block and assigns all of `place`.
    fn last_assignment(
        &self,
        place: &Place,
        (block, position): Position,
    ) -> Option<(&'_ Rvalue<'db>, Position)> {
        let statements = &self.body.blocks[block].statements[..position];
        let (index, statement) = statements
            .iter()
            .enumerate()
            .rev()
            .find(|(_, statement)| statement_writes(&statement.kind, place.local))?;
        match &statement.kind {
            StatementKind::Assign(assigned, rvalue)
                if assigned == place && place.projections.is_empty() =>
            {
                Some((rvalue, (block, index)))
            }
            _ => None,
        }
    }

    /// Where the length `operand`, read at `at`, comes from.
    fn length(&self, operand: &Operand<'db>, at: Position) -> Option<Length> {
        match operand {
            Operand::Constant(constant) => match constant.value {
                ConstantValue::Integer(length) => Some(Length::Constant(length)),
                _ => None,
            },
            Operand::Copy(place) if place.projections.is_empty() => {
                if !writes(self.body, &self.counted.blocks, place.local) {
                    return Some(Length::Invariant(place.local));
                }
                match self.definition(place.local, at)?.0 {
                    Rvalue::Length(array, dimension)
                        if !writes(self.body, &self.counted.blocks, array.local)
                            && array
                                .projections
                                .iter()
                                .all(|step| matches!(step, Projection::Field(_))) =>
                    {
                        Some(Length::Read(array.clone(), *dimension))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// The value of `place`, read at `at`, as a base plus a constant. `computed` collects the
    /// `CheckedBinary` locals it is computed through; `depth` counts the steps followed.
    fn value(
        &self,
        place: &Place,
        at: Position,
        computed: &mut Vec<Local>,
        depth: u32,
    ) -> Option<Affine> {
        let local = place.local;
        if place.projections.is_empty() {
            if local == self.counted.counter {
                return Some(Affine {
                    base: Base::Counter,
                    offset: 0,
                });
            }
            if !writes(self.body, &self.counted.blocks, local) {
                return Some(Affine {
                    base: Base::Invariant(local),
                    offset: 0,
                });
            }
        }
        if depth > DEEPEST {
            return None;
        }
        let (definition, defined) = self.definition(local, at)?;
        let (operator, left, right) = match (place.projections.as_slice(), definition) {
            ([], Rvalue::Use(operand)) => return self.operand(operand, defined, computed, depth),
            ([], Rvalue::Binary(operator, left, right)) => (operator, left, right),
            ([Projection::Field(0)], Rvalue::CheckedBinary(operator, left, right)) => {
                computed.push(local);
                (operator, left, right)
            }
            _ => return None,
        };
        let left = self.operand(left, defined, computed, depth)?;
        let right = self.operand(right, defined, computed, depth)?;
        combine(*operator, left, right)
    }

    /// The value of `operand`, read at `at`, as [`value`](Self::value) gives it.
    fn operand(
        &self,
        operand: &Operand<'db>,
        at: Position,
        computed: &mut Vec<Local>,
        depth: u32,
    ) -> Option<Affine> {
        match operand {
            Operand::Copy(place) => self.value(place, at, computed, depth + 1),
            Operand::Constant(constant) => match constant.value {
                ConstantValue::Integer(offset) if offset.abs() <= LARGEST_OFFSET => Some(Affine {
                    base: Base::Constant,
                    offset,
                }),
                _ => None,
            },
            Operand::Move(_) => None,
        }
    }

    /// The rvalue of the one statement in the loop that writes `local`, and where it is, when
    /// it assigns all of `local` and runs before `at` on every path to `at`: earlier in the
    /// same block, or in a block that dominates `at`'s. Each pass then reads at `at` the value
    /// that statement computed on the same pass.
    fn definition(&self, local: Local, at: Position) -> Option<(&'_ Rvalue<'db>, Position)> {
        let mut found = None;
        for &block in &self.counted.blocks {
            let data = &self.body.blocks[block];
            if terminator_writes(&data.terminator.kind, local) {
                return None;
            }
            for (index, statement) in data.statements.iter().enumerate() {
                if !statement_writes(&statement.kind, local) {
                    continue;
                }
                match &statement.kind {
                    StatementKind::Assign(place, rvalue)
                        if found.is_none() && *place == Place::from(local) =>
                    {
                        found = Some((rvalue, (block, index)));
                    }
                    _ => return None,
                }
            }
        }
        let (rvalue, defined) = found?;
        let runs_first = if defined.0 == at.0 {
            defined.1 < at.1
        } else {
            self.dominators[at.0][defined.0]
        };
        runs_first.then_some((rvalue, defined))
    }
}

/// `left operator right`, when it is a base plus a constant.
fn combine(operator: BinaryOperator, left: Affine, right: Affine) -> Option<Affine> {
    let (base, offset) = match (operator, left.base, right.base) {
        (BinaryOperator::Add, base, Base::Constant)
        | (BinaryOperator::Add, Base::Constant, base) => (base, left.offset + right.offset),
        (BinaryOperator::Subtract, base, Base::Constant) => (base, left.offset - right.offset),
        _ => return None,
    };
    (offset.abs() <= LARGEST_OFFSET).then_some(Affine { base, offset })
}

/// For each block of `body`, whether each block dominates it: lies on every path to it from
/// the entry. Every block dominates a block no path reaches.
///
/// Cost: a few passes over the blocks, each `blocks²` bits; bodies have hundreds of blocks at
/// most.
fn dominators(body: &Body<'_>) -> Vec<Vec<bool>> {
    let count = body.blocks.len();
    let predecessors = crate::loops::predecessors(body);
    let mut dominators = vec![vec![true; count]; count];
    if let Some(entry) = dominators.first_mut() {
        *entry = (0..count).map(|block| block == 0).collect();
    }
    let mut changed = true;
    while changed {
        changed = false;
        for block in 1..count {
            let mut next = vec![true; count];
            for &predecessor in &predecessors[block] {
                for (kept, &dominates) in next.iter_mut().zip(&dominators[predecessor]) {
                    *kept &= dominates;
                }
            }
            next[block] = true;
            if next != dominators[block] {
                dominators[block] = next;
                changed = true;
            }
        }
    }
    dominators
}

/// The local whose overflow flag `rvalue` negates, when it is `Not(copy t.1)`: the condition
/// of the overflow check after `t = CheckedBinary(...)`.
pub(crate) fn overflow_flag(rvalue: &Rvalue<'_>) -> Option<Local> {
    match rvalue {
        Rvalue::Unary(UnaryOperator::Not, Operand::Copy(place))
            if place.projections == [Projection::Field(1)] =>
        {
            Some(place.local)
        }
        _ => None,
    }
}
