//! `with` statements (language reference, "Statements", "`with`"; decisions 0036 and 0053).
//!
//! A `with` item is one of three things:
//!
//! - `unsafe`, which allows unchecked indexing in the block;
//! - a file, a `NetCDF` `Dataset` or an `h5by` `File` (the `files` module);
//! - a value of a class that defines `__enter__` and `__exit__`, a context manager
//!   (decision 0053). `__enter__(self: Owned[Self]) -> Self` takes the value and gives it back;
//!   the name after `as` holds it for the block, and lowering calls `__exit__(self)` on it on
//!   every way out of the block.
//!
//! `__exit__` must find its value where `__enter__` left it, so the name after `as` cannot be
//! moved or assigned in the block:
//!
//! ```text
//! error: `log` is held by a `with` block, so it cannot be moved
//!  --> main.byn:3:16
//!   |
//! 3 |         keep = log
//!   |                ^^^ moved here
//!  ::: main.byn:2:31
//!   |
//! 2 |     with Section("solve") as log:
//!   |                              --- `__exit__` is called on `log` when the block ends
//!   = note: keep a copy instead: `log.copy()`
//! ```

use super::Body;
use super::calls::Arguments;
use super::exclusive::Passed;
use database::Definition;
use diagnostics::Diagnostic;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Callee, Item, Key, Mode, Signature, Type, TypeKind};

/// A variable an enclosing `with` block holds for `__exit__`.
pub(super) struct Held {
    /// The variable's first binding, which the resolver gives every use of it.
    binding: NodeId,
    /// The name after `as` that binds it in the `with` statement.
    target: NodeId,
}

/// The two methods of a context manager's class, with the class's type arguments in their
/// signatures.
struct Manager<'db> {
    /// `__enter__` and its signature.
    enter: (Item<'db>, Signature<'db>),
    /// `__exit__` and its signature.
    exit: Signature<'db>,
}

impl<'db> Body<'db> {
    /// `with item, ...: body`: each item in turn, then the block. A context manager's variable
    /// is held for the block, and one whose `__exit__` takes `self` as `Owned` is moved after
    /// it. Returns whether control can reach the end of the statement.
    pub(super) fn with_statement(&mut self, statement: NodeId) -> bool {
        let items: Vec<NodeId> = self.tree.children_with(statement, Role::Item).collect();
        let held = self.held.len();
        let mut consumed = Vec::new();
        for item in items {
            let (Some(value), target) = (
                self.tree.child(item, Role::Value),
                self.tree.child(item, Role::Target),
            ) else {
                continue;
            };
            let ty = self.expression(value, None);
            match self.manager(value, ty) {
                Some(Ok(manager)) => {
                    consumed.extend(self.enter(item, (value, ty), target, &manager));
                    continue;
                }
                Some(Err(())) => continue,
                None => {}
            }
            let is_unsafe = self.tree.kind(value) == SyntaxKind::Name
                && self.tree.text(value, self.text) == "unsafe";
            match target {
                Some(target) => {
                    if let Some(ty) = self.with_file(statement, value, ty) {
                        self.bind(target, ty, Some(value));
                    }
                }
                None if is_unsafe => {}
                None => {
                    if self.with_file(statement, value, ty).is_some() {
                        self.lowerer.error(
                            value,
                            "a file in `with` needs a name",
                            "not named",
                            Some("name it to use it in the block: `with open(path) as f:`"),
                        );
                    }
                }
            }
        }
        let reachable = self.block(statement, Role::Body);
        self.held.truncate(held);
        for (binding, target) in consumed {
            self.state.moved.insert(binding, target);
            self.with_ends.insert(target);
        }
        reachable
    }

    /// The context manager methods of the class of `ty`, the type of the `with` item `value`,
    /// with their signatures checked against decision 0053. `None` when the class defines
    /// neither, so the item is something else; `Err` after reporting a class that defines one
    /// but not the other, or either with another signature.
    fn manager(&mut self, value: NodeId, ty: Type<'db>) -> Option<Result<Manager<'db>, ()>> {
        let (enter, exit) = (
            self.dunder_method(ty, "__enter__"),
            self.dunder_method(ty, "__exit__"),
        );
        if enter.is_none() && exit.is_none() {
            return None;
        }
        let shown = self.show(ty);
        let (Some(enter), Some((_, exit))) = (enter, exit) else {
            let message = format!("`{shown}` needs both `__enter__` and `__exit__` for `with`");
            self.lowerer.error(
                value,
                &message,
                "not a context manager",
                Some("define `def __enter__(self: Owned[Self]) -> Self` and `def __exit__(self)` (decision 0053)"),
            );
            return Some(Err(()));
        };
        let none = |ty: Type<'db>| matches!(ty.kind(self.db), TypeKind::None);
        let enter_returns = match enter.1.returns.kind(self.db) {
            TypeKind::Fallible { value, .. } => *value,
            _ => enter.1.returns,
        };
        if !matches!(enter.1.parameters.as_slice(), [only] if only.mode == Mode::Owned)
            || enter_returns != ty
        {
            let message = format!(
                "`{shown}.__enter__` must take `self: Owned[Self]` and return `Self` for `with`"
            );
            self.lowerer.error(
                value,
                &message,
                "used by this `with`",
                Some("the value moves into the block, and `__exit__` is called on the one `__enter__` returns: `return self` (decision 0053)"),
            );
            return Some(Err(()));
        }
        let exit_returns = match exit.returns.kind(self.db) {
            TypeKind::Fallible { value, .. } => *value,
            _ => exit.returns,
        };
        if exit.parameters.len() != 1 || !none(exit_returns) {
            let message =
                format!("`{shown}.__exit__` must take only `self` and return `None` for `with`");
            self.lowerer.error(
                value,
                &message,
                "used by this `with`",
                Some("`__exit__` cannot see or suppress an error; handle errors with `try` around the `with` (decision 0053)"),
            );
            return Some(Err(()));
        }
        Some(Ok(Manager { enter, exit }))
    }

    /// The `with` item `item` whose value `value`, of type `ty`, is a context manager: records
    /// the call of `__enter__`, which moves the value, binds `target` to what it returns,
    /// checks that the errors of `__enter__` and `__exit__` are handled, and holds `target` for
    /// the block. Returns the variable and its target when `__exit__` takes `self` as
    /// `Owned`, which moves it at the end of the block.
    fn enter(
        &mut self,
        item: NodeId,
        (value, ty): (NodeId, Type<'db>),
        target: Option<NodeId>,
        manager: &Manager<'db>,
    ) -> Option<(NodeId, NodeId)> {
        let (enter, signature) = &manager.enter;
        let receiver = Passed {
            node: value,
            mode: Mode::Owned,
            ty,
        };
        let arguments = Arguments {
            positional: Vec::new(),
            keywords: Vec::new(),
        };
        let returns = self.check_call(
            item,
            "__enter__",
            signature,
            Some(receiver),
            Callee::Method(*enter),
            &arguments,
        );
        if let TypeKind::Fallible { errors, .. } = returns.kind(self.db) {
            let errors = errors.clone();
            self.typed.errors.insert(Key(item), errors.clone());
            self.propagate(item, "`__enter__` can fail with", &errors);
        }
        if let TypeKind::Fallible { errors, .. } = manager.exit.returns.kind(self.db) {
            let errors = errors.clone();
            self.propagate(
                item,
                "`__exit__` at the end of this `with` can fail with",
                &errors,
            );
        }
        // A target that is not one name was reported by the syntax check.
        let target = target.filter(|&target| self.tree.kind(target) == SyntaxKind::Name)?;
        self.bind(target, ty, Some(value));
        let Some(&Definition::Local(binding)) = self.lowerer.definition(target) else {
            return None;
        };
        self.held.push(Held { binding, target });
        let owned = manager
            .exit
            .parameters
            .first()
            .is_some_and(|parameter| parameter.mode == Mode::Owned && self.moves(ty));
        owned.then_some((binding, target))
    }

    /// Reports `node`, a use that `verb` describes, "moved" or "assigned", when its variable is
    /// held by an enclosing `with` block for `__exit__`. Returns whether it was reported.
    pub(super) fn held_by_with(&mut self, node: NodeId, verb: &str) -> bool {
        let Some(root) = self.place_root(node) else {
            return false;
        };
        let Some(&Definition::Local(binding)) = self.lowerer.definition(root) else {
            return false;
        };
        let Some(target) = self
            .held
            .iter()
            .rev()
            .find(|held| held.binding == binding)
            .map(|held| held.target)
        else {
            return false;
        };
        let name = self.tree.text(root, self.text);
        let advice = if verb == "moved" {
            format!(
                "keep a copy instead: `{}.copy()`",
                self.tree.text(node, self.text)
            )
        } else {
            "give the new value another name".to_owned()
        };
        let diagnostic = Diagnostic::error(format!(
            "`{name}` is held by a `with` block, so it cannot be {verb}"
        ))
        .with_primary(self.tree.span(node), format!("{verb} here"))
        .with_secondary(
            self.tree.span(target),
            format!("`__exit__` is called on `{name}` when the block ends"),
        )
        .with_note(advice);
        self.lowerer.diagnostics.push(diagnostic);
        true
    }
}
