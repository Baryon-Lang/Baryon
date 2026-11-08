//! `for i in prange(start, stop, step):` (language reference, "Parallel and vector loops";
//! decision 0018). The loop body becomes its own body, [outlined](mir::Body::outlined), that runs
//! one iteration; the loop becomes a [`ParallelFor`](mir::TerminatorKind::ParallelFor) that runs
//! it once per iteration on the runtime's threads.
//!
//! The outlined body takes, in order: the iteration number, counted from 0; the loop's start, when
//! the loop does not start at 0 with step 1; each *shared* variable the body uses, one assigned
//! before the loop, borrowed when the body only reads it and `InOut` when it writes into it; and
//! each shared scalar the body reduces into, such as `total += x`. The loop variable is computed
//! from the iteration number as `start + iteration * step`. Variables first assigned in the body
//! are private: they are the outlined body's own, and are dropped when the iteration ends, so
//! `continue` returns.
//!
//! The checker proved the body safe to run this way (its `parallel` module): shared arrays are
//! written only at the loop variable's index, shared scalars only as reductions, and nothing
//! leaves the loop early. Lowering relies on that and does not check it again.
//!
//! Not lowered yet: `accumulate=` (roadmap 1.12.1), and the `min` and `max` reductions, which
//! wait for lowering of `min` and `max`.

use crate::builder::Builder;
use crate::operators::arithmetic;
use database::Definition;
use mir::{
    Argument, BinaryOperator, Constant, Local, Operand, Place, Reduction, ReductionOperator,
    Rvalue, TerminatorKind,
};
use std::collections::BTreeMap;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Integer, Mode, Primitive, Type, TypeKind};

/// How the body of a `prange` loop uses one shared variable.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Use {
    /// Only reads it.
    Read,
    /// Writes into it, at the loop variable's elements.
    Write,
    /// Reduces into it with this operator.
    Reduce(ReductionOperator),
}

/// One value passed to an outlined body: its name, type and mode there, and the variable it
/// stands for, by binding, when it is one.
pub(crate) type Passed<'db> = (String, Type<'db>, Mode, Option<NodeId>);

impl<'db> Builder<'db> {
    /// Whether `iterator` is a call to `prange`.
    pub(crate) fn is_prange(&self, iterator: NodeId) -> bool {
        self.tree.kind(iterator) == SyntaxKind::Call
            && self
                .tree
                .child(iterator, Role::Function)
                .is_some_and(|function| {
                    matches!(
                        self.definition(function),
                        Some(Definition::Builtin("prange"))
                    )
                })
    }

    /// Whether the call `node` passes a keyword argument, such as `accumulate=`.
    pub(crate) fn has_keyword(&self, node: NodeId) -> bool {
        self.tree
            .child(node, Role::Arguments)
            .into_iter()
            .flat_map(|list| self.tree.children_with(list, Role::Item))
            .any(|item| self.tree.kind(item) == SyntaxKind::Keyword)
    }

    /// The `prange` loop `statement` over `first..bound` by `step`, whose variable is
    /// `variable`: computes the iteration count, outlines the body, and runs it.
    pub(crate) fn parallel_for(
        &mut self,
        statement: NodeId,
        variable: Local,
        first: Option<NodeId>,
        bound: NodeId,
        step: i128,
    ) {
        let Some(iterator) = self.tree.child(statement, Role::Iterator) else {
            return;
        };
        let ty = self.local_type(variable);
        let i64 = Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::I64)),
        );
        if ty != i64 {
            return self.unsupported(iterator, "`prange` over integers other than `i64`");
        }
        let Some(shared) = self.shared(statement) else {
            return;
        };
        let start = self.temporary(ty, iterator);
        match first {
            Some(first) => self.write(Place::from(start), ty, first),
            None => self.assign(
                Place::from(start),
                Rvalue::Use(Operand::Constant(Constant::integer(ty, 0))),
                iterator,
            ),
        }
        let count = self.count(start, bound, step, iterator);

        let mut passed: Vec<Passed<'db>> = vec![("iteration".into(), i64, Mode::Owned, None)];
        let mut arguments = Vec::new();
        let offset = first.is_some() || step != 1;
        if offset {
            passed.push(("start".into(), ty, Mode::Borrow, None));
            arguments.push(Argument::Borrow(Place::from(start)));
        }
        let mut reductions = Vec::new();
        let mut reduced = Vec::new();
        for (&binding, &(local, usage)) in &shared {
            let name = self.local_name(local);
            let local_ty = self.local_type(local);
            match usage {
                Use::Read => {
                    passed.push((name, local_ty, Mode::Borrow, Some(binding)));
                    arguments.push(Argument::Borrow(Place::from(local)));
                }
                Use::Write => {
                    passed.push((name, local_ty, Mode::InOut, Some(binding)));
                    arguments.push(Argument::InOut(Place::from(local)));
                }
                Use::Reduce(operator) => {
                    reduced.push((name, local_ty, Mode::InOut, Some(binding)));
                    reductions.push(Reduction {
                        place: Place::from(local),
                        operator,
                    });
                }
            }
        }
        passed.extend(reduced);

        let mut nested = self.nested(statement, &passed);
        nested.iteration(statement, offset.then_some(step));
        let body = match nested.finish() {
            Ok(body) => body,
            Err(diagnostics) => return self.report(diagnostics),
        };
        let position = u32::try_from(self.body.outlined.len()).unwrap_or(u32::MAX);
        self.body.outlined.push(body);
        self.then(statement, |target| TerminatorKind::ParallelFor {
            body: position,
            count: Operand::Copy(Place::from(count)),
            arguments,
            reductions,
            target,
        });
    }

    /// The number of iterations of a loop from `start` to `bound` by the constant `step`, in a
    /// new `i64` temporary: the distance covered in the step's direction, divided by the step
    /// and rounded up. It is 0 or less when the loop runs no iteration.
    fn count(&mut self, start: Local, bound: NodeId, step: i128, node: NodeId) -> Local {
        let ty = self.local_type(start);
        let limit = self.temporary(ty, bound);
        self.write(Place::from(limit), ty, bound);
        let (from, to) = if step > 0 {
            (start, limit)
        } else {
            (limit, start)
        };
        let distance = self.temporary(ty, node);
        self.assign(
            Place::from(distance),
            Rvalue::Binary(
                BinaryOperator::Subtract,
                Operand::Copy(Place::from(to)),
                Operand::Copy(Place::from(from)),
            ),
            node,
        );
        let stride = step.abs();
        if stride == 1 {
            return distance;
        }
        let rounded = self.temporary(ty, node);
        self.assign(
            Place::from(rounded),
            Rvalue::Binary(
                BinaryOperator::Add,
                Operand::Copy(Place::from(distance)),
                Operand::Constant(Constant::integer(ty, stride - 1)),
            ),
            node,
        );
        let count = self.temporary(ty, node);
        self.assign(
            Place::from(count),
            Rvalue::Binary(
                BinaryOperator::FloorDivide,
                Operand::Copy(Place::from(rounded)),
                Operand::Constant(Constant::integer(ty, stride)),
            ),
            node,
        );
        count
    }

    /// In an outlined body: sets the loop variable from the iteration number, `start +
    /// iteration * step` when `step` is given and the iteration number itself otherwise, runs
    /// the loop body, and returns.
    fn iteration(&mut self, statement: NodeId, step: Option<i128>) {
        let Some(variable) = self
            .tree
            .child(statement, Role::Target)
            .and_then(|target| self.target_variable(target))
        else {
            return;
        };
        let ty = self.local_type(variable);
        let iteration = Operand::Copy(Place::from(Local::new(1)));
        let value = match step {
            None => Rvalue::Use(iteration),
            Some(step) => {
                let scaled = self.temporary(ty, statement);
                self.assign(
                    Place::from(scaled),
                    Rvalue::Binary(
                        BinaryOperator::Multiply,
                        iteration,
                        Operand::Constant(Constant::integer(ty, step)),
                    ),
                    statement,
                );
                Rvalue::Binary(
                    BinaryOperator::Add,
                    Operand::Copy(Place::from(Local::new(2))),
                    Operand::Copy(Place::from(scaled)),
                )
            }
        };
        self.assign(Place::from(variable), value, statement);
        let finish = self.new_block();
        self.loops.push(crate::builder::Loop {
            next: finish,
            end: finish,
        });
        self.block(statement, Role::Body);
        self.loops.pop();
        self.jump(&mut Some(finish), statement);
        self.start(finish);
        self.return_none(statement);
        self.drop_all(statement);
        self.terminate(TerminatorKind::Return, statement);
    }

    /// The shared variables the body of the `prange` loop `statement` uses, by binding, with
    /// their locals and how the body uses them. `None` after reporting a use lowering does not
    /// handle.
    fn shared(&mut self, statement: NodeId) -> Option<BTreeMap<NodeId, (Local, Use)>> {
        let loop_start = self.tree.span(statement).start();
        let mut shared = BTreeMap::new();
        let mut nodes = Vec::new();
        for body in self.tree.children_with(statement, Role::Body) {
            self.descendants(body, &mut nodes);
        }
        for &node in &nodes {
            if self.tree.kind(node) != SyntaxKind::Name {
                continue;
            }
            let Some(&Definition::Local(binding)) = self.definition(node) else {
                continue;
            };
            if self.tree.span(binding).start() >= loop_start {
                continue;
            }
            if let Some(local) = self.variable(node) {
                shared.entry(binding).or_insert((local, Use::Read));
            }
        }
        for &node in &nodes {
            for (binding, usage) in self.writes(node)? {
                if let Some((_, current)) = shared.get_mut(&binding) {
                    *current = match (*current, usage) {
                        (Use::Read, usage) => usage,
                        (existing, Use::Read) => existing,
                        (Use::Reduce(left), Use::Reduce(right)) if left == right => {
                            Use::Reduce(left)
                        }
                        (Use::Write, Use::Write) => Use::Write,
                        _ => {
                            self.unsupported(node, "mixed writes to a shared variable");
                            return None;
                        }
                    };
                }
            }
        }
        Some(shared)
    }

    /// The variables, by binding, that the statement or call `node` writes, and how: an
    /// in-place operator on a number variable reduces into it; any other assignment, in-place
    /// operator or `InOut` argument writes into the variable it starts from. `None` after
    /// reporting a write lowering does not handle.
    fn writes(&mut self, node: NodeId) -> Option<Vec<(NodeId, Use)>> {
        let mut writes = Vec::new();
        match self.tree.kind(node) {
            SyntaxKind::Assign | SyntaxKind::AnnotatedAssign => {
                let targets: Vec<NodeId> = self.tree.children_with(node, Role::Target).collect();
                for target in targets {
                    let number = self.variable(target).is_some_and(|local| {
                        matches!(self.local_type(local).kind(self.db), TypeKind::Primitive(_))
                    });
                    if self.tree.kind(target) == SyntaxKind::Name
                        && number
                        && self.is_shared_name(target, node)
                    {
                        self.unsupported(node, "`min` and `max` reductions in `prange`");
                        return None;
                    }
                    writes.extend(self.root(target).map(|binding| (binding, Use::Write)));
                }
            }
            SyntaxKind::AugmentedAssign => {
                let target = self.tree.child(node, Role::Target)?;
                let Some(binding) = self.root(target) else {
                    return Some(writes);
                };
                if self.tree.kind(target) == SyntaxKind::Name && self.is_number(target) {
                    let operator = match arithmetic(self.tree.node(node).operator()) {
                        Some(BinaryOperator::Add | BinaryOperator::Subtract) => {
                            ReductionOperator::Add
                        }
                        Some(BinaryOperator::Multiply) => ReductionOperator::Multiply,
                        _ => {
                            self.unsupported(node, "this reduction");
                            return None;
                        }
                    };
                    writes.push((binding, Use::Reduce(operator)));
                } else {
                    writes.push((binding, Use::Write));
                }
            }
            SyntaxKind::Call => {
                let Some(call) = self.typed.call(node) else {
                    return Some(writes);
                };
                let (types::Callee::Function(item) | types::Callee::Method(item)) = call.callee
                else {
                    return Some(writes);
                };
                let types::Declaration::Function(signature) =
                    &self.db.declaration(item).declaration
                else {
                    return Some(writes);
                };
                for (argument, parameter) in call.arguments.iter().zip(&signature.parameters) {
                    if parameter.mode == Mode::InOut
                        && let Some(argument) = argument
                    {
                        writes.extend(self.root(argument.0).map(|binding| (binding, Use::Write)));
                    }
                }
            }
            _ => {}
        }
        Some(writes)
    }

    /// Whether the name `target`, assigned by `statement`, names a variable bound before
    /// `statement`.
    fn is_shared_name(&self, target: NodeId, statement: NodeId) -> bool {
        matches!(self.definition(target), Some(&Definition::Local(binding))
            if self.tree.span(binding).start() < self.tree.span(statement).start())
    }

    /// Whether the expression `node` is a number.
    fn is_number(&self, node: NodeId) -> bool {
        matches!(self.ty(node).kind(self.db), TypeKind::Primitive(_))
    }

    /// The binding of the variable a place such as `atoms.forces[i]` starts from.
    fn root(&self, mut node: NodeId) -> Option<NodeId> {
        while matches!(
            self.tree.kind(node),
            SyntaxKind::Attribute | SyntaxKind::Subscript
        ) {
            node = self.tree.child(node, Role::Value)?;
        }
        match self.definition(node)? {
            &Definition::Local(binding) => Some(binding),
            _ => None,
        }
    }

    /// Every node below `node`, in source order, onto `nodes`.
    fn descendants(&self, node: NodeId, nodes: &mut Vec<NodeId>) {
        nodes.push(node);
        for child in self.tree.children(node) {
            self.descendants(child, nodes);
        }
    }
}
