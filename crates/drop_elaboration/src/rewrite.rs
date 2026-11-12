//! Rewrites a body's drops from what the dataflow analysis found: keeps, removes, guards or
//! opens each one, and keeps the drop flags up to date.

use crate::dataflow::State;
use crate::paths::{Effect, Paths, is_field_path, statement_effects, terminator_effects};
use mir::{
    BasicBlock, BlockId, Body, Constant, ConstantValue, Local, LocalKind, Operand, Place,
    Projection, Rvalue, Statement, StatementKind, Terminator, TerminatorKind,
};
use source::Span;
use std::collections::HashMap;
use types::{Declared, Nominal, Primitive, Type, TypeKind};

/// One drop to emit in place of a built drop.
struct Action {
    /// The place to drop.
    place: Place,
    /// The move path whose flag guards the drop, or `None` when the value is certainly there.
    flag: Option<usize>,
}

/// What deciding a drop needs: the body's types and what each struct declares.
struct Decider<'a, 'db> {
    /// The database types are read from.
    db: &'db dyn salsa::Database,
    /// The body's locals, for the type of each place.
    locals: &'a [mir::LocalDeclaration<'db>],
    /// The body's move paths.
    paths: &'a Paths,
    /// What each struct declares.
    declared: &'a dyn Fn(&Nominal<'db>) -> Declared<'db>,
}

/// The drops that replace each built drop of a move path, keyed by block and statement position.
type Decisions = HashMap<(usize, usize), Vec<Action>>;

/// Rewrites every drop of a move path in `body` and maintains the flags the rewritten drops read.
pub(crate) fn rewrite<'db>(
    db: &'db dyn salsa::Database,
    body: &mut Body<'db>,
    paths: &Paths,
    entries: &[State],
    declared: &dyn Fn(&Nominal<'db>) -> Declared<'db>,
) {
    let decider = Decider {
        db,
        locals: &body.locals,
        paths,
        declared,
    };
    let mut decisions = decider.decide_all(&body.blocks, entries);
    let flags = Flags::allocate(db, body, paths, &decisions);
    let original = std::mem::take(&mut body.blocks);
    let mut out: Vec<BasicBlock<'db>> = original.iter().map(|_| placeholder(body.span)).collect();
    for (block_index, block) in original.into_iter().enumerate() {
        let mut current = BlockId::new(block_index);
        let mut statements = Vec::new();
        if current == BlockId::ENTRY {
            statements.extend(flags.initial(body.parameter_count, body.span));
        }
        for (statement_index, statement) in block.statements.into_iter().enumerate() {
            let effects = statement_effects(&statement.kind);
            let span = statement.span;
            match decisions.remove(&(block_index, statement_index)) {
                None => statements.push(statement),
                Some(actions) => {
                    for action in actions {
                        let drop = Statement {
                            kind: StatementKind::Drop(action.place),
                            span,
                        };
                        match action.flag.and_then(|path| flags.locals[path]) {
                            None => statements.push(drop),
                            Some(flag) => {
                                let before = std::mem::take(&mut statements);
                                current = guard(&mut out, current, before, flag, drop);
                            }
                        }
                    }
                }
            }
            statements.extend(flags.updates(&effects, span));
        }
        let mut terminator = block.terminator;
        statements.extend(flags.updates(&terminator_effects(&terminator.kind), terminator.span));
        flags.split_call_edges(&mut out, &mut terminator);
        out[current.index()] = BasicBlock {
            statements,
            terminator,
        };
    }
    body.blocks = out;
}

/// An empty block that stands in for one whose statements are still being written.
fn placeholder<'db>(span: Span) -> BasicBlock<'db> {
    BasicBlock {
        statements: Vec::new(),
        terminator: Terminator {
            kind: TerminatorKind::Unreachable,
            span,
        },
    }
}

/// Ends block `current` with `statements` and a branch on `flag` to a new block that runs `drop`,
/// and returns the new block both paths continue in.
fn guard<'db>(
    out: &mut Vec<BasicBlock<'db>>,
    current: BlockId,
    statements: Vec<Statement<'db>>,
    flag: Local,
    drop: Statement<'db>,
) -> BlockId {
    let span = drop.span;
    let dropping = BlockId::new(out.len());
    let after = BlockId::new(out.len() + 1);
    out.push(BasicBlock {
        statements: vec![drop],
        terminator: Terminator {
            kind: TerminatorKind::Goto(after),
            span,
        },
    });
    out.push(placeholder(span));
    out[current.index()] = BasicBlock {
        statements,
        terminator: Terminator {
            kind: TerminatorKind::Branch {
                condition: Operand::Copy(flag.into()),
                then: dropping,
                otherwise: after,
            },
            span,
        },
    };
    after
}

/// The block to jump to instead of `target` so that `updates` run on this edge only: `target`
/// itself when there are none, otherwise a new block that runs them and goes on to `target`.
fn split_edge<'db>(
    out: &mut Vec<BasicBlock<'db>>,
    target: BlockId,
    updates: Vec<Statement<'db>>,
    span: Span,
) -> BlockId {
    if updates.is_empty() {
        return target;
    }
    out.push(BasicBlock {
        statements: updates,
        terminator: Terminator {
            kind: TerminatorKind::Goto(target),
            span,
        },
    });
    BlockId::new(out.len() - 1)
}

/// The drop flags: one `bool` local for each move path a guarded drop reads.
struct Flags<'a, 'db> {
    /// The `bool` type.
    bool: Type<'db>,
    /// The body's move paths.
    paths: &'a Paths,
    /// The flag of each path, by path position; `None` for paths no drop is guarded by.
    locals: Vec<Option<Local>>,
}

impl<'a, 'db> Flags<'a, 'db> {
    /// A new `bool` local in `body` for each move path a decided drop is guarded by.
    fn allocate(
        db: &'db dyn salsa::Database,
        body: &mut Body<'db>,
        paths: &'a Paths,
        decisions: &Decisions,
    ) -> Self {
        let bool = Type::new(db, TypeKind::Primitive(Primitive::Bool));
        let mut flags: Vec<Option<Local>> = vec![None; paths.len()];
        for path in decisions
            .values()
            .flatten()
            .filter_map(|action| action.flag)
        {
            if flags[path].is_none() {
                flags[path] = Some(body.push_local(bool, LocalKind::Temporary, body.span));
            }
        }
        Self {
            bool,
            paths,
            locals: flags,
        }
    }

    /// Moves the flag updates of a call's edges onto the edges: the destination is filled only on
    /// success and the error local only on failure, so each update goes in a block of its own.
    fn split_call_edges(&self, out: &mut Vec<BasicBlock<'db>>, terminator: &mut Terminator<'db>) {
        let span = terminator.span;
        let TerminatorKind::Call {
            destination,
            target,
            error,
            ..
        } = &mut terminator.kind
        else {
            return;
        };
        if is_field_path(destination) {
            let updates = self.updates(&[Effect::Init(destination.clone())], span);
            *target = split_edge(out, *target, updates, span);
        }
        if let Some(edge) = error {
            let updates = self.updates(&[Effect::Init(Place::from(edge.error))], span);
            edge.target = split_edge(out, edge.target, updates, span);
        }
    }

    /// The statements that set every flag at the entry: `True` for a parameter's paths, which
    /// hold the argument, `False` for every other.
    fn initial(&self, parameter_count: u32, span: Span) -> Vec<Statement<'db>> {
        let state = State::entry(self.paths, parameter_count);
        (0..self.paths.len())
            .filter_map(|path| Some((self.locals[path]?, state.maybe_init[path])))
            .map(|(flag, value)| self.set(flag, value, span))
            .collect()
    }

    /// The statements that bring the flags up to date after `effects`.
    fn updates(&self, effects: &[Effect], span: Span) -> Vec<Statement<'db>> {
        let mut statements = Vec::new();
        for effect in effects {
            let (place, value) = match effect {
                Effect::Init(place) => (place, true),
                Effect::Uninit(place) => (place, false),
            };
            for path in self.paths.under(place) {
                if let Some(flag) = self.locals[path] {
                    statements.push(self.set(flag, value, span));
                }
            }
        }
        statements
    }

    /// `flag = value`.
    fn set(&self, flag: Local, value: bool, span: Span) -> Statement<'db> {
        Statement {
            kind: StatementKind::Assign(
                flag.into(),
                Rvalue::Use(Operand::Constant(Constant {
                    ty: self.bool,
                    value: ConstantValue::Bool(value),
                })),
            ),
            span,
        }
    }
}

impl<'db> Decider<'_, 'db> {
    /// Decides every built drop of a move path in `blocks`, from the state at each block's start.
    fn decide_all(&self, blocks: &[BasicBlock<'db>], entries: &[State]) -> Decisions {
        let mut decisions = HashMap::new();
        for (block_index, block) in blocks.iter().enumerate() {
            let mut state = entries[block_index].clone();
            for (statement_index, statement) in block.statements.iter().enumerate() {
                if let StatementKind::Drop(place) = &statement.kind
                    && is_field_path(place)
                {
                    let mut actions = Vec::new();
                    self.decide(place, None, &state, &mut actions);
                    decisions.insert((block_index, statement_index), actions);
                }
                for effect in statement_effects(&statement.kind) {
                    state.apply(self.paths, &effect);
                }
            }
        }
        decisions
    }

    /// The drops that replace a drop of `place`, given the state before it. `governing` is the
    /// nearest tracked path at or above `place` already found: an untracked part of a value is
    /// there exactly when that path is.
    fn decide(
        &self,
        place: &Place,
        governing: Option<usize>,
        state: &State,
        actions: &mut Vec<Action>,
    ) {
        let Some(governing) = self.paths.index(place).or(governing) else {
            actions.push(Action {
                place: place.clone(),
                flag: None,
            });
            return;
        };
        if !state.maybe_init[governing] {
            return;
        }
        let partly_moved = self
            .paths
            .strictly_under(place)
            .any(|path| state.maybe_uninit[path]);
        let fields = if partly_moved {
            self.fields(place)
        } else {
            None
        };
        match fields {
            Some(fields) => {
                for (position, ty) in (0u32..).zip(fields) {
                    if self.owns_memory(ty, &mut Vec::new()) {
                        let field = place.clone().project(Projection::Field(position));
                        self.decide(&field, Some(governing), state, actions);
                    }
                }
            }
            None => actions.push(Action {
                place: place.clone(),
                flag: state.maybe_uninit[governing].then_some(governing),
            }),
        }
    }

    /// The field types of the struct or tuple at `place`, or `None` if it holds neither.
    fn fields(&self, place: &Place) -> Option<Vec<Type<'db>>> {
        let mut ty = self.locals.get(place.local.index())?.ty;
        for projection in &place.projections {
            let Projection::Field(position) = projection else {
                return None;
            };
            ty = *self.parts(ty)?.get(*position as usize)?;
        }
        self.parts(ty)
    }

    /// The field types of a struct, or the item types of a tuple.
    fn parts(&self, ty: Type<'db>) -> Option<Vec<Type<'db>>> {
        match ty.kind(self.db) {
            TypeKind::Tuple(items) => Some(items.clone()),
            TypeKind::Struct(nominal) => match (self.declared)(nominal) {
                Declared::Struct(fields) => Some(fields),
                Declared::Enum { .. } => None,
            },
            _ => None,
        }
    }

    /// Whether a value of type `ty` owns memory a drop must free, or a file a drop must close.
    /// `open` holds the structs being looked into, so a struct that contains itself, an error
    /// reported earlier, ends the search.
    fn owns_memory(&self, ty: Type<'db>, open: &mut Vec<Type<'db>>) -> bool {
        match ty.kind(self.db) {
            TypeKind::Primitive(_)
            | TypeKind::None
            | TypeKind::Enum(_)
            | TypeKind::Fixed { .. }
            | TypeKind::Callable { .. }
            | TypeKind::Function { .. } => false,
            // A `File` holds an open file a drop closes; a `Philox` is only numbers.
            TypeKind::Standard(standard) => standard.owns_resources(),
            TypeKind::Str
            | TypeKind::List(_)
            | TypeKind::Dict { .. }
            | TypeKind::Array { .. }
            | TypeKind::DataFrame { .. }
            | TypeKind::Parameter { .. }
            | TypeKind::Unknown => true,
            TypeKind::Optional(value) => self.owns_memory(*value, open),
            TypeKind::Tuple(items) | TypeKind::Union(items) => {
                items.iter().any(|&item| self.owns_memory(item, open))
            }
            TypeKind::Fallible { value, errors } => {
                self.owns_memory(*value, open)
                    || errors.iter().any(|&error| self.owns_memory(error, open))
            }
            TypeKind::Struct(_) => {
                if open.contains(&ty) {
                    return false;
                }
                open.push(ty);
                let owns = self.parts(ty).is_some_and(|fields| {
                    fields.iter().any(|&field| self.owns_memory(field, open))
                });
                open.pop();
                owns
            }
        }
    }
}
