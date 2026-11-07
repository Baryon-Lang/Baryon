//! Control flow: which statements can fall through to the next one, and how a `None` check
//! narrows a `T | None` variable to `T` (language reference, "Types", "Optional values").
//!
//! A condition such as `x is not None` gives *facts*: what is known about variables when it is
//! true, and when it is false. A branch starts from the facts of the condition that leads to
//! it, and after an `if` the known types are those that every branch falling through agrees
//! on. So both of these narrow `distance` to `f64` for the rest of the function:
//!
//! ```python
//! if distance is None:
//!     return "unknown"
//! ```
//!
//! ```python
//! if distance is None:
//!     distance = 0.0
//! ```
//!
//! Assigning a `T` narrows a variable too, and assigning `None` or a `T | None` widens it again.
//! A loop body may run many times, so a variable assigned anywhere in a loop loses what was
//! known about it before the loop.

use super::Body;
use std::collections::BTreeMap;
use syntax::{NodeId, Operator, Role, SyntaxKind};
use types::{Type, TypeKind};

/// What is known about variables at one point: the narrowed type of each, by the `Name` node of
/// its first binding.
pub(super) type Narrowed<'db> = BTreeMap<NodeId, Type<'db>>;

/// Narrowings that hold when a condition is true, or when it is false.
type Facts<'db> = Vec<(NodeId, Type<'db>)>;

impl<'db> Body<'db> {
    /// What `condition` tells about variables when it is true, and when it is false.
    pub(super) fn facts(&self, condition: NodeId) -> (Facts<'db>, Facts<'db>) {
        match self.tree.kind(condition) {
            SyntaxKind::Compare => self.none_check(condition),
            SyntaxKind::UnaryOperation if self.tree.node(condition).operator() == Operator::Not => {
                let (yes, no) = self
                    .tree
                    .child(condition, Role::Value)
                    .map(|value| self.facts(value))
                    .unwrap_or_default();
                (no, yes)
            }
            SyntaxKind::BooleanOperation => {
                let and = self.tree.node(condition).operator() == Operator::And;
                let mut all = Vec::new();
                for item in self.tree.children_with(condition, Role::Item) {
                    let (yes, no) = self.facts(item);
                    all.extend(if and { yes } else { no });
                }
                if and {
                    (all, Vec::new())
                } else {
                    (Vec::new(), all)
                }
            }
            _ => (Vec::new(), Vec::new()),
        }
    }

    /// The facts of `x is None` or `x is not None`, where `x` is a `T | None` variable.
    fn none_check(&self, compare: NodeId) -> (Facts<'db>, Facts<'db>) {
        let nothing = (Vec::new(), Vec::new());
        let comparators: Vec<NodeId> = self.tree.children_with(compare, Role::Item).collect();
        let ([comparator], Some(left)) =
            (comparators.as_slice(), self.tree.child(compare, Role::Left))
        else {
            return nothing;
        };
        let operator = self.tree.node(*comparator).operator();
        let right = self.tree.child(*comparator, Role::Right);
        if !matches!(operator, Operator::Is | Operator::IsNot)
            || right.is_none_or(|right| self.tree.kind(right) != SyntaxKind::None)
            || self.tree.kind(left) != SyntaxKind::Name
        {
            return nothing;
        }
        let Some(&database::Definition::Local(binding)) = self.lowerer.definition(left) else {
            return nothing;
        };
        let Some(TypeKind::Optional(value)) = self.locals.get(&binding).map(|ty| ty.kind(self.db))
        else {
            return nothing;
        };
        let fact = vec![(binding, *value)];
        if operator == Operator::IsNot {
            (fact, Vec::new())
        } else {
            (Vec::new(), fact)
        }
    }

    /// Records `facts` as known from here on.
    pub(super) fn assume(&mut self, facts: &Facts<'db>) {
        self.narrowed.extend(facts.iter().copied());
    }

    /// What is known after branches that each fall through with one of `ends`: the narrowings
    /// they all agree on.
    fn join(ends: Vec<Narrowed<'db>>) -> Narrowed<'db> {
        let mut ends = ends.into_iter();
        let mut joined = ends.next().unwrap_or_default();
        for end in ends {
            joined.retain(|binding, ty| end.get(binding) == Some(ty));
        }
        joined
    }

    /// Updates what is known about the variable `binding`, declared with the type `held`, after
    /// it is assigned a value of type `ty`.
    pub(super) fn assigned(&mut self, binding: NodeId, held: Type<'db>, ty: Type<'db>) {
        self.narrowed.remove(&binding);
        if let TypeKind::Optional(value) = held.kind(self.db)
            && ty == *value
        {
            self.narrowed.insert(binding, *value);
        }
    }

    /// Forgets what is known about every variable assigned anywhere inside `node`.
    pub(super) fn forget_assigned(&mut self, node: NodeId) {
        if self.narrowed.is_empty() {
            return;
        }
        let role = self.tree.node(node).role();
        if self.tree.kind(node) == SyntaxKind::Name
            && matches!(role, Role::Target | Role::Name | Role::Item)
            && let Some(&database::Definition::Local(binding)) = self.lowerer.definition(node)
        {
            self.narrowed.remove(&binding);
        }
        let children: Vec<NodeId> = self.tree.children(node).collect();
        for child in children {
            self.forget_assigned(child);
        }
    }

    /// `if`, `elif` and `else`: each branch starts from what its condition, and the conditions
    /// before it being false, tell. Returns whether any branch falls through.
    pub(super) fn if_statement(&mut self, statement: NodeId) -> bool {
        let entry = self.narrowed.clone();
        let mut branches = vec![(self.tree.child(statement, Role::Condition), statement)];
        branches.extend(
            self.tree
                .children_with(statement, Role::Else)
                .map(|clause| (self.tree.child(clause, Role::Condition), clause)),
        );
        let mut ends = Vec::new();
        let mut otherwise = true;
        for (condition, parent) in branches {
            let (yes, no) = if let Some(condition) = condition {
                self.condition(Some(condition));
                self.facts(condition)
            } else {
                otherwise = false;
                (Vec::new(), Vec::new())
            };
            let before = self.narrowed.clone();
            self.assume(&yes);
            if self.block(parent, Role::Body) {
                ends.push(self.narrowed.clone());
            }
            self.narrowed = before;
            self.assume(&no);
        }
        if otherwise {
            ends.push(self.narrowed.clone());
        }
        let falls = !ends.is_empty();
        self.narrowed = if falls { Self::join(ends) } else { entry };
        falls
    }

    /// `while condition:` and its `else`. The body runs with the condition's facts.
    pub(super) fn while_statement(&mut self, statement: NodeId) {
        self.forget_assigned(statement);
        let entry = self.narrowed.clone();
        let condition = self.tree.child(statement, Role::Condition);
        self.condition(condition);
        let (yes, _) = condition
            .map(|condition| self.facts(condition))
            .unwrap_or_default();
        self.assume(&yes);
        self.block(statement, Role::Body);
        self.narrowed = entry.clone();
        self.block(statement, Role::Else);
        self.narrowed = entry;
    }

    /// Runs `check` on a loop, a `try` or another statement whose parts may run any number of
    /// times or stop part way, starting and ending with what is known before it, less the
    /// variables it assigns.
    pub(super) fn isolated<T>(
        &mut self,
        statement: NodeId,
        check: impl FnOnce(&mut Self) -> T,
    ) -> T {
        self.forget_assigned(statement);
        let entry = self.narrowed.clone();
        let result = check(self);
        self.narrowed = entry;
        result
    }

    /// Reports `ty`, the type of `node`, when it is a `T | None` used where only a `T` will do,
    /// and returns whether it was.
    pub(super) fn maybe_none(&mut self, node: NodeId, ty: Type<'db>) -> bool {
        if !matches!(ty.kind(self.db), TypeKind::Optional(_)) {
            return false;
        }
        let (message, hint) = if self.tree.kind(node) == SyntaxKind::Name {
            let name = self.tree.text(node, self.text);
            (
                format!("`{name}` may be `None`"),
                format!("check it first: `if {name} is not None:`"),
            )
        } else {
            (
                "this value may be `None`".to_owned(),
                "give it a name and check it first: `if value is not None:`".to_owned(),
            )
        };
        let label = format!("has type `{}`", self.show(ty));
        self.lowerer.error(node, &message, &label, Some(&hint));
        true
    }
}
