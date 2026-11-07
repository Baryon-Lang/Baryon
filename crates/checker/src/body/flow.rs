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
//!
//! The same walk tracks which variables are assigned on every path (the `assignment` module):
//! after an `if`, those every branch that falls through assigns; after a loop, only those
//! assigned before it, as the body may run zero times. For each variable some path skips, it
//! remembers where that path starts, so the error can point at it.

use super::Body;
use std::collections::{BTreeMap, BTreeSet};
use syntax::{NodeId, Operator, Role, SyntaxKind};
use types::{Type, TypeKind};

/// What is known about variables at one point in a function body. Variables are named by the
/// `Name` node of their first binding, which the resolver gives every use.
#[derive(Clone, Debug, Default)]
pub(super) struct State<'db> {
    /// The `T | None` variables known to hold a `T`, with that `T`.
    pub(super) narrowed: BTreeMap<NodeId, Type<'db>>,
    /// The variables assigned on every path that reaches this point.
    pub(super) assigned: BTreeSet<NodeId>,
    /// For a variable some path to this point does not assign, where that path starts.
    pub(super) skipped: BTreeMap<NodeId, Skip>,
    /// The variables moved on some path to this point, each with where it moved (the `moves`
    /// module).
    pub(super) moved: BTreeMap<NodeId, NodeId>,
}

/// Where a path that skips an assignment starts, and what to say about it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Skip {
    /// The node to point at: a condition, a clause, a pattern or a loop's header.
    pub(super) node: NodeId,
    /// The label at that node, such as "not assigned when this is false".
    pub(super) label: &'static str,
}

/// One path that falls through to the end of a branching statement: what is known at its end,
/// and where it starts.
pub(super) struct End<'db> {
    /// What is known at the end of the path.
    pub(super) state: State<'db>,
    /// Where the path starts, to name it when it skips an assignment.
    pub(super) skip: Skip,
}

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
        self.state.narrowed.extend(facts.iter().copied());
    }

    /// Continues after a branching statement whose paths that fall through end as `ends`, and
    /// returns whether there are any. What is known is what they all agree on; a variable
    /// that some of them assign is remembered as skipped by the others. With no path falling
    /// through, what follows cannot run, and `entry` is kept.
    pub(super) fn merge(&mut self, entry: State<'db>, ends: &[End<'db>]) -> bool {
        let Some(first) = ends.first() else {
            self.state = entry;
            return false;
        };
        let mut joined = first.state.clone();
        for end in &ends[1..] {
            joined
                .narrowed
                .retain(|binding, ty| end.state.narrowed.get(binding) == Some(ty));
            joined
                .assigned
                .retain(|binding| end.state.assigned.contains(binding));
        }
        for end in ends {
            for (&binding, &skip) in &end.state.skipped {
                joined.skipped.entry(binding).or_insert(skip);
            }
            for (&binding, &site) in &end.state.moved {
                joined.moved.entry(binding).or_insert(site);
            }
        }
        for end in ends {
            for other in ends {
                for &binding in &other.state.assigned {
                    if !end.state.assigned.contains(&binding) {
                        joined.skipped.entry(binding).or_insert(end.skip);
                    }
                }
            }
        }
        self.state = joined;
        true
    }

    /// Updates what is known about the variable `binding`, declared with the type `held`, after
    /// it is assigned a value of type `ty`.
    pub(super) fn assigned(&mut self, binding: NodeId, held: Type<'db>, ty: Type<'db>) {
        self.state.narrowed.remove(&binding);
        if let TypeKind::Optional(value) = held.kind(self.db)
            && ty == *value
        {
            self.state.narrowed.insert(binding, *value);
        }
    }

    /// Forgets what is known about every variable assigned anywhere inside `node`.
    pub(super) fn forget_assigned(&mut self, node: NodeId) {
        if self.state.narrowed.is_empty() {
            return;
        }
        let role = self.tree.node(node).role();
        if self.tree.kind(node) == SyntaxKind::Name
            && matches!(role, Role::Target | Role::Name | Role::Item)
            && let Some(&database::Definition::Local(binding)) = self.lowerer.definition(node)
        {
            self.state.narrowed.remove(&binding);
        }
        let children: Vec<NodeId> = self.tree.children(node).collect();
        for child in children {
            self.forget_assigned(child);
        }
    }

    /// `if`, `elif` and `else`: each branch starts from what its condition, and the conditions
    /// before it being false, tell. Returns whether any branch falls through.
    pub(super) fn if_statement(&mut self, statement: NodeId) -> bool {
        let entry = self.state.clone();
        let mut branches = vec![(self.tree.child(statement, Role::Condition), statement)];
        branches.extend(
            self.tree
                .children_with(statement, Role::Else)
                .map(|clause| (self.tree.child(clause, Role::Condition), clause)),
        );
        let mut ends = Vec::new();
        let mut last_condition = None;
        for (condition, parent) in branches {
            let (yes, no, skip) = if let Some(condition) = condition {
                self.condition(Some(condition));
                last_condition = Some(condition);
                let (yes, no) = self.facts(condition);
                let label = "not assigned when this is true";
                (
                    yes,
                    no,
                    Skip {
                        node: condition,
                        label,
                    },
                )
            } else {
                last_condition = None;
                let label = "not assigned in this `else` branch";
                (
                    Vec::new(),
                    Vec::new(),
                    Skip {
                        node: parent,
                        label,
                    },
                )
            };
            let before = self.state.clone();
            self.assume(&yes);
            if self.block(parent, Role::Body) {
                ends.push(End {
                    state: self.state.clone(),
                    skip,
                });
            }
            self.state = before;
            self.assume(&no);
        }
        if let Some(condition) = last_condition {
            let label = "not assigned when this is false";
            ends.push(End {
                state: self.state.clone(),
                skip: Skip {
                    node: condition,
                    label,
                },
            });
        }
        self.merge(entry, &ends)
    }

    /// `while condition:` and its `else`. The body runs with the condition's facts. Returns
    /// whether the loop can end: `while True:` ends only through a `break`.
    pub(super) fn while_statement(&mut self, statement: NodeId) -> bool {
        let condition = self.tree.child(statement, Role::Condition);
        let header = condition.unwrap_or(statement);
        self.repeated(statement, header, |body| {
            body.condition(condition);
            let (yes, _) = condition
                .map(|condition| body.facts(condition))
                .unwrap_or_default();
            let entry = body.state.clone();
            body.assume(&yes);
            body.block(statement, Role::Body);
            body.state = entry;
            body.block(statement, Role::Else);
        });
        let forever =
            condition.is_some_and(|condition| self.tree.kind(condition) == SyntaxKind::True);
        !forever || self.breaks(statement)
    }

    /// Whether a `break` inside `node` ends the loop `node`, not a loop nested in it.
    fn breaks(&self, node: NodeId) -> bool {
        self.tree
            .children(node)
            .any(|child| match self.tree.kind(child) {
                SyntaxKind::Break => true,
                SyntaxKind::For | SyntaxKind::While => false,
                kind if kind.is_expression() => false,
                _ => self.breaks(child),
            })
    }

    /// Checks a loop, whose body may run any number of times, including none, with `check`.
    /// Afterwards, what is known is what was known before it, less what it assigns: a variable
    /// first assigned in the loop is skipped by the path that runs the body zero times, which
    /// starts at `header`.
    pub(super) fn repeated(
        &mut self,
        statement: NodeId,
        header: NodeId,
        check: impl FnOnce(&mut Self),
    ) {
        self.forget_assigned(statement);
        let entry = self.state.clone();
        check(self);
        let inside = std::mem::replace(&mut self.state, entry);
        self.moved_in_loop(statement, &self.state.clone(), &inside);
        for (&binding, &site) in &inside.moved {
            self.state.moved.entry(binding).or_insert(site);
        }
        for binding in inside.assigned.difference(&self.state.assigned) {
            let label = "not assigned if the loop runs zero times";
            self.state.skipped.entry(*binding).or_insert(Skip {
                node: header,
                label,
            });
        }
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
