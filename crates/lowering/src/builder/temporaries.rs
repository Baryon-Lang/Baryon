//! The temporaries of the function being lowered: making them, and dropping each one that owns
//! memory when its statement ends, right after the call that consumes it, or on an early exit.
//! A child of `builder` so it can reach the builder's private state.

use super::Builder;
use mir::{Local, LocalKind, Place, StatementKind};
use syntax::NodeId;
use types::{Mode, Type};

impl<'db> Builder<'db> {
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

    /// Drops the owning temporary `local` now, for `node`, rather than when its statement ends.
    /// A whole-array sum drops its temporary right after the call that sums it, so array
    /// expansion finds the three statements it fuses together, even when the sum is an argument
    /// of a longer statement, `print((a * b).sum())`.
    pub(crate) fn drop_now(&mut self, local: Local, node: NodeId) {
        for scope in &mut self.temporaries {
            scope.retain(|&temporary| temporary != local);
        }
        self.push(StatementKind::Drop(Place::from(local)), node);
    }

    /// Ends what [`enter`](Self::enter) started without dropping its temporaries, which the
    /// code that follows moves: what a temporary needs while calls that may fail run, and no
    /// longer after (the `managers` module).
    pub(crate) fn forget(&mut self) {
        self.temporaries.pop();
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
}
