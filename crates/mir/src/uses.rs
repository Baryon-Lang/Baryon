//! What a statement, a terminator or a block mentions, for [`Body::validate`](crate::Body::validate)
//! and for passes that need to know where a local is used, such as array fusion (roadmap 3.1).

use crate::{
    Argument, BasicBlock, BlockId, Callee, Elementwise, Local, Operand, Piece, Place, Projection,
    Rvalue, SliceIndex, StatementKind, TerminatorKind,
};

impl StatementKind<'_> {
    /// Every local the statement names, in order and with repeats: the places it writes,
    /// reads, moves or drops, and the locals that index them.
    ///
    /// ```
    /// use mir::{Local, Operand, Rvalue, StatementKind};
    ///
    /// let (a, b) = (Local::new(1), Local::new(2));
    /// let statement: StatementKind<'_> =
    ///     StatementKind::Assign(a.into(), Rvalue::Use(Operand::Copy(b.into())));
    /// assert_eq!(statement.locals(), vec![a, b]);
    /// ```
    #[must_use]
    pub fn locals(&self) -> Vec<Local> {
        let mut uses = Uses::default();
        uses.statement(self);
        uses.locals
    }
}

impl TerminatorKind<'_> {
    /// Every local the terminator names, in order and with repeats: its operands, its call's
    /// arguments and destination, a parallel loop's arguments, reductions and accumulations,
    /// and the error local of a call's error edge.
    ///
    /// ```
    /// use mir::{BlockId, Local, Operand, TerminatorKind};
    ///
    /// let condition = Local::new(1);
    /// let branch: TerminatorKind<'_> = TerminatorKind::Branch {
    ///     condition: Operand::Copy(condition.into()),
    ///     then: BlockId::new(1),
    ///     otherwise: BlockId::new(2),
    /// };
    /// assert_eq!(branch.locals(), vec![condition]);
    /// ```
    #[must_use]
    pub fn locals(&self) -> Vec<Local> {
        let mut uses = Uses::default();
        uses.terminator(self);
        uses.locals
    }
}

/// What one block mentions: the locals and blocks it names, and whether it holds a whole-array
/// expression.
#[derive(Default)]
pub(crate) struct Uses {
    /// Every local named, in order, with repeats.
    pub(crate) locals: Vec<Local>,
    /// Every block named, in order, with repeats.
    pub(crate) blocks: Vec<BlockId>,
    /// The local of every place an [`Accumulate`](StatementKind::Accumulate) adds into.
    pub(crate) accumulated: Vec<Local>,
    /// Whether an [`Rvalue::Elementwise`] appears.
    pub(crate) elementwise: bool,
    /// Whether a view is used as a whole value.
    pub(crate) whole_view: bool,
}

impl Uses {
    /// Records everything `block` names.
    pub(crate) fn block(&mut self, block: &BasicBlock<'_>) {
        for statement in &block.statements {
            self.statement(&statement.kind);
        }
        self.terminator(&block.terminator.kind);
    }

    /// Records everything `statement` names.
    fn statement(&mut self, statement: &StatementKind<'_>) {
        match statement {
            StatementKind::Assign(place, rvalue) => {
                self.whole_view |= place.is_view() && !matches!(rvalue, Rvalue::Elementwise(_));
                self.place(place);
                self.rvalue(rvalue);
            }
            StatementKind::Drop(place) => self.value(place),
            StatementKind::Accumulate { place, value, .. } => {
                self.value(place);
                self.operand(value);
                self.accumulated.push(place.local);
            }
        }
    }

    /// Records everything `terminator` names.
    fn terminator(&mut self, terminator: &TerminatorKind<'_>) {
        match terminator {
            TerminatorKind::Goto(target) => self.blocks.push(*target),
            TerminatorKind::Branch {
                condition,
                then,
                otherwise,
            } => {
                self.operand(condition);
                self.blocks.extend([*then, *otherwise]);
            }
            TerminatorKind::Switch {
                value,
                cases,
                otherwise,
            } => {
                self.operand(value);
                self.blocks.extend(cases.iter().map(|&(_, target)| target));
                self.blocks.push(*otherwise);
            }
            TerminatorKind::Return | TerminatorKind::Unreachable => {}
            TerminatorKind::Raise(error) => self.operand(error),
            TerminatorKind::Call {
                callee,
                arguments,
                destination,
                target,
                error,
            } => {
                if let Callee::Value(function) = callee {
                    self.operand(function);
                }
                self.arguments(arguments);
                self.value(destination);
                self.blocks.push(*target);
                if let Some(edge) = error {
                    self.locals.push(edge.error);
                    self.blocks.push(edge.target);
                }
            }
            TerminatorKind::Assert {
                condition,
                check,
                target,
            } => {
                self.operand(condition);
                for operand in check.operands() {
                    self.operand(operand);
                }
                self.blocks.push(*target);
            }
            TerminatorKind::ParallelFor {
                count,
                arguments,
                reductions,
                accumulations,
                target,
                ..
            } => {
                self.operand(count);
                self.arguments(arguments);
                for reduction in reductions {
                    self.value(&reduction.place);
                }
                for accumulation in accumulations {
                    self.value(accumulation);
                }
                self.blocks.push(*target);
            }
        }
    }

    /// Records the locals call arguments name.
    fn arguments(&mut self, arguments: &[Argument<'_>]) {
        for argument in arguments {
            match argument {
                Argument::Borrow(place) | Argument::InOut(place) => self.place(place),
                Argument::Owned(operand) => self.operand(operand),
            }
        }
    }

    /// Records the locals `place` names: its base and its indices.
    fn place(&mut self, place: &Place) {
        self.locals.push(place.local);
        for projection in &place.projections {
            match projection {
                Projection::Index(indices) => self.locals.extend(indices),
                Projection::Slice(dimensions) => {
                    self.locals
                        .extend(dimensions.iter().flat_map(SliceIndex::locals));
                }
                Projection::Field(_)
                | Projection::Value
                | Projection::Error(_)
                | Projection::Member(_) => {}
            }
        }
    }

    /// Records the locals `place` names, where it is used as a whole value, which a view
    /// cannot be.
    fn value(&mut self, place: &Place) {
        self.whole_view |= place.is_view();
        self.place(place);
    }

    /// Records the locals `operand` names. Its value is read whole, which a view's cannot be.
    fn operand(&mut self, operand: &Operand<'_>) {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => self.value(place),
            Operand::Constant(_) => {}
        }
    }

    /// Records the locals `rvalue` names, and whether it is a whole-array expression.
    fn rvalue(&mut self, rvalue: &Rvalue<'_>) {
        match rvalue {
            Rvalue::Use(operand) | Rvalue::Unary(_, operand) | Rvalue::Convert(operand, _) => {
                self.operand(operand);
            }
            Rvalue::Binary(_, left, right) | Rvalue::CheckedBinary(_, left, right) => {
                self.operand(left);
                self.operand(right);
            }
            Rvalue::Aggregate(_, operands) | Rvalue::NewArray(_, operands) => {
                for operand in operands {
                    self.operand(operand);
                }
            }
            Rvalue::Length(place, _)
            | Rvalue::Stride(place, _)
            | Rvalue::IsPresent(place)
            | Rvalue::ErrorIndex(place)
            | Rvalue::MemberIndex(place) => {
                self.place(place);
            }
            Rvalue::Elementwise(tree) => {
                self.elementwise = true;
                self.elementwise(tree);
            }
            Rvalue::Format(pieces) => {
                for piece in pieces {
                    if let Piece::Value(place, _) = piece {
                        self.value(place);
                    }
                }
            }
        }
    }

    /// Records the locals the leaves of a whole-array expression name.
    fn elementwise(&mut self, tree: &Elementwise<'_>) {
        match tree {
            // A leaf is read one element at a time, so it may be a view; moving one may not.
            Elementwise::Leaf(Operand::Copy(place)) => self.place(place),
            Elementwise::Leaf(operand) => self.operand(operand),
            Elementwise::Unary(_, operand) => self.elementwise(operand),
            Elementwise::Binary(_, left, right) => {
                self.elementwise(left);
                self.elementwise(right);
            }
            Elementwise::Fused(tree, _) => self.elementwise(tree),
        }
    }
}
