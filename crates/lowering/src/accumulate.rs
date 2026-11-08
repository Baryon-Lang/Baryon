//! Arrays a `prange` loop lists in `accumulate=` (decisions 0013 and 0024).
//!
//! `for i in prange(n, accumulate=forces):` passes `forces` to the outlined body as an `InOut`
//! parameter listed in the loop's [`accumulations`](mir::TerminatorKind::ParallelFor), after
//! the reductions. Inside the body, `forces[j] += f` and `forces[j] -= f` lower to the usual
//! bounds checks on the index, then one [`Accumulate`](mir::StatementKind::Accumulate)
//! statement: the backend adds `f` into the running chunk's private copy of the element, and the
//! copies combine into the array after the loop.
//!
//! The checker already allows nothing else on an accumulated array inside the loop: no read, no
//! `=`, no call. Not lowered yet: an array reached through a field, `accumulate=atoms.forces`, and
//! a slice or a whole array as the target of the add.

use crate::builder::Builder;
use database::Definition;
use mir::{AccumulateOperator, BinaryOperator, Local, Place, Projection, StatementKind};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Type, TypeKind};

impl<'db> Builder<'db> {
    /// The arrays the `prange` call `iterator` lists in `accumulate=`, by the binding of each
    /// variable and its local, in the order listed. Empty without the argument; `None` after
    /// reporting an entry lowering does not handle.
    pub(crate) fn accumulated_arrays(&mut self, iterator: NodeId) -> Option<Vec<(NodeId, Local)>> {
        let mut arrays = Vec::new();
        for value in self.keyword_values(iterator) {
            let entries: Vec<NodeId> = if self.tree.kind(value) == SyntaxKind::Tuple {
                self.tree.children_with(value, Role::Item).collect()
            } else {
                vec![value]
            };
            for entry in entries {
                let binding = match self.definition(entry) {
                    Some(&Definition::Local(binding))
                        if self.tree.kind(entry) == SyntaxKind::Name =>
                    {
                        binding
                    }
                    _ => {
                        self.unsupported(entry, "`accumulate=` of an array that is not a variable");
                        return None;
                    }
                };
                let local = self.variable(entry)?;
                arrays.push((binding, local));
            }
        }
        Some(arrays)
    }

    /// The values of the keyword arguments of the call `call`, such as `forces` in
    /// `prange(n, accumulate=forces)`, in order.
    pub(crate) fn keyword_values(&self, call: NodeId) -> Vec<NodeId> {
        self.tree
            .child(call, Role::Arguments)
            .into_iter()
            .flat_map(|list| self.tree.children_with(list, Role::Item))
            .filter(|&item| self.tree.kind(item) == SyntaxKind::Keyword)
            .filter_map(|item| self.tree.child(item, Role::Value))
            .collect()
    }

    /// `target op= value` where `place`, the target, is an element of an accumulated array:
    /// adds the value into the running chunk's copy with an `Accumulate` statement. `place`
    /// already holds the index's bounds checks; `ty` is the element's type.
    pub(crate) fn accumulate(
        &mut self,
        place: Place,
        operator: BinaryOperator,
        ty: Type<'db>,
        value: NodeId,
        statement: NodeId,
    ) {
        let operator = match operator {
            BinaryOperator::Add => AccumulateOperator::Add,
            BinaryOperator::Subtract => AccumulateOperator::Subtract,
            _ => {
                return self
                    .unsupported(statement, "this in-place operator on an accumulated array");
            }
        };
        let indexed = place
            .projections
            .iter()
            .any(|projection| matches!(projection, Projection::Index(_)));
        if !indexed || matches!(ty.kind(self.db), TypeKind::Array { .. }) {
            return self.unsupported(
                statement,
                "adding into a slice or a whole accumulated array",
            );
        }
        let value = self.operand_as(value, ty);
        self.push(
            StatementKind::Accumulate {
                place,
                operator,
                value,
            },
            statement,
        );
    }
}
