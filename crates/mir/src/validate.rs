//! The structural rules every body keeps, checked by [`Body::validate`].
//!
//! Lowering and every pass call it in their tests, so a pass that breaks a rule fails where the
//! mistake is made, not in a backend three stages later.

use crate::{
    Argument, BasicBlock, BlockId, Body, Callee, Elementwise, Local, LocalKind, Operand, Phase,
    Place, Projection, Rvalue, StatementKind, TerminatorKind,
};
use database::Db;
use std::fmt;
use types::{Type, TypeKind};

/// One broken rule in a body. Each one is a bug in the stage that built the body.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Invalid {
    /// The body has no blocks, so it has no entry.
    NoBlocks,
    /// The first local is not the return local, or a later one is.
    ReturnLocal(Local),
    /// A parameter is not among locals `1..=parameter_count`, or a local there is not a
    /// parameter.
    ParameterOrder(Local),
    /// A block mentions a local the body does not have.
    UnknownLocal(BlockId, Local),
    /// A block jumps to a block the body does not have.
    UnknownBlock(BlockId, BlockId),
    /// A block of a [`Phase::Ready`] body holds an [`Elementwise`](Rvalue::Elementwise) rvalue,
    /// which only built bodies may hold.
    NotExpanded(BlockId),
    /// A local of a [`Phase::Ready`] body has a type that contains a type parameter.
    NotMonomorphic(Local),
}

impl fmt::Display for Invalid {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoBlocks => formatter.write_str("the body has no blocks"),
            Self::ReturnLocal(local) => {
                write!(
                    formatter,
                    "_{} is misplaced as the return local",
                    local.index()
                )
            }
            Self::ParameterOrder(local) => {
                write!(formatter, "_{} breaks the parameter order", local.index())
            }
            Self::UnknownLocal(block, local) => {
                write!(
                    formatter,
                    "bb{} uses _{}, which does not exist",
                    block.index(),
                    local.index()
                )
            }
            Self::UnknownBlock(block, target) => {
                write!(
                    formatter,
                    "bb{} jumps to bb{}, which does not exist",
                    block.index(),
                    target.index()
                )
            }
            Self::NotExpanded(block) => {
                write!(
                    formatter,
                    "bb{} holds a whole-array expression in a ready body",
                    block.index()
                )
            }
            Self::NotMonomorphic(local) => {
                write!(
                    formatter,
                    "_{} has a type parameter in a ready body",
                    local.index()
                )
            }
        }
    }
}

impl Body<'_> {
    /// Every structural rule the body breaks, in order of local, then block; empty when it
    /// keeps them all. The rules:
    ///
    /// - there is at least one block, and every block a terminator names exists;
    /// - local 0 is the return local, and the parameters are exactly locals
    ///   `1..=parameter_count`;
    /// - every local a statement or terminator names exists;
    /// - a [`Phase::Ready`] body has no whole-array expressions and no type parameters.
    ///
    /// It does not check types: lowering builds bodies from type-checked code, and a type check
    /// of the IR is a separate pass if bugs show one is needed.
    #[must_use]
    pub fn validate(&self, db: &dyn Db) -> Vec<Invalid> {
        let mut problems = Vec::new();
        for (index, local) in self.locals.iter().enumerate() {
            let id = Local::new(index);
            let parameter = (1..=self.parameter_count as usize).contains(&index);
            match local.kind {
                LocalKind::Return if index != 0 => problems.push(Invalid::ReturnLocal(id)),
                LocalKind::Return => {}
                _ if index == 0 => problems.push(Invalid::ReturnLocal(id)),
                LocalKind::Parameter { .. } if !parameter => {
                    problems.push(Invalid::ParameterOrder(id));
                }
                LocalKind::Variable { .. } | LocalKind::Temporary if parameter => {
                    problems.push(Invalid::ParameterOrder(id));
                }
                _ => {}
            }
            if self.phase == Phase::Ready && has_parameter(db, local.ty) {
                problems.push(Invalid::NotMonomorphic(id));
            }
        }
        if self.locals.len() <= self.parameter_count as usize {
            problems.push(Invalid::ParameterOrder(Local::new(self.locals.len())));
        }
        if self.blocks.is_empty() {
            problems.push(Invalid::NoBlocks);
        }
        for (index, block) in self.blocks.iter().enumerate() {
            let mut uses = Uses::default();
            uses.block(block);
            let id = BlockId::new(index);
            for local in uses.locals {
                if local.index() >= self.locals.len() {
                    problems.push(Invalid::UnknownLocal(id, local));
                }
            }
            for target in uses.blocks {
                if target.index() >= self.blocks.len() {
                    problems.push(Invalid::UnknownBlock(id, target));
                }
            }
            if self.phase == Phase::Ready && uses.elementwise {
                problems.push(Invalid::NotExpanded(id));
            }
        }
        problems
    }
}

/// Whether `ty` is or contains a type parameter.
fn has_parameter(db: &dyn Db, ty: Type<'_>) -> bool {
    let any = |types: &[Type<'_>]| types.iter().any(|&ty| has_parameter(db, ty));
    match ty.kind(db) {
        TypeKind::Parameter { .. } => true,
        TypeKind::Primitive(_) | TypeKind::Str | TypeKind::None | TypeKind::Unknown => false,
        TypeKind::Tuple(items) => any(items),
        TypeKind::List(item) | TypeKind::Optional(item) => has_parameter(db, *item),
        TypeKind::Array { element, .. } | TypeKind::Fixed { element, .. } => {
            has_parameter(db, *element)
        }
        TypeKind::Struct(nominal) | TypeKind::Enum(nominal) => any(&nominal.arguments),
        TypeKind::Callable {
            parameters,
            returns,
        } => any(parameters) || has_parameter(db, *returns),
        TypeKind::Fallible { value, errors } => has_parameter(db, *value) || any(errors),
    }
}

/// What one block mentions: the locals and blocks it names, and whether it holds a whole-array
/// expression.
#[derive(Default)]
struct Uses {
    /// Every local named, in order, with repeats.
    locals: Vec<Local>,
    /// Every block named, in order, with repeats.
    blocks: Vec<BlockId>,
    /// Whether an [`Rvalue::Elementwise`] appears.
    elementwise: bool,
}

impl Uses {
    /// Records everything `block` names.
    fn block(&mut self, block: &BasicBlock<'_>) {
        for statement in &block.statements {
            match &statement.kind {
                StatementKind::Assign(place, rvalue) => {
                    self.place(place);
                    self.rvalue(rvalue);
                }
                StatementKind::Drop(place) => self.place(place),
            }
        }
        match &block.terminator.kind {
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
                for argument in arguments {
                    match argument {
                        Argument::Borrow(place) | Argument::InOut(place) => self.place(place),
                        Argument::Owned(operand) => self.operand(operand),
                    }
                }
                self.place(destination);
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
        }
    }

    /// Records the locals `place` names: its base and its indices.
    fn place(&mut self, place: &Place) {
        self.locals.push(place.local);
        for projection in &place.projections {
            if let Projection::Index(indices) = projection {
                self.locals.extend(indices);
            }
        }
    }

    /// Records the locals `operand` names.
    fn operand(&mut self, operand: &Operand<'_>) {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => self.place(place),
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
            Rvalue::Length(place, _) | Rvalue::IsPresent(place) | Rvalue::ErrorIndex(place) => {
                self.place(place);
            }
            Rvalue::Elementwise(tree) => {
                self.elementwise = true;
                self.elementwise(tree);
            }
        }
    }

    /// Records the locals the leaves of a whole-array expression name.
    fn elementwise(&mut self, tree: &Elementwise<'_>) {
        match tree {
            Elementwise::Leaf(operand) => self.operand(operand),
            Elementwise::Unary(_, operand) => self.elementwise(operand),
            Elementwise::Binary(_, left, right) => {
                self.elementwise(left);
                self.elementwise(right);
            }
        }
    }
}
