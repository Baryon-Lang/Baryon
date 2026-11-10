//! List comprehensions (language reference, "Lists"): `[value for target in iterator if
//! condition]`, with any number of `for` clauses, each with any number of `if` conditions.
//!
//! A comprehension is the loops it reads as. `[x * x for x in xs if x > 0]` starts an empty list,
//! loops over `xs` as a `for` statement does, skips a pass whose condition is false, and appends
//! each value. A second `for` clause is a loop inside the first, as in Python, so
//! `[(i, j) for i in range(n) for j in range(i)]` makes the pairs with `j < i`, row by row.
//!
//! Each clause loops the way the `loops` module lowers a `for` statement: over `range`, an array,
//! a list of a copy type or a tuple. `prange` and `simd` are statements' loops, so a comprehension
//! over one is reported as not lowered. The temporaries a pass makes are dropped when the pass
//! ends, so a pass that builds text or a list frees it before the next.

use crate::builder::Builder;
use crate::loops::Pass;
use mir::{Argument, Builtin, Callee, Place, Rvalue, TerminatorKind};
use syntax::{NodeId, Role};
use types::{Type, TypeKind};

impl<'db> Builder<'db> {
    /// The list comprehension `node` into `place`: an empty list, then one loop per `for`
    /// clause, outermost first, that appends the value on every pass whose conditions hold.
    ///
    /// Cost: one pass of the loops, and the appends' amortized growth of the list's buffer.
    pub(crate) fn list_comprehension(&mut self, place: &Place, node: NodeId) {
        let ty = self.ty(node);
        if self.list_element(ty, node).is_none() {
            return;
        }
        self.assign(place.clone(), Rvalue::Aggregate(ty, Vec::new()), node);
        let clauses: Vec<NodeId> = self.tree.children_with(node, Role::Item).collect();
        self.clauses(place, ty, &clauses, node);
    }

    /// The loops of `clauses`, the `for` clauses of the comprehension `node` from the current
    /// one in, appending to the list at `place`, of type `ty`, inside the innermost.
    fn clauses(&mut self, place: &Place, ty: Type<'db>, clauses: &[NodeId], node: NodeId) {
        let Some((&clause, rest)) = clauses.split_first() else {
            return self.append_value(place, ty, node);
        };
        let (Some(target), Some(iterator)) = (
            self.tree.child(clause, Role::Target),
            self.tree.child(clause, Role::Iterator),
        ) else {
            return;
        };
        if self.is_prange(iterator) || self.simd_range(iterator).is_some() {
            return self.unsupported(iterator, "`prange` and `simd` in a comprehension");
        }
        let pass: Pass<'_, 'db> = &mut |builder, next, _end| {
            let conditions: Vec<NodeId> = builder
                .tree
                .children_with(clause, Role::Condition)
                .collect();
            for condition in conditions {
                let holds = builder.condition(condition);
                let passed = builder.new_block();
                builder.branch(holds, (passed, next), condition);
                builder.start(passed);
            }
            builder.enter();
            builder.clauses(place, ty, rest, node);
            builder.leave(clause);
            builder.jump(&mut Some(next), clause);
        };
        if self.is_range(iterator) {
            self.range_loop(clause, target, iterator, pass);
        } else {
            self.collection_loop(clause, target, iterator, pass);
        }
    }

    /// Appends the value of the comprehension `node`, converted to the element type of `ty`, to
    /// the list at `place`: [`Builtin::Append`] with the list `InOut` and the value `Owned`.
    fn append_value(&mut self, place: &Place, ty: Type<'db>, node: NodeId) {
        let (Some(value), &TypeKind::List(element)) =
            (self.tree.child(node, Role::Value), ty.kind(self.db))
        else {
            return;
        };
        let value = self.operand_as(value, element);
        let nothing = self.temporary(Type::new(self.db, TypeKind::None), node);
        self.then(node, |next| TerminatorKind::Call {
            callee: Callee::Builtin(Builtin::Append),
            arguments: vec![Argument::InOut(place.clone()), Argument::Owned(value)],
            destination: Place::from(nothing),
            target: next,
            error: None,
        });
    }
}
