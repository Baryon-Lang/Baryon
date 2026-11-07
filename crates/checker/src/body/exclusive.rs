//! Exclusivity and read-only parameters (language reference, "Values and references", "The
//! exclusivity check" and "Parameters").
//!
//! While a call can change a value, no other argument of the same call may refer to it. That
//! guarantee is what lets the compiler tell LLVM that arrays never alias. A *place* is a
//! variable followed by `.field` and `[...]` steps, and two places overlap when they start at
//! the same variable and one's steps are a prefix of the other's, where any `[...]` matches any
//! other: indices are never compared. Each call is checked on its own, because a borrow ends
//! where its call ends:
//!
//! ```text
//! error: `atoms` is changed by this call and also passed to it
//!   --> md.npl:40:22
//!    |
//! 40 |     integrate(atoms, atoms.forces)
//!    |               -----  ^^^^^^^^^^^^ `atoms.forces` is part of `atoms`
//!    |               |
//!    |               `atoms` is passed as `InOut`
//! ```
//!
//! A read-only parameter, the default mode, belongs to the caller and is never changed: not
//! assigned, not the target of `+=`, not passed as `InOut`, and not changed by a method. One
//! of a copy type is the exception: it is passed by value, so the function holds its own copy,
//! and may change it without the caller seeing.

use super::Body;
use database::Definition;
use diagnostics::Diagnostic;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Mode, Type};

/// One argument of a call, as passed: the receiver of a method call counts as one.
pub(super) struct Passed<'db> {
    /// The argument's expression.
    pub(super) node: NodeId,
    /// How its parameter receives it.
    pub(super) mode: Mode,
    /// Its type, which says whether an `Owned` argument moves or is copied.
    pub(super) ty: Type<'db>,
}

/// One step of a place after its variable.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Step<'text> {
    /// `.name`.
    Field(&'text str),
    /// `[...]`, whatever the indices.
    Index,
}

impl<'db> Body<'db> {
    /// Applies what a call does to its arguments, `passed`, once they are all checked: reports
    /// any argument that overlaps one the call changes or moves, then marks each `InOut`
    /// argument as changed and each `Owned` one as moved. Moves come last, as they happen
    /// when the call starts, after every argument is read.
    pub(super) fn pass(&mut self, passed: &[Passed<'db>]) {
        for (index, first) in passed.iter().enumerate() {
            for second in &passed[index + 1..] {
                if self.claims(first) {
                    self.overlap(first, second);
                } else if self.claims(second) {
                    self.overlap(second, first);
                }
            }
        }
        for argument in passed {
            match argument.mode {
                Mode::InOut => self.change(argument.node),
                Mode::Owned => self.consume(argument.node, argument.ty),
                Mode::Borrow => {}
            }
        }
    }

    /// Whether the call changes or moves `argument`, so that no other argument may refer to
    /// it. An `Owned` copy type is copied, which leaves the caller's value alone.
    fn claims(&self, argument: &Passed<'db>) -> bool {
        match argument.mode {
            Mode::InOut => true,
            Mode::Owned => self.moves(argument.ty),
            Mode::Borrow => false,
        }
    }

    /// Reports `other` when its place overlaps that of `claimed`, which the call changes or
    /// moves.
    fn overlap(&mut self, claimed: &Passed<'db>, other: &Passed<'db>) {
        let (Some((_, steps)), Some((_, other_steps))) =
            (self.place(claimed.node), self.place(other.node))
        else {
            return;
        };
        if !self.overlaps(claimed.node, other.node) {
            return;
        }
        let name = self.tree.text(claimed.node, self.text);
        let other_name = self.tree.text(other.node, self.text);
        let (verb, mode) = if claimed.mode == Mode::InOut {
            ("changes", "InOut")
        } else {
            ("moves", "Owned")
        };
        let past = if claimed.mode == Mode::InOut {
            "changed"
        } else {
            "moved"
        };
        let label = match steps.len().cmp(&other_steps.len()) {
            std::cmp::Ordering::Less => format!("`{other_name}` is part of `{name}`"),
            std::cmp::Ordering::Greater => format!("`{other_name}` contains `{name}`"),
            std::cmp::Ordering::Equal if name == other_name => format!("`{name}` again"),
            std::cmp::Ordering::Equal => {
                format!("`{other_name}` may be the same as `{name}`: indices are not compared")
            }
        };
        let diagnostic =
            Diagnostic::error(format!("`{name}` is {past} by this call and also passed to it"))
                .with_primary(self.tree.span(other.node), label)
                .with_secondary(
                    self.tree.span(claimed.node),
                    format!("`{name}` is passed as `{mode}`"),
                )
                .with_note(if self.moves(other.ty) {
                    format!(
                        "a value this call {verb} cannot also be read through another argument; pass a copy: `{other_name}.copy()`"
                    )
                } else {
                    format!(
                        "a value this call {verb} cannot also be read through another argument; read it into a variable first: `value = {other_name}`"
                    )
                });
        self.lowerer.diagnostics.push(diagnostic);
    }

    /// Whether the places `first` and `second` overlap: they start at the same variable, and
    /// one's steps are a prefix of the other's. Not a place, such as `x + 1`, overlaps
    /// nothing.
    pub(super) fn overlaps(&self, first: NodeId, second: NodeId) -> bool {
        let (Some((root, steps)), Some((other_root, other_steps))) =
            (self.place(first), self.place(second))
        else {
            return false;
        };
        root == other_root && steps.iter().zip(&other_steps).all(|(a, b)| a == b)
    }

    /// The variable a place such as `atoms.forces[i]` starts at, and its steps; `None` when
    /// `node` is not a place, such as `x + 1`.
    fn place(&self, node: NodeId) -> Option<(NodeId, Vec<Step<'_>>)> {
        match self.tree.kind(node) {
            SyntaxKind::Name => match self.lowerer.definition(node) {
                Some(&Definition::Local(binding)) => Some((binding, Vec::new())),
                _ => None,
            },
            SyntaxKind::Attribute => {
                let (root, mut steps) = self.place(self.tree.child(node, Role::Value)?)?;
                let field = self.tree.child(node, Role::Attribute)?;
                steps.push(Step::Field(self.tree.text(field, self.text)));
                Some((root, steps))
            }
            SyntaxKind::Subscript => {
                let (root, mut steps) = self.place(self.tree.child(node, Role::Value)?)?;
                steps.push(Step::Index);
                Some((root, steps))
            }
            _ => None,
        }
    }

    /// Records that the place `node` is changed: an assignment target, the target of `+=`,
    /// an `InOut` argument or receiver. Reports it when an enclosing loop borrows it (the
    /// `loops` module), or when it is part of a read-only parameter.
    pub(super) fn change(&mut self, node: NodeId) {
        if let Some(parallel) = self.parallel.last_mut() {
            parallel.writes.insert(node);
        }
        let Some(root) = self.place_root(node) else {
            return;
        };
        if self.borrowed_by_loop(node, "changed") {
            return;
        }
        let Some(&Definition::Local(binding)) = self.lowerer.definition(root) else {
            return;
        };
        // A copy type is passed by value, so the function holds its own copy, and changing
        // it is invisible to the caller.
        let copied = self.locals.get(&binding).is_none_or(|&ty| !self.moves(ty));
        if self.modes.get(&binding) != Some(&Mode::Borrow) || copied {
            return;
        }
        let name = self.tree.text(root, self.text);
        let message = format!("`{name}` is a read-only parameter, so it cannot be changed");
        let hint = if name == "self" {
            "to change the struct, declare the method's first parameter `self: InOut[Self]`"
                .to_owned()
        } else {
            format!(
                "take it as `InOut[...]` to change the caller's value, or change a copy: `work = {name}.copy()`"
            )
        };
        self.lowerer
            .error(node, &message, "changed here", Some(&hint));
    }
}
