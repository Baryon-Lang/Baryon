//! `with` on a context manager (decision 0053): a value of a class that defines
//! `__enter__(self: Owned[Self]) -> Self` and `__exit__(self)`.
//!
//! `with Section("solve") as log: body` lowers to:
//!
//! 1. `log = Section("solve").__enter__()`, with `__enter__`'s error edge when it can fail;
//! 2. the body, with an [`Exit`] on [`Builder::exits`];
//! 3. `log.__exit__()` at the end of the body.
//!
//! Nothing unwinds (decision 0005), so every other way out of the body calls `__exit__` itself
//! before it jumps: `return` (after computing its value), `break` and `continue` (for the
//! blocks inside their loop), and an error (for the blocks inside the `try` that catches it,
//! or all of them when it leaves the function). Each runs the innermost block's `__exit__`
//! first. While one `__exit__` call is placed, the blocks and `try` statements inside its own
//! block are set aside, so an error from it goes where an error of the `with` statement goes,
//! and replaces the one in flight, as in Python.
//!
//! Without `as`, a hidden temporary holds the value; when `__exit__` borrows it, the temporary
//! is dropped right after the call, on every path.

use crate::builder::Builder;
use crate::frames::erase;
use mir::{Argument, Local, Operand, Place, StatementKind, TerminatorKind};
use syntax::{NodeId, Role};
use types::{Declaration, Item, Mode, Signature, Type, TypeKind};

/// One `with` block of a context manager being lowered: the `__exit__` call each way out of it
/// places.
#[derive(Clone)]
pub(crate) struct Exit<'db> {
    /// The variable after `as`, or the hidden temporary, that holds the value.
    holder: Local,
    /// `__exit__`.
    method: Item<'db>,
    /// The type arguments of the value's class, for a generic class.
    type_arguments: Vec<Type<'db>>,
    /// How `__exit__` takes `self`.
    mode: Mode,
    /// The errors `__exit__` can fail with, in its `Raises` order.
    errors: Vec<Type<'db>>,
    /// Whether the holder is dropped after the call: a hidden temporary that `__exit__` only
    /// borrows.
    drop_after: bool,
    /// How many loops enclosed the `with` statement: `break` and `continue` leave the blocks
    /// entered since.
    loops: usize,
    /// How many `try` statements enclosed the `with` statement: an error caught by one of them
    /// leaves the blocks entered since, and an error from `__exit__` goes to one of them.
    handlers: usize,
    /// The `with` item, for the call's source position.
    node: NodeId,
}

impl<'db> Builder<'db> {
    /// The `with` statement `statement` from its item `items[0]` on, when the checker recorded
    /// a call of `__enter__` for that item: enters it, lowers the rest of the items and the
    /// body inside it, and exits it. Returns `false`, doing nothing, for any other item.
    pub(crate) fn with_manager(&mut self, statement: NodeId, items: &[NodeId]) -> bool {
        let Some((&item, rest)) = items.split_first() else {
            return false;
        };
        if !matches!(
            self.typed.call(item).map(|call| &call.callee),
            Some(types::Callee::Method(_))
        ) {
            return false;
        }
        let Some(value) = self.tree.child(item, Role::Value) else {
            return true;
        };
        let ty = self.ty(value);
        let Some(exit) = self.exit_method(ty) else {
            self.unsupported(item, "this context manager");
            return true;
        };
        let entered = self.temporary(ty, item);
        self.enter();
        self.call(Place::from(entered), item);
        self.leave(item);
        let target = self.tree.child(item, Role::Target);
        let holder = match target {
            Some(target) => {
                let Some(variable) = self.target_variable(target) else {
                    self.unsupported(target, "this `with` target");
                    return true;
                };
                let moved = Operand::Move(Place::from(entered));
                if self.needs_drop(ty) {
                    self.replace(Place::from(variable), moved, target);
                } else {
                    self.assign(Place::from(variable), mir::Rvalue::Use(moved), target);
                }
                variable
            }
            None => entered,
        };
        let (method, mode, errors) = exit;
        let type_arguments = match ty.kind(self.db) {
            TypeKind::Struct(nominal) => nominal
                .arguments
                .iter()
                .map(|&argument| erase(self.db, argument))
                .collect(),
            _ => Vec::new(),
        };
        let exit = Exit {
            holder,
            method,
            type_arguments,
            mode,
            errors,
            drop_after: target.is_none() && mode != Mode::Owned && self.needs_drop(ty),
            loops: self.loops.len(),
            handlers: self.handlers.len(),
            node: item,
        };
        self.exits.push(exit);
        if !self.with_manager(statement, rest) {
            match rest {
                [] => self.block(statement, Role::Body),
                _ => self.unsupported(rest[0], "a file in the same `with` as a context manager"),
            }
        }
        if let Some(exit) = self.exits.pop()
            && !self.is_dead()
        {
            self.exit_call(&exit);
        }
        true
    }

    /// The method `name` the class of `ty` defines itself, and its signature. `None` for any
    /// other type, or when the class does not define it.
    pub(crate) fn dunder(&self, ty: Type<'db>, name: &str) -> Option<(Item<'db>, Signature<'db>)> {
        let TypeKind::Struct(nominal) = ty.kind(self.db) else {
            return None;
        };
        let class = Item::new(self.db, self.package, nominal.file, nominal.name);
        let Declaration::Struct { methods, .. } = &self.db.declaration(class).declaration else {
            return None;
        };
        let tree = self.db.parse(nominal.file).tree();
        let text = nominal.file.source(self.db).text();
        let method = *methods
            .iter()
            .find(|method| tree.text(method.name(self.db), text) == name)?;
        let Declaration::Function(signature) = &self.db.declaration(method).declaration else {
            return None;
        };
        Some((method, signature.clone()))
    }

    /// `__exit__` of the class of `ty`, how it takes `self`, and the errors it can fail with.
    /// `None` when the class has no `__exit__`, which the checker reported.
    fn exit_method(&self, ty: Type<'db>) -> Option<(Item<'db>, Mode, Vec<Type<'db>>)> {
        let (method, signature) = self.dunder(ty, "__exit__")?;
        let mode = signature.parameters.first()?.mode;
        let errors = match signature.returns.kind(self.db) {
            TypeKind::Fallible { errors, .. } => errors.clone(),
            _ => Vec::new(),
        };
        Some((method, mode, errors))
    }

    /// Calls `__exit__` for `exit`, whose block is no longer on [`exits`](Self::exits): the
    /// blocks and `try` statements inside it are set aside, so its error goes where an error
    /// of its `with` statement goes.
    fn exit_call(&mut self, exit: &Exit<'db>) {
        let handlers = self
            .handlers
            .split_off(exit.handlers.min(self.handlers.len()));
        let node = exit.node;
        let holder = Place::from(exit.holder);
        let argument = match exit.mode {
            Mode::Borrow => Argument::Borrow(holder.clone()),
            Mode::InOut => Argument::InOut(holder.clone()),
            Mode::Owned => Argument::Owned(Operand::Move(holder.clone())),
        };
        let nothing = Type::new(self.db, TypeKind::None);
        let result = self.temporary(nothing, node);
        let edge = self.error_edge(&exit.errors, node);
        let next = self.new_block();
        let mut call_edge = edge.clone();
        if exit.drop_after
            && let Some(edge) = call_edge.as_mut()
        {
            // The hidden value is dropped whether `__exit__` returns or fails.
            let dispatch = self.new_block();
            self.start(edge.target);
            self.push(StatementKind::Drop(holder.clone()), node);
            self.terminate(TerminatorKind::Goto(dispatch), node);
            edge.target = dispatch;
        }
        self.terminate(
            TerminatorKind::Call {
                callee: mir::Callee::Function {
                    item: exit.method,
                    type_arguments: exit.type_arguments.clone(),
                },
                arguments: vec![argument],
                destination: Place::from(result),
                target: next,
                error: call_edge.clone(),
            },
            node,
        );
        if let Some(edge) = call_edge {
            self.dispatch(&edge, &exit.errors, node);
        }
        self.start(next);
        if exit.drop_after {
            self.push(StatementKind::Drop(holder), node);
        }
        self.handlers.extend(handlers);
    }

    /// Calls `__exit__` for each block on [`exits`](Self::exits) from position `from` on,
    /// innermost first, before a jump out of them. Each call sees only the blocks outside its
    /// own, so an error from it runs those and no others. The blocks stay on the list
    /// afterwards: the code after the jump cannot be reached, and each block still ends where
    /// it was entered.
    pub(crate) fn exit_from(&mut self, from: usize) {
        let mut left = Vec::new();
        while self.exits.len() > from {
            let Some(exit) = self.exits.pop() else {
                break;
            };
            self.exit_call(&exit);
            left.push(exit);
        }
        self.exits.extend(left.into_iter().rev());
    }

    /// `return value` inside a `with` block: computes the value first, as Python does, into a
    /// temporary an error from `__exit__` drops, then calls `__exit__` for every block, then
    /// moves the value into the return place. The caller drops the rest and returns.
    pub(crate) fn return_through_exits(&mut self, value: NodeId, statement: NodeId) {
        let returns = self.returns;
        let held = self.temporary(returns, value);
        self.enter();
        self.write(Place::from(held), returns, value);
        self.drop_later(held);
        self.exit_from(0);
        self.forget();
        let moved = mir::Rvalue::Use(Operand::Move(Place::from(held)));
        self.assign(Place::from(Local::RETURN), moved, statement);
    }
}

impl Exit<'_> {
    /// How many loops enclosed the `with` statement.
    pub(crate) fn loops(&self) -> usize {
        self.loops
    }

    /// How many `try` statements enclosed the `with` statement.
    pub(crate) fn handlers(&self) -> usize {
        self.handlers
    }
}
