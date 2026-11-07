//! The state of lowering one function: the body being built, the block being filled, and what
//! each name of the function became.

use database::{Definition, File, Package, Resolution};
use diagnostics::Diagnostic;
use mir::{
    BlockId, Body, Local, LocalKind, Place, Statement, StatementKind, Terminator, TerminatorKind,
};
use std::collections::BTreeMap;
use syntax::{NodeId, Role, SyntaxKind, SyntaxTree};
use types::{Item, Mode, Signature, Type, TypeDb, TypeKind, TypedBody};

/// Lowers one function. The `statements`, `expressions` and `calls` modules add its methods.
pub(crate) struct Builder<'db> {
    /// The database.
    pub(crate) db: &'db dyn TypeDb,
    /// The package the function belongs to, which its names are resolved in.
    pub(crate) package: Package,
    /// The file that holds the function.
    pub(crate) file: File,
    /// The file's tree.
    pub(crate) tree: &'db SyntaxTree,
    /// The file's text.
    pub(crate) text: &'db str,
    /// What each name in the file refers to.
    resolution: &'db Resolution,
    /// The type checker's results for the function.
    pub(crate) typed: &'db TypedBody<'db>,
    /// The function's `FunctionDefinition` node.
    pub(crate) function: NodeId,
    /// The class a method belongs to; `None` for a function.
    pub(crate) class: Option<NodeId>,
    /// The function's return type, without its errors: `T` for `Raises[T, E]`.
    pub(crate) returns: Type<'db>,
    /// The body built so far.
    pub(crate) body: Body<'db>,
    /// The block statements are added to.
    current: BlockId,
    /// Whether control can reach the end of the current block. After `return`, `raise`,
    /// `break` and `continue` it cannot, and the next statement starts a new block.
    dead: bool,
    /// The local of each parameter and variable, by the `Name` node of its first binding.
    variables: BTreeMap<NodeId, Local>,
    /// For each statement being lowered, outermost first, the temporaries it made that own
    /// memory and were not moved, which it drops when it ends.
    temporaries: Vec<Vec<Local>>,
    /// The enclosing loops, innermost last.
    pub(crate) loops: Vec<Loop>,
    /// The `try` statements whose bodies are being lowered, innermost last: where an error
    /// raised inside one goes.
    pub(crate) handlers: Vec<Handler<'db>>,
    /// The `except` clauses whose bodies are being lowered, innermost last: what a bare `raise`
    /// raises again.
    pub(crate) caught: Vec<Clause<'db>>,
    /// One error per construct the first slice does not lower.
    diagnostics: Vec<Diagnostic>,
}

/// One `try` statement whose body is being lowered.
pub(crate) struct Handler<'db> {
    /// Its `except` clauses, in source order.
    pub(crate) clauses: Vec<Clause<'db>>,
    /// How many temporary scopes were open when the `try` started. An error caught by it drops
    /// the temporaries of the scopes opened since.
    pub(crate) depth: usize,
}

/// One `except` clause of a `try`.
#[derive(Clone)]
pub(crate) struct Clause<'db> {
    /// The `ExceptHandler` node.
    pub(crate) node: NodeId,
    /// The error types it catches, in the order it names them.
    pub(crate) errors: Vec<Type<'db>>,
    /// Where each caught error is stored, by position in `errors`: the variable `as` binds for
    /// a clause of one type, otherwise a temporary of that error's type.
    pub(crate) locals: Vec<Local>,
    /// For a clause of several types, an `i64` local that says which one was caught, so a bare
    /// `raise` raises that one again.
    pub(crate) which: Option<Local>,
    /// The block its body starts in.
    pub(crate) block: BlockId,
}

/// Where `break` and `continue` go in one loop.
#[derive(Clone, Copy)]
pub(crate) struct Loop {
    /// Where `continue` goes: the condition of a `while`, the step of a `for`.
    pub(crate) next: BlockId,
    /// Where `break` goes: after the loop, past its `else`.
    pub(crate) end: BlockId,
}

impl<'db> Builder<'db> {
    /// A builder for the function `function`, of the class `class` when it is a method, with
    /// one local per parameter and per variable, and an empty entry block.
    pub(crate) fn new(
        db: &'db dyn TypeDb,
        item: Item<'db>,
        signature: &Signature<'db>,
        typed: &'db TypedBody<'db>,
        function: NodeId,
        class: Option<NodeId>,
    ) -> Self {
        let (package, file) = (item.package(db), item.file(db));
        let tree = db.parse(file).tree();
        let returns = match signature.returns.kind(db) {
            TypeKind::Fallible { value, .. } => *value,
            _ => signature.returns,
        };
        let mut body = Body::new(returns, tree.span(function));
        let mut variables = BTreeMap::new();
        let parameters = tree
            .child(function, Role::Parameters)
            .into_iter()
            .flat_map(|parameters| tree.children(parameters))
            .filter(|&parameter| tree.kind(parameter) == SyntaxKind::Parameter)
            .filter_map(|parameter| tree.child(parameter, Role::Name));
        for (name, parameter) in parameters.zip(&signature.parameters) {
            let kind = LocalKind::Parameter {
                name: parameter.name.clone(),
                mode: parameter.mode,
            };
            let local = body.push_local(parameter.ty, kind, tree.span(name));
            variables.insert(name, local);
        }
        let text = file.source(db).text();
        for (key, &ty) in &typed.locals {
            if let std::collections::btree_map::Entry::Vacant(entry) = variables.entry(key.0) {
                let name = tree.text(key.0, text).to_owned();
                entry.insert(body.push_local(ty, LocalKind::Variable { name }, tree.span(key.0)));
            }
        }
        let span = body.span;
        let current = body.push_block(Terminator {
            kind: TerminatorKind::Unreachable,
            span,
        });
        Self {
            db,
            package,
            file,
            tree,
            text,
            resolution: db.resolve(package, file),
            typed,
            function,
            class,
            returns,
            body,
            current,
            dead: false,
            variables,
            temporaries: vec![Vec::new()],
            loops: Vec::new(),
            handlers: Vec::new(),
            caught: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    /// The finished body, or the constructs it could not lower.
    pub(crate) fn finish(self) -> Result<Body<'db>, Vec<Diagnostic>> {
        if self.diagnostics.is_empty() {
            Ok(self.body)
        } else {
            Err(self.diagnostics)
        }
    }

    /// Reports that `node`, a `what`, is outside the first slice.
    pub(crate) fn unsupported(&mut self, node: NodeId, what: &str) {
        let diagnostic = Diagnostic::error(format!("lowering does not handle {what} yet"))
            .with_primary(self.tree.span(node), "not lowered yet")
            .with_note("the compiler lowers more of the language as roadmap item 1.9.3 continues");
        self.diagnostics.push(diagnostic);
    }

    /// What the name `node` refers to.
    pub(crate) fn definition(&self, node: NodeId) -> Option<&'db Definition> {
        self.resolution.definition(node)
    }

    /// The local of the variable or parameter the name `node` refers to.
    pub(crate) fn variable(&self, node: NodeId) -> Option<Local> {
        match self.definition(node)? {
            Definition::Local(binding) => self.variables.get(binding).copied(),
            _ => None,
        }
    }

    /// The type of the expression `node`, as the checker found it; unknown if it has none.
    pub(crate) fn ty(&self, node: NodeId) -> Type<'db> {
        self.typed
            .expression(node)
            .unwrap_or_else(|| Type::new(self.db, TypeKind::Unknown))
    }

    /// The type a local holds.
    pub(crate) fn local_type(&self, local: Local) -> Type<'db> {
        self.body.locals[local.index()].ty
    }

    /// A new temporary of type `ty`, for the value of `node`.
    pub(crate) fn temporary(&mut self, ty: Type<'db>, node: NodeId) -> Local {
        self.body
            .push_local(ty, LocalKind::Temporary, self.tree.span(node))
    }

    /// Marks the temporary `local` to be dropped when the current statement ends, if its type
    /// owns memory.
    pub(crate) fn drop_later(&mut self, local: Local) {
        if self.needs_drop(self.local_type(local))
            && let Some(scope) = self.temporaries.last_mut()
        {
            scope.push(local);
        }
    }

    /// Adds a statement that does `kind`, for `node`, to the current block.
    pub(crate) fn push(&mut self, kind: StatementKind<'db>, node: NodeId) {
        self.revive();
        let span = self.tree.span(node);
        self.body.blocks[self.current.index()]
            .statements
            .push(Statement { kind, span });
    }

    /// Adds `place = rvalue`, for `node`.
    pub(crate) fn assign(&mut self, place: Place, rvalue: mir::Rvalue<'db>, node: NodeId) {
        self.push(StatementKind::Assign(place, rvalue), node);
    }

    /// A new empty block, to be filled later.
    pub(crate) fn new_block(&mut self) -> BlockId {
        let span = self.body.span;
        self.body.push_block(Terminator {
            kind: TerminatorKind::Unreachable,
            span,
        })
    }

    /// Ends the current block with `kind`, for `node`. Control cannot reach what follows until
    /// [`start`](Self::start) names the next block.
    pub(crate) fn terminate(&mut self, kind: TerminatorKind<'db>, node: NodeId) {
        self.revive();
        let span = self.tree.span(node);
        self.body.blocks[self.current.index()].terminator = Terminator { kind, span };
        self.dead = true;
    }

    /// Continues in `block`.
    pub(crate) fn start(&mut self, block: BlockId) {
        self.current = block;
        self.dead = false;
    }

    /// Ends the current block with `kind`, for `node`, and continues in a new block, the one
    /// `kind` names as `next`.
    pub(crate) fn then(&mut self, node: NodeId, kind: impl FnOnce(BlockId) -> TerminatorKind<'db>) {
        let next = self.new_block();
        self.terminate(kind(next), node);
        self.start(next);
    }

    /// Jumps to `target`, creating it the first time, unless control cannot reach here. Joins
    /// use it so a join no branch reaches is never made.
    pub(crate) fn jump(&mut self, target: &mut Option<BlockId>, node: NodeId) {
        if !self.dead {
            let block = *target.get_or_insert_with(|| self.new_block());
            self.terminate(TerminatorKind::Goto(block), node);
        }
    }

    /// Continues in the join `target` if a branch reached it; otherwise control cannot reach
    /// what follows.
    pub(crate) fn join(&mut self, target: Option<BlockId>) {
        match target {
            Some(block) => self.start(block),
            None => self.dead = true,
        }
    }

    /// Whether control can reach the current point.
    pub(crate) fn is_dead(&self) -> bool {
        self.dead
    }

    /// Starts a new block for code control cannot reach, after `return` and the like, so it
    /// never lands in a block that is already terminated.
    fn revive(&mut self) {
        if self.dead {
            let block = self.new_block();
            self.start(block);
        }
    }

    /// Starts a statement, or a condition, whose owning temporaries are dropped when it ends.
    pub(crate) fn enter(&mut self) {
        self.temporaries.push(Vec::new());
    }

    /// Ends what [`enter`](Self::enter) started, for `node`: drops its owning temporaries, the
    /// last made first, unless control cannot get here, when the exit that left already dropped
    /// them.
    pub(crate) fn leave(&mut self, node: NodeId) {
        let locals = self.temporaries.pop().unwrap_or_default();
        if !self.dead {
            for local in locals.into_iter().rev() {
                self.push(StatementKind::Drop(Place::from(local)), node);
            }
        }
    }

    /// Whether [`drop_all`](Self::drop_all) would drop `local`: a variable or `Owned` parameter
    /// that owns memory, or an owning temporary of a statement being lowered.
    pub(crate) fn dropped_on_exit(&self, local: Local) -> bool {
        self.temporaries
            .iter()
            .flatten()
            .any(|&temporary| temporary == local)
            || self.owners().contains(&local)
    }

    /// How many temporary scopes are open, for [`drop_temporaries`](Self::drop_temporaries).
    pub(crate) fn depth(&self) -> usize {
        self.temporaries.len()
    }

    /// Drops the owning temporaries of every scope past the first `depth`, innermost first, for
    /// a jump out of those scopes to `node`'s handler. The scopes stay open: the code after the
    /// jump is unreachable, and each scope still ends where it was entered.
    pub(crate) fn drop_temporaries(&mut self, depth: usize, node: NodeId) {
        let temporaries: Vec<Local> = self
            .temporaries
            .get(depth..)
            .unwrap_or_default()
            .iter()
            .flatten()
            .rev()
            .copied()
            .collect();
        for local in temporaries {
            self.push(StatementKind::Drop(Place::from(local)), node);
        }
    }

    /// Before leaving the function at `node`: drops the owning temporaries of every statement
    /// being lowered, then every variable and `Owned` parameter that owns memory, the last
    /// declared first. Borrowed parameters belong to the caller.
    pub(crate) fn drop_all(&mut self, node: NodeId) {
        let temporaries: Vec<Local> = self.temporaries.iter().flatten().rev().copied().collect();
        for local in temporaries {
            self.push(StatementKind::Drop(Place::from(local)), node);
        }
        for local in self.owners() {
            self.push(StatementKind::Drop(Place::from(local)), node);
        }
    }

    /// Every variable and `Owned` parameter that owns memory, the last declared first.
    fn owners(&self) -> Vec<Local> {
        (1..self.body.locals.len())
            .rev()
            .map(Local::new)
            .filter(|&local| {
                let declaration = &self.body.locals[local.index()];
                let owner = match &declaration.kind {
                    LocalKind::Parameter { mode, .. } => *mode == Mode::Owned,
                    LocalKind::Variable { .. } => true,
                    LocalKind::Return | LocalKind::Temporary => false,
                };
                owner && self.needs_drop(declaration.ty)
            })
            .collect()
    }

    /// The variable local whose name `node` binds or reads, which an assignment target names.
    pub(crate) fn target_variable(&self, node: NodeId) -> Option<Local> {
        (self.tree.kind(node) == SyntaxKind::Name)
            .then(|| self.variable(node))
            .flatten()
    }

    /// The source text of `node`.
    pub(crate) fn text_of(&self, node: NodeId) -> &'db str {
        self.tree.text(node, self.text)
    }
}
