//! Versions one counted loop: adds a copy of it without the hoisted checks, and a block before
//! both that makes those checks once and picks the copy when they all pass.

use crate::loops::{CountedLoop, for_each_target, statement_writes};
use crate::values::{Base, Hoisted, Length, overflow_flag};
use mir::{
    BinaryOperator, BlockId, Body, Check, Constant, ConstantValue, Local, LocalKind, Operand,
    Place, Rvalue, Statement, StatementKind, Terminator, TerminatorKind,
};
use source::Span;
use types::{Primitive, Type, TypeKind};

/// Rewrites `body` so control enters `counted` through a new block that tests the `hoisted`
/// checks for every pass at once, and goes to a copy of the loop without them when they all
/// pass, or to the loop as it was when one does not.
///
/// Adds the copy's headers, and the copies of headers already in `done`, to `done`, with the
/// loop's own header, so no loop is versioned twice. A copied `simd` header keeps its mark.
pub(crate) fn version<'db>(
    db: &'db dyn salsa::Database,
    body: &mut Body<'db>,
    counted: &CountedLoop<'db>,
    hoisted: &[Hoisted],
    done: &mut Vec<usize>,
) {
    let first = body.blocks.len();
    let copy_of = |block: usize| {
        counted
            .blocks
            .iter()
            .position(|&member| member == block)
            .map(|position| first + position)
    };
    let Some(fast_header) = copy_of(counted.header) else {
        return;
    };
    let checks: Vec<usize> = hoisted.iter().map(|check| check.block).collect();
    let computed: Vec<Local> = hoisted
        .iter()
        .flat_map(|check| check.computed.iter().copied())
        .collect();
    for &block in &counted.blocks {
        let mut copy = body.blocks[block].clone();
        for_each_target(&mut copy.terminator.kind, &mut |target| {
            if let Some(copied) = copy_of(target.index()) {
                *target = BlockId::new(copied);
            }
        });
        let removed = checks.contains(&block)
            || overflow_checked(&copy.statements, &copy.terminator.kind, &computed);
        if let (true, TerminatorKind::Assert { target, .. }) = (removed, &copy.terminator.kind) {
            copy.terminator.kind = TerminatorKind::Goto(*target);
        }
        body.blocks.push(copy);
    }
    for (position, &block) in counted.blocks.iter().enumerate() {
        if body.vectorized.contains(&BlockId::new(block)) {
            body.vectorized.push(BlockId::new(first + position));
        }
        let marks: Vec<Place> = body
            .unit_strides
            .iter()
            .filter(|(marked, _)| marked.index() == block)
            .map(|(_, place)| place.clone())
            .collect();
        for place in marks {
            body.unit_strides
                .push((BlockId::new(first + position), place));
        }
        if block == counted.header || done.contains(&block) {
            done.push(first + position);
        }
    }
    done.push(counted.header);

    let span = body.blocks[counted.header].terminator.span;
    let int = body.locals[counted.counter.index()].ty;
    let mut checker = PreCheck {
        body,
        statements: Vec::new(),
        int,
        boolean: Type::new(db, TypeKind::Primitive(Primitive::Bool)),
        span,
    };
    let fast = checker.all(counted, hoisted);
    let PreCheck { statements, .. } = checker;
    let pre_check = body.push_block(Terminator {
        kind: TerminatorKind::Branch {
            condition: fast,
            then: BlockId::new(fast_header),
            otherwise: BlockId::new(counted.header),
        },
        span,
    });
    body.blocks[pre_check.index()].statements = statements;
    for &entry in &counted.entries {
        for_each_target(&mut body.blocks[entry].terminator.kind, &mut |target| {
            if target.index() == counted.header {
                *target = pre_check;
            }
        });
    }
}

/// Whether a block of `statements` ending in `terminator` ends in the overflow check of one of
/// the `computed` locals: `condition = Not(copy t.1); assert(copy condition, no overflow)`.
fn overflow_checked(
    statements: &[Statement<'_>],
    terminator: &TerminatorKind<'_>,
    computed: &[Local],
) -> bool {
    let TerminatorKind::Assert {
        condition: Operand::Copy(condition),
        check: Check::NoOverflow(_),
        ..
    } = terminator
    else {
        return false;
    };
    let last = statements
        .iter()
        .rev()
        .find(|statement| statement_writes(&statement.kind, condition.local));
    matches!(
        last.map(|statement| &statement.kind),
        Some(StatementKind::Assign(place, rvalue))
            if place == condition
                && overflow_flag(rvalue).is_some_and(|flag| computed.contains(&flag))
    )
}

/// Builds the statements of the block that makes a loop's hoisted checks before it.
struct PreCheck<'b, 'db> {
    /// The body, for new temporaries.
    body: &'b mut Body<'db>,
    /// The statements built so far.
    statements: Vec<Statement<'db>>,
    /// The type of the counter and the indices, `i64`.
    int: Type<'db>,
    /// `bool`.
    boolean: Type<'db>,
    /// The loop header's span, for the new statements.
    span: Span,
}

impl<'db> PreCheck<'_, 'db> {
    /// A `bool` that is `True` when every check of `hoisted` passes on every pass of
    /// `counted`, given its counter's value on entry.
    fn all(&mut self, counted: &CountedLoop<'db>, hoisted: &[Hoisted]) -> Operand<'db> {
        let mut all = None;
        for check in hoisted {
            let holds = self.holds(counted, check);
            all = Some(match all {
                None => holds,
                Some(earlier) => self.assign(
                    self.boolean,
                    Rvalue::Binary(BinaryOperator::BitAnd, earlier, holds),
                ),
            });
        }
        all.unwrap_or(Operand::Constant(Constant {
            ty: self.boolean,
            value: ConstantValue::Bool(true),
        }))
    }

    /// A `bool` that is `True` when `check` passes on every pass of `counted`. With `k` the
    /// index's offset and `L` the length, the index lies in `0..L` when:
    ///
    /// - for the counter, which runs over `start..stop`: `start >= -k` and `stop <= L - k`;
    /// - for a value the loop does not change, `v`: `v >= -k` and `v < L - k`;
    /// - for a constant, `k`, which is not negative: `k < L`.
    fn holds(&mut self, counted: &CountedLoop<'db>, check: &Hoisted) -> Operand<'db> {
        let length = self.length(&check.length);
        let offset = check.index.offset;
        let (base, last, operator) = match check.index.base {
            Base::Constant => {
                let index = self.integer(offset);
                return self.assign(
                    self.boolean,
                    Rvalue::Binary(BinaryOperator::Less, index, length),
                );
            }
            Base::Counter => (
                counted.counter,
                counted.stop.clone(),
                BinaryOperator::LessEqual,
            ),
            Base::Invariant(local) => (
                local,
                Operand::Copy(Place::from(local)),
                BinaryOperator::Less,
            ),
        };
        let low = self.integer(-offset);
        let above = self.assign(
            self.boolean,
            Rvalue::Binary(
                BinaryOperator::GreaterEqual,
                Operand::Copy(Place::from(base)),
                low,
            ),
        );
        let offset = self.integer(offset);
        let limit = self.assign(
            self.int,
            Rvalue::Binary(BinaryOperator::Subtract, length, offset),
        );
        let below = self.assign(self.boolean, Rvalue::Binary(operator, last, limit));
        self.assign(
            self.boolean,
            Rvalue::Binary(BinaryOperator::BitAnd, above, below),
        )
    }

    /// The length `length` as an operand, read into a temporary when it is an array's.
    fn length(&mut self, length: &Length) -> Operand<'db> {
        match length {
            Length::Constant(value) => self.integer(*value),
            Length::Invariant(local) => Operand::Copy(Place::from(*local)),
            Length::Read(array, dimension) => {
                self.assign(self.int, Rvalue::Length(array.clone(), *dimension))
            }
        }
    }

    /// The `i64` constant `value`.
    fn integer(&self, value: i128) -> Operand<'db> {
        Operand::Constant(Constant::integer(self.int, value))
    }

    /// Assigns `rvalue` to a new temporary of type `ty`, and reads it.
    fn assign(&mut self, ty: Type<'db>, rvalue: Rvalue<'db>) -> Operand<'db> {
        let local = self.body.push_local(ty, LocalKind::Temporary, self.span);
        self.statements.push(Statement {
            kind: StatementKind::Assign(Place::from(local), rvalue),
            span: self.span,
        });
        Operand::Copy(Place::from(local))
    }
}
