//! Arrays a `prange` loop lists in `accumulate=` (decisions 0013 and 0024).
//!
//! `for i in prange(n, accumulate=forces):` passes `forces` to the outlined body as an `InOut`
//! parameter listed in the loop's [`accumulations`](mir::TerminatorKind::ParallelFor), after
//! the reductions. Inside the body, `forces[j] += f` and `forces[j] -= f` lower to the usual
//! bounds checks on the index, then one [`Accumulate`](mir::StatementKind::Accumulate)
//! statement: the backend adds `f` into the running chunk's private copy of the element, and the
//! copies combine into the array after the loop.
//!
//! A field of a variable, `accumulate=atoms.forces`, works the same way: the body still reaches
//! the struct through its own parameter, for every other field and for the array's lengths, and
//! an add whose target starts with `atoms.forces` goes to the accumulated parameter instead.
//!
//! The checker already allows nothing else on an accumulated array inside the loop: no read, no
//! `=`, no call, and it lists only variables and their fields. Not lowered yet: a slice or a
//! whole array as the target of the add.

use crate::builder::Builder;
use crate::parallel::Passed;
use database::Definition;
use mir::{AccumulateOperator, BinaryOperator, Local, Place, Projection, StatementKind};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Mode, Type, TypeKind};

/// One array a `prange` loop lists in `accumulate=`: a variable, `forces`, or a field of one,
/// `atoms.forces`.
pub(crate) struct Listed<'db> {
    /// The binding of the variable the entry starts from: `forces` or `atoms`.
    pub(crate) binding: NodeId,
    /// The field names after the variable, outermost first: empty for `forces`, `["forces"]`
    /// for `atoms.forces`.
    pub(crate) fields: Vec<&'db str>,
    /// The array, in the loop's own body.
    pub(crate) place: Place,
    /// The array's type.
    pub(crate) ty: Type<'db>,
    /// The entry as written, which names the outlined body's parameter.
    pub(crate) name: String,
}

impl<'db> Builder<'db> {
    /// The arrays the `prange` call `iterator` lists in `accumulate=`, in the order listed.
    /// Empty without the argument; `None` after reporting an entry lowering does not handle.
    pub(crate) fn accumulated_arrays(&mut self, iterator: NodeId) -> Option<Vec<Listed<'db>>> {
        let mut arrays = Vec::new();
        for value in self.keyword_values(iterator) {
            let entries: Vec<NodeId> = if self.tree.kind(value) == SyntaxKind::Tuple {
                self.tree.children_with(value, Role::Item).collect()
            } else {
                vec![value]
            };
            for entry in entries {
                let Some((binding, fields)) = self.field_path(entry) else {
                    // The checker lists only variables and their fields.
                    self.unsupported(entry, "this `accumulate=` entry");
                    return None;
                };
                let place = self.place(entry)?;
                arrays.push(Listed {
                    binding,
                    fields,
                    place,
                    ty: self.ty(entry),
                    name: self.text_of(entry).to_owned(),
                });
            }
        }
        Some(arrays)
    }

    /// The variable, by binding, and the field names that the expression `node` reaches,
    /// such as `atoms` and `["forces"]` for `atoms.forces`. `None` when `node` is anything else
    /// than a variable or fields of one. Reports nothing.
    pub(crate) fn field_path(&self, node: NodeId) -> Option<(NodeId, Vec<&'db str>)> {
        match self.tree.kind(node) {
            SyntaxKind::Name => match self.definition(node)? {
                &Definition::Local(binding) => Some((binding, Vec::new())),
                _ => None,
            },
            SyntaxKind::Attribute => {
                let (binding, mut fields) = self.field_path(self.tree.child(node, Role::Value)?)?;
                fields.push(self.text_of(self.tree.child(node, Role::Attribute)?));
                Some((binding, fields))
            }
            _ => None,
        }
    }

    /// The place in the running chunk's copy that `place`, the target of an in-place
    /// operator in an outlined body, adds into: `place` moved onto the accumulated parameter
    /// when it starts with the path of an array the loop lists in `accumulate=`, and `None`
    /// otherwise.
    pub(crate) fn accumulated_place(&self, place: &Place) -> Option<Place> {
        self.accumulated.iter().find_map(|(prefix, parameter)| {
            let rest = place
                .projections
                .strip_prefix(prefix.projections.as_slice())?;
            (place.local == prefix.local).then(|| Place {
                local: *parameter,
                projections: rest.to_vec(),
            })
        })
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

/// Adds one `InOut` parameter per array in `accumulated` to `passed`, the outlined body's
/// parameters so far, and returns the arrays' places in the loop's body, for
/// [`accumulations`](mir::TerminatorKind::ParallelFor), with the outlined body's
/// [`accumulated`](Builder::accumulated) entries. A variable's parameter stands for the variable
/// in the body; a field's root struct keeps its own parameter, and an add into the field moves
/// onto the new one.
pub(crate) fn pass_accumulated<'db>(
    accumulated: Vec<Listed<'db>>,
    passed: &mut Vec<Passed<'db>>,
) -> (Vec<Place>, Vec<(Place, Local)>) {
    let mut accumulations = Vec::new();
    let mut targets = Vec::new();
    for listed in accumulated {
        let parameter = Local::new(passed.len() + 1);
        let prefix = if listed.fields.is_empty() {
            passed.push((listed.name, listed.ty, Mode::InOut, Some(listed.binding)));
            Place::from(parameter)
        } else {
            passed.push((listed.name, listed.ty, Mode::InOut, None));
            // The body reaches the struct through its own parameter; an add into the
            // field moves onto `parameter`.
            let Some(root) = passed
                .iter()
                .position(|passed| passed.3 == Some(listed.binding))
            else {
                accumulations.push(listed.place);
                continue;
            };
            let fields = listed
                .place
                .projections
                .iter()
                .skip(listed.place.projections.len() - listed.fields.len())
                .cloned()
                .collect();
            Place {
                local: Local::new(root + 1),
                projections: fields,
            }
        };
        targets.push((prefix, parameter));
        accumulations.push(listed.place);
    }
    (accumulations, targets)
}
