//! The structural rules every body keeps, checked by [`Body::validate`].
//!
//! Lowering and every pass call it in their tests, so a pass that breaks a rule fails where the
//! mistake is made, not in a backend three stages later.

use crate::uses::Uses;
use crate::{BlockId, Body, Local, LocalKind, Phase, TerminatorKind};
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
    /// A block of a [`Phase::Ready`] body holds an [`Elementwise`](crate::Rvalue::Elementwise) rvalue,
    /// which only built bodies may hold.
    NotExpanded(BlockId),
    /// A local of a [`Phase::Ready`] body has a type that contains a type parameter.
    NotMonomorphic(Local),
    /// A block's [`ParallelFor`](TerminatorKind::ParallelFor) names an outlined body the body
    /// does not have.
    UnknownOutlined(BlockId, u32),
    /// A block's [`ParallelFor`](TerminatorKind::ParallelFor) passes a different number of
    /// values than its outlined body has parameters: the iteration, the arguments and the
    /// reductions and the accumulations.
    OutlinedParameters(BlockId),
    /// A block's [`Accumulate`](crate::StatementKind::Accumulate) adds into a place whose local is
    /// not an accumulated parameter: one the [`ParallelFor`](TerminatorKind::ParallelFor) that
    /// runs this outlined body lists in its accumulations. An `Accumulate` outside an outlined
    /// body is always this.
    NotAccumulated(BlockId, Local),
    /// The outlined body at this position breaks a rule.
    Outlined(u32, Box<Invalid>),
    /// [`Body::vectorized`] marks a block the body does not have.
    UnknownVectorized(BlockId),
    /// A block uses a view, a place that ends in a [`Slice`](crate::Projection::Slice), as a whole
    /// value: moves or copies it, drops it, or assigns it anything but a whole-array
    /// expression. A view is a borrow and owns no memory, so any of these would store a borrow
    /// or free memory it does not own.
    WholeView(BlockId),
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
            Self::UnknownOutlined(block, body) => {
                write!(
                    formatter,
                    "bb{} runs outlined body {body}, which does not exist",
                    block.index()
                )
            }
            Self::OutlinedParameters(block) => {
                write!(
                    formatter,
                    "bb{} passes its outlined body the wrong number of values",
                    block.index()
                )
            }
            Self::NotAccumulated(block, local) => {
                write!(
                    formatter,
                    "bb{} accumulates into _{}, which is not an accumulated parameter",
                    block.index(),
                    local.index()
                )
            }
            Self::Outlined(body, problem) => {
                write!(formatter, "in outlined body {body}: {problem}")
            }
            Self::WholeView(block) => {
                write!(
                    formatter,
                    "bb{} uses a view as a whole value",
                    block.index()
                )
            }
            Self::UnknownVectorized(block) => {
                write!(
                    formatter,
                    "bb{} is marked as a `simd` loop header but does not exist",
                    block.index()
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
    /// - a [`Phase::Ready`] body has no whole-array expressions and no type parameters;
    /// - a [`ParallelFor`](TerminatorKind::ParallelFor) names an outlined body that exists,
    ///   and passes it one value per parameter;
    /// - an [`Accumulate`](crate::StatementKind::Accumulate) adds only into an accumulated parameter
    ///   of an outlined body, so never in a function's own body;
    /// - a view, a place that ends in a [`Slice`](crate::Projection::Slice), is only borrowed, read
    ///   by [`Length`](crate::Rvalue::Length), a leaf of a whole-array expression or the destination
    ///   of one: never moved, copied whole, dropped or assigned anything else;
    /// - each block marked as a `simd` loop header exists,
    /// - each outlined body keeps these rules too.
    ///
    /// It does not check types: lowering builds bodies from type-checked code, and a type check
    /// of the IR is a separate pass if bugs show one is needed.
    ///
    /// ```
    /// use mir::{Body, Invalid};
    /// use source::{FileId, Span};
    /// use types::{Float, Primitive, Type, TypeKind};
    /// # #[salsa::db]
    /// # #[derive(Clone, Default)]
    /// # struct Database { storage: salsa::Storage<Self> }
    /// # #[salsa::db]
    /// # impl salsa::Database for Database {}
    /// # #[salsa::db]
    /// # impl database::Db for Database {
    /// #     fn parse(&self, _: database::File) -> &database::ParsedFile { unreachable!() }
    /// #     fn resolve(&self, _: database::Package, _: database::File) -> &database::Resolution {
    /// #         unreachable!()
    /// #     }
    /// # }
    /// # let database = Database::default();
    /// let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    ///
    /// // A body with no blocks has nowhere to start.
    /// let body = Body::new(f64, Span::new(FileId::from_index(0), 0, 10));
    /// let problems = body.validate(&database);
    /// assert_eq!(problems, [Invalid::NoBlocks]);
    /// assert_eq!(problems[0].to_string(), "the body has no blocks");
    /// ```
    #[must_use]
    pub fn validate(&self, db: &dyn Db) -> Vec<Invalid> {
        self.check(db, &[])
    }

    /// The rules this body breaks, where `accumulated` holds the parameters its
    /// [`ParallelFor`](TerminatorKind::ParallelFor) accumulates into: none for a function's own
    /// body.
    fn check(&self, db: &dyn Db, accumulated: &[Local]) -> Vec<Invalid> {
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
            if uses.whole_view {
                problems.push(Invalid::WholeView(id));
            }
            for local in uses.accumulated {
                if !accumulated.contains(&local) {
                    problems.push(Invalid::NotAccumulated(id, local));
                }
            }
            self.parallel_for(id, &block.terminator.kind, &mut problems);
        }
        for &header in &self.vectorized {
            if header.index() >= self.blocks.len() {
                problems.push(Invalid::UnknownVectorized(header));
            }
        }
        for (position, outlined) in (0..).zip(&self.outlined) {
            let accumulated = self.accumulated(position);
            let inside = outlined.check(db, &accumulated).into_iter();
            problems.extend(inside.map(|problem| Invalid::Outlined(position, Box::new(problem))));
        }
        problems
    }

    /// The rules a [`ParallelFor`](TerminatorKind::ParallelFor) in block `id` breaks.
    fn parallel_for(
        &self,
        id: BlockId,
        terminator: &TerminatorKind<'_>,
        problems: &mut Vec<Invalid>,
    ) {
        let TerminatorKind::ParallelFor {
            body,
            arguments,
            reductions,
            accumulations,
            ..
        } = terminator
        else {
            return;
        };
        match self.outlined.get(*body as usize) {
            None => problems.push(Invalid::UnknownOutlined(id, *body)),
            Some(outlined) => {
                let values = 1 + arguments.len() + reductions.len() + accumulations.len();
                if outlined.parameter_count as usize != values {
                    problems.push(Invalid::OutlinedParameters(id));
                }
            }
        }
    }

    /// The parameters of outlined body `position` that the
    /// [`ParallelFor`](TerminatorKind::ParallelFor) running it accumulates into: the last ones,
    /// after the iteration, the arguments and the reductions. Empty when no `ParallelFor` runs
    /// it.
    fn accumulated(&self, position: u32) -> Vec<Local> {
        let mut accumulated = Vec::new();
        for block in &self.blocks {
            if let TerminatorKind::ParallelFor {
                body,
                arguments,
                reductions,
                accumulations,
                ..
            } = &block.terminator.kind
                && *body == position
            {
                let first = 2 + arguments.len() + reductions.len();
                accumulated.extend((first..first + accumulations.len()).map(Local::new));
            }
        }
        accumulated
    }
}

/// Whether `ty` is or contains a type parameter.
fn has_parameter(db: &dyn Db, ty: Type<'_>) -> bool {
    let any = |types: &[Type<'_>]| types.iter().any(|&ty| has_parameter(db, ty));
    match ty.kind(db) {
        TypeKind::Parameter { .. } => true,
        TypeKind::Primitive(_)
        | TypeKind::Str
        | TypeKind::None
        | TypeKind::Function { .. }
        | TypeKind::Standard(_)
        | TypeKind::Unknown => false,
        TypeKind::Tuple(items) | TypeKind::Union(items) => any(items),
        TypeKind::List(item) | TypeKind::Optional(item) => has_parameter(db, *item),
        TypeKind::Dict { key, value } => has_parameter(db, *key) || has_parameter(db, *value),
        TypeKind::Array { element, .. } | TypeKind::Fixed { element, .. } => {
            has_parameter(db, *element)
        }
        TypeKind::DataFrame { columns, row } => {
            columns
                .iter()
                .any(|column| has_parameter(db, column.element))
                || row.is_some_and(|row| has_parameter(db, row))
        }
        TypeKind::Struct(nominal) | TypeKind::Enum(nominal) => any(&nominal.arguments),
        TypeKind::Callable {
            parameters,
            returns,
        } => any(parameters) || has_parameter(db, *returns),
        TypeKind::Fallible { value, errors } => has_parameter(db, *value) || any(errors),
    }
}
