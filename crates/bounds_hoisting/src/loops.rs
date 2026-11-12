//! Finds the counted loops of a body: the loops lowering writes for `for i in range(start, stop)`.

use mir::{
    Argument, BinaryOperator, BlockId, Body, ConstantValue, Local, Operand, Place, Projection,
    Rvalue, StatementKind, TerminatorKind,
};

/// A loop over `counter` from its value on entry up to, not including, `stop`, by 1:
///
/// ```text
/// header: test = Less(copy counter, stop); branch test -> [true: body, false: exit]
/// ...the body's blocks...
/// latch:  counter = Add(copy counter, 1); goto header
/// ```
///
/// The counter is written only by the latch, so inside the body it holds a value from its value
/// on entry to `stop - 1`, and `stop` is not written in the loop, so it is the same every pass.
pub(crate) struct CountedLoop<'db> {
    /// The block that tests the counter.
    pub(crate) header: usize,
    /// Every block of the loop, the header and the latch included, in order.
    pub(crate) blocks: Vec<usize>,
    /// The counter.
    pub(crate) counter: Local,
    /// Where the counter stops, a constant or a local the loop does not write.
    pub(crate) stop: Operand<'db>,
    /// The blocks outside the loop that jump to the header.
    pub(crate) entries: Vec<usize>,
}

/// Every counted loop of `body` whose header is not in `done`.
pub(crate) fn counted_loops<'db>(body: &Body<'db>, done: &[usize]) -> Vec<CountedLoop<'db>> {
    let predecessors = predecessors(body);
    (0..body.blocks.len())
        .filter(|header| !done.contains(header))
        .filter_map(|header| counted_loop(body, header, &predecessors))
        .collect()
}

/// The counted loop whose header is block `header`, if it is one.
fn counted_loop<'db>(
    body: &Body<'db>,
    header: usize,
    predecessors: &[Vec<usize>],
) -> Option<CountedLoop<'db>> {
    let block = &body.blocks[header];
    let [statement] = block.statements.as_slice() else {
        return None;
    };
    let StatementKind::Assign(
        test,
        Rvalue::Binary(BinaryOperator::Less, Operand::Copy(counter), stop),
    ) = &statement.kind
    else {
        return None;
    };
    let TerminatorKind::Branch {
        condition: Operand::Copy(condition),
        ..
    } = &block.terminator.kind
    else {
        return None;
    };
    if condition != test || !counter.projections.is_empty() {
        return None;
    }
    let counter = counter.local;
    let latches: Vec<usize> = predecessors[header]
        .iter()
        .copied()
        .filter(|&latch| is_latch(body, latch, header, counter))
        .collect();
    let [latch] = latches.as_slice() else {
        return None;
    };
    let blocks = natural_loop(header, *latch, predecessors);
    let entries: Vec<usize> = predecessors[header]
        .iter()
        .copied()
        .filter(|block| !blocks.contains(block))
        .collect();
    let stop_written = match stop {
        Operand::Copy(place) => !place.projections.is_empty() || writes(body, &blocks, place.local),
        Operand::Move(_) => true,
        Operand::Constant(_) => false,
    };
    let counter_written = blocks
        .iter()
        .filter(|&&block| block != *latch)
        .any(|&block| writes_in(body, block, counter));
    (!stop_written && !counter_written && !entries.is_empty()).then(|| CountedLoop {
        header,
        blocks,
        counter,
        stop: stop.clone(),
        entries,
    })
}

/// Whether block `latch` is `counter = Add(copy counter, 1); goto header` and nothing else.
fn is_latch(body: &Body<'_>, latch: usize, header: usize, counter: Local) -> bool {
    let block = &body.blocks[latch];
    let [statement] = block.statements.as_slice() else {
        return false;
    };
    let one = |operand: &Operand<'_>| matches!(operand, Operand::Constant(constant) if constant.value == ConstantValue::Integer(1));
    matches!(
        &statement.kind,
        StatementKind::Assign(place, Rvalue::Binary(BinaryOperator::Add, Operand::Copy(read), step))
            if *place == Place::from(counter) && *read == Place::from(counter) && one(step)
    ) && block.terminator.kind == TerminatorKind::Goto(BlockId::new(header))
}

/// The blocks of the loop that `latch` closes by jumping back to `header`: the header and every
/// block that reaches the latch without passing through the header.
fn natural_loop(header: usize, latch: usize, predecessors: &[Vec<usize>]) -> Vec<usize> {
    let mut blocks = vec![header];
    let mut pending = vec![latch];
    while let Some(block) = pending.pop() {
        if !blocks.contains(&block) {
            blocks.push(block);
            pending.extend(predecessors[block].iter().copied());
        }
    }
    blocks.sort_unstable();
    blocks
}

/// The blocks that jump to each block of `body`.
pub(crate) fn predecessors(body: &Body<'_>) -> Vec<Vec<usize>> {
    let mut predecessors = vec![Vec::new(); body.blocks.len()];
    for (block, data) in body.blocks.iter().enumerate() {
        for target in successors(&data.terminator.kind) {
            if let Some(list) = predecessors.get_mut(target.index()) {
                list.push(block);
            }
        }
    }
    predecessors
}

/// The blocks `terminator` may jump to.
pub(crate) fn successors(terminator: &TerminatorKind<'_>) -> Vec<BlockId> {
    let mut targets = Vec::new();
    for_each_target(&mut terminator.clone(), &mut |target| targets.push(*target));
    targets
}

/// Calls `visit` on each block `terminator` may jump to, so it can redirect it.
pub(crate) fn for_each_target(
    terminator: &mut TerminatorKind<'_>,
    visit: &mut impl FnMut(&mut BlockId),
) {
    match terminator {
        TerminatorKind::Goto(target)
        | TerminatorKind::Assert { target, .. }
        | TerminatorKind::ParallelFor { target, .. } => visit(target),
        TerminatorKind::Branch {
            then, otherwise, ..
        } => {
            visit(then);
            visit(otherwise);
        }
        TerminatorKind::Switch {
            cases, otherwise, ..
        } => {
            for (_, target) in cases {
                visit(target);
            }
            visit(otherwise);
        }
        TerminatorKind::Call { target, error, .. } => {
            visit(target);
            if let Some(edge) = error {
                visit(&mut edge.target);
            }
        }
        TerminatorKind::Return | TerminatorKind::Raise(_) | TerminatorKind::Unreachable => {}
    }
}

/// Whether any of `blocks` may change `local` as a whole: assign or drop it, a field of it,
/// or all of it, move it, or pass it where a callee may replace it. Writing one element of an
/// array, through an index or a view, does not count: it keeps the array's shape.
pub(crate) fn writes(body: &Body<'_>, blocks: &[usize], local: Local) -> bool {
    blocks.iter().any(|&block| writes_in(body, block, local))
}

/// Whether block `block` may change `local` as a whole, as [`writes`] describes.
pub(crate) fn writes_in(body: &Body<'_>, block: usize, local: Local) -> bool {
    let data = &body.blocks[block];
    data.statements
        .iter()
        .any(|statement| statement_writes(&statement.kind, local))
        || terminator_writes(&data.terminator.kind, local)
}

/// Whether `statement` may change `local` as a whole, as [`writes`] describes.
pub(crate) fn statement_writes(statement: &StatementKind<'_>, local: Local) -> bool {
    match statement {
        StatementKind::Assign(place, rvalue) => whole(place, local) || moves(rvalue, local),
        StatementKind::Drop(place) | StatementKind::Accumulate { place, .. } => whole(place, local),
    }
}

/// Whether `terminator` may change `local` as a whole, as [`writes`] describes.
pub(crate) fn terminator_writes(terminator: &TerminatorKind<'_>, local: Local) -> bool {
    let argument_writes = |arguments: &[Argument<'_>]| {
        arguments.iter().any(|argument| match argument {
            Argument::Borrow(_) => false,
            Argument::InOut(place) => place.local == local,
            Argument::Owned(operand) => moved(operand, local),
        })
    };
    match terminator {
        TerminatorKind::Call {
            arguments,
            destination,
            error,
            ..
        } => {
            destination.local == local
                || argument_writes(arguments)
                || error.as_ref().is_some_and(|edge| edge.error == local)
        }
        TerminatorKind::ParallelFor {
            arguments,
            reductions,
            accumulations,
            ..
        } => {
            argument_writes(arguments)
                || reductions
                    .iter()
                    .any(|reduction| reduction.place.local == local)
                || accumulations.iter().any(|place| place.local == local)
        }
        TerminatorKind::Raise(operand) => moved(operand, local),
        _ => false,
    }
}

/// Whether `place` is `local` or a field of it: a place whose write may change its shape.
fn whole(place: &Place, local: Local) -> bool {
    place.local == local
        && !place
            .projections
            .iter()
            .any(|step| matches!(step, Projection::Index(_) | Projection::Slice(_)))
}

/// Whether `operand` moves `local`, or a part of it, out.
fn moved(operand: &Operand<'_>, local: Local) -> bool {
    matches!(operand, Operand::Move(place) if place.local == local)
}

/// Whether `rvalue` moves `local`, or a part of it, out of one of its operands. A ready body
/// holds no whole-array expression, and the rvalues that name a place only read it.
///
/// Every statement of a loop is asked this about each local the pass follows, so it looks at
/// the operands in place rather than collecting them.
fn moves(rvalue: &Rvalue<'_>, local: Local) -> bool {
    match rvalue {
        Rvalue::Use(operand) | Rvalue::Unary(_, operand) | Rvalue::Convert(operand, _) => {
            moved(operand, local)
        }
        Rvalue::Binary(_, left, right) | Rvalue::CheckedBinary(_, left, right) => {
            moved(left, local) || moved(right, local)
        }
        Rvalue::Aggregate(_, operands) | Rvalue::NewArray(_, operands) => {
            operands.iter().any(|operand| moved(operand, local))
        }
        Rvalue::Length(..)
        | Rvalue::IsPresent(_)
        | Rvalue::ErrorIndex(_)
        | Rvalue::MemberIndex(_)
        | Rvalue::Elementwise(_)
        | Rvalue::Format(_) => false,
    }
}
