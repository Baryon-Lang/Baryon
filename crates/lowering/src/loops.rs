//! Loops: `while`, and `for` over `range`, over a rank-1 `Array` or `Fixed` array, and over a
//! tuple whose items have one type.
//!
//! Every `for` loop keeps a hidden counter, so the body may assign the loop variable without
//! changing which pass comes next, as in Python. A loop over `prange` is a parallel loop, which
//! the `parallel` module lowers.

use crate::builder::{Builder, Loop};
use mir::{
    BinaryOperator, BlockId, Check, Constant, Local, Operand, Place, Projection, Rvalue,
    TerminatorKind,
};
use syntax::{NodeId, Role};
use types::{Type, TypeKind};

/// The step of a `range` loop.
enum Step {
    /// A constant step, whose sign says at compile time which way the loop test goes.
    Constant(i128),
    /// A step computed at run time, by this node.
    Runtime(NodeId),
}

impl<'db> Builder<'db> {
    /// `while condition: body`. `continue` tests the condition again; `break` leaves. The syntax
    /// check rejects a loop's `else`, so there is none to lower.
    pub(crate) fn while_statement(&mut self, statement: NodeId) {
        let Some(condition) = self.tree.child(statement, Role::Condition) else {
            return;
        };
        let header = self.new_block();
        self.terminate(TerminatorKind::Goto(header), statement);
        self.start(header);
        let condition = self.condition(condition);
        let (body, end) = (self.new_block(), self.new_block());
        self.terminate(
            TerminatorKind::Branch {
                condition,
                then: body,
                otherwise: end,
            },
            statement,
        );
        self.start(body);
        self.repeat(statement, header, end);
        self.start(end);
    }

    /// `for target in iterator: body`: over `range`, `prange` or `simd(range(…))` when the
    /// iterator is such a call, and over the elements of a collection otherwise.
    pub(crate) fn for_statement(&mut self, statement: NodeId) {
        let (Some(target), Some(iterator)) = (
            self.tree.child(statement, Role::Target),
            self.tree.child(statement, Role::Iterator),
        ) else {
            return;
        };
        if self.is_range(iterator) || self.simd_range(iterator).is_some() {
            self.range_loop(statement, target, iterator);
        } else {
            self.collection_loop(statement, target, iterator);
        }
    }

    /// Whether `iterator` is a call of the built-in `range` or `prange`.
    fn is_range(&self, iterator: NodeId) -> bool {
        self.typed.call(iterator).is_some_and(|call| {
            matches!(&call.callee, types::Callee::Builtin(name)
                if name == "range" || name == "prange")
        })
    }

    /// `for name in range(start, stop, step): body`. The counter runs from `start` while it is
    /// below `stop`, or above it for a negative step, and the variable takes its value at the top
    /// of each pass. `start`, `stop` and `step` are computed once, in that order, before the
    /// loop.
    ///
    /// A constant step decides the direction of the test when lowering. A step computed at run
    /// time is checked to be non-zero, a panic where Python raises `ValueError`, and its sign
    /// picks the test on each pass, a branch LLVM moves out of the loop. Adding the step can wrap
    /// only when `stop` is within one step of the counter type's limit.
    fn range_loop(&mut self, statement: NodeId, target: NodeId, iterator: NodeId) {
        let Some(variable) = self.target_variable(target) else {
            return self.unsupported(target, "`for` loops over `range` that unpack");
        };
        let (iterator, vectorized) = match self.simd_range(iterator) {
            Some(range) => (range, true),
            None => (iterator, false),
        };
        let Some((first, bound, step)) = self.range(iterator) else {
            return;
        };
        if self.is_prange(iterator) {
            let Step::Constant(step) = step else {
                return self.unsupported(iterator, "a `prange` step that is not a constant");
            };
            return self.parallel_for(statement, variable, first, bound, step);
        }
        let mut vectorized = vectorized;
        if vectorized {
            let reductions = self.float_reductions(statement);
            let complex = reductions
                .iter()
                .any(|&(local, _)| !self.is_float(self.local_type(local)));
            if !reductions.is_empty() {
                match step {
                    Step::Constant(step)
                        if !complex && self.local_type(variable) == self.int_type() =>
                    {
                        let range = (first, bound, step);
                        return self.laned_for(statement, variable, range, &reductions);
                    }
                    // A complex sum, a counter other than `i64` or a step known only at run
                    // time keeps the plain loop's order, unvectorized.
                    _ => vectorized = false,
                }
            }
        }
        let ty = self.local_type(variable);
        let counter = self.temporary(ty, iterator);
        match first {
            Some(first) => self.write(Place::from(counter), ty, first),
            None => self.assign(
                Place::from(counter),
                Rvalue::Use(Operand::Constant(Constant::integer(ty, 0))),
                iterator,
            ),
        }
        let limit = self.temporary(ty, bound);
        self.write(Place::from(limit), ty, bound);
        let step = match step {
            Step::Constant(step) => Err(step),
            Step::Runtime(node) => Ok(self.runtime_step(ty, node)),
        };

        let header = self.new_block();
        self.terminate(TerminatorKind::Goto(header), statement);
        self.start(header);
        if vectorized {
            self.body.vectorized.push(header);
        }
        let (body, next, end) = (self.new_block(), self.new_block(), self.new_block());
        let test = |operator| (operator, counter, limit, body, end);
        match step {
            Err(constant) if constant > 0 => self.loop_test(test(BinaryOperator::Less), iterator),
            Err(_) => self.loop_test(test(BinaryOperator::Greater), iterator),
            Ok((_, positive)) => {
                let (up, down) = (self.new_block(), self.new_block());
                self.terminate(
                    TerminatorKind::Branch {
                        condition: Operand::Copy(Place::from(positive)),
                        then: up,
                        otherwise: down,
                    },
                    iterator,
                );
                self.start(up);
                self.loop_test(test(BinaryOperator::Less), iterator);
                self.start(down);
                self.loop_test(test(BinaryOperator::Greater), iterator);
            }
        }
        self.start(body);
        self.assign(
            Place::from(variable),
            Rvalue::Use(Operand::Copy(Place::from(counter))),
            target,
        );
        self.repeat(statement, next, end);
        self.start(next);
        let step = match step {
            Err(constant) => Operand::Constant(Constant::integer(ty, constant)),
            Ok((local, _)) => Operand::Copy(Place::from(local)),
        };
        self.assign(
            Place::from(counter),
            Rvalue::Binary(
                BinaryOperator::Add,
                Operand::Copy(Place::from(counter)),
                step,
            ),
            iterator,
        );
        self.terminate(TerminatorKind::Goto(header), statement);
        self.start(end);
    }

    /// Computes the run-time step at `node` into a new local of the counter type `ty`, panics
    /// when it is zero, and returns it with a new `bool` local that holds whether it is above
    /// zero.
    fn runtime_step(&mut self, ty: Type<'db>, node: NodeId) -> (Local, Local) {
        let step = self.temporary(ty, node);
        self.write(Place::from(step), ty, node);
        let zero = || Operand::Constant(Constant::integer(ty, 0));
        let boolean = self.boolean_type();
        let nonzero = self.temporary(boolean, node);
        self.assign(
            Place::from(nonzero),
            Rvalue::Binary(
                BinaryOperator::NotEqual,
                Operand::Copy(Place::from(step)),
                zero(),
            ),
            node,
        );
        self.then(node, |target| TerminatorKind::Assert {
            condition: Operand::Copy(Place::from(nonzero)),
            check: Check::NonZeroStep,
            target,
        });
        let positive = self.temporary(boolean, node);
        self.assign(
            Place::from(positive),
            Rvalue::Binary(
                BinaryOperator::Greater,
                Operand::Copy(Place::from(step)),
                zero(),
            ),
            node,
        );
        (step, positive)
    }

    /// Ends the current block with the test of a loop: `counter operator limit` goes on to
    /// `body`, and anything else to `end`.
    fn loop_test(
        &mut self,
        (operator, counter, limit, body, end): (BinaryOperator, Local, Local, BlockId, BlockId),
        node: NodeId,
    ) {
        let condition = self.temporary(self.boolean_type(), node);
        self.assign(
            Place::from(condition),
            Rvalue::Binary(
                operator,
                Operand::Copy(Place::from(counter)),
                Operand::Copy(Place::from(limit)),
            ),
            node,
        );
        self.terminate(
            TerminatorKind::Branch {
                condition: Operand::Copy(Place::from(condition)),
                then: body,
                otherwise: end,
            },
            node,
        );
    }

    /// `for target in collection: body`, over a rank-1 `Array` or `Fixed` array or a tuple whose
    /// items have one copy type. The collection is computed once and borrowed for the loop, as
    /// the reference's values and references chapter says, so the body cannot change it. Each
    /// pass copies the next element into `target`, which may unpack it: `for x, y in points`.
    ///
    /// An array's element is read at the counter without a bounds check, since the counter
    /// stays below the length. A tuple's items have no index to read at, so each pass switches
    /// on the counter to the block that copies its item; LLVM unrolls the short loop.
    fn collection_loop(&mut self, statement: NodeId, target: NodeId, iterator: NodeId) {
        let ty = self.ty(iterator);
        let (element, items) = match ty.kind(self.db) {
            TypeKind::Array { element, rank: 1 } => (*element, None),
            TypeKind::Fixed { element, shape } if shape.rank() == 1 => (*element, None),
            TypeKind::Tuple(items) if !items.is_empty() && self.is_copy(ty) => {
                (items[0], Some(items.len()))
            }
            TypeKind::Tuple(_) => {
                return self.unsupported(
                    iterator,
                    "`for` loops over a tuple of values that are not copied",
                );
            }
            _ => {
                return self.unsupported(iterator, "`for` loops over values of this type");
            }
        };
        let collection = self.borrow(iterator);
        let int = self.int_type();
        let length = match items {
            Some(count) => Operand::Constant(Constant::integer(
                int,
                i128::try_from(count).unwrap_or(i128::MAX),
            )),
            None => self.length(&collection, ty, 0, iterator),
        };
        let limit = self.temporary(int, iterator);
        self.assign(Place::from(limit), Rvalue::Use(length), iterator);
        let counter = self.temporary(int, iterator);
        self.assign(
            Place::from(counter),
            Rvalue::Use(Operand::Constant(Constant::integer(int, 0))),
            iterator,
        );

        let header = self.new_block();
        self.terminate(TerminatorKind::Goto(header), statement);
        self.start(header);
        let (body, next, end) = (self.new_block(), self.new_block(), self.new_block());
        self.loop_test((BinaryOperator::Less, counter, limit, body, end), iterator);
        self.start(body);
        match items {
            None => {
                let place = collection.project(Projection::Index(vec![counter]));
                self.assign_part(target, place, element);
            }
            Some(count) => self.tuple_item(target, (&collection, count), counter, element),
        }
        self.repeat(statement, next, end);
        self.start(next);
        self.assign(
            Place::from(counter),
            Rvalue::Binary(
                BinaryOperator::Add,
                Operand::Copy(Place::from(counter)),
                Operand::Constant(Constant::integer(int, 1)),
            ),
            iterator,
        );
        self.terminate(TerminatorKind::Goto(header), statement);
        self.start(end);
    }

    /// Copies item number `counter` of the tuple at `tuple`, which has `count` items of type
    /// `element`, into `target`: a switch on the counter goes to one block per item, and each
    /// joins the rest of the loop body.
    fn tuple_item(
        &mut self,
        target: NodeId,
        (tuple, count): (&Place, usize),
        counter: Local,
        element: Type<'db>,
    ) {
        let blocks: Vec<BlockId> = (0..count).map(|_| self.new_block()).collect();
        let otherwise = self.new_block();
        self.terminate(
            TerminatorKind::Switch {
                value: Operand::Copy(Place::from(counter)),
                cases: (0..).zip(blocks.iter().copied()).collect(),
                otherwise,
            },
            target,
        );
        self.start(otherwise);
        // The counter stays below the count, so no other value reaches the switch.
        self.terminate(TerminatorKind::Unreachable, target);
        let mut join = None;
        for (field, block) in (0u32..).zip(blocks) {
            self.start(block);
            let place = tuple.clone().project(Projection::Field(field));
            self.assign_part(target, place, element);
            self.jump(&mut join, target);
        }
        self.join(join);
    }

    /// The start, stop and step of `range(start, stop, step)` or `prange(…)` at `iterator`:
    /// the start and stop nodes, the start `None` when left out, and the step, `1` when left
    /// out. `None` after an error when `iterator` is not such a call.
    fn range(&mut self, iterator: NodeId) -> Option<(Option<NodeId>, NodeId, Step)> {
        let keywords = self.keyword_values(iterator);
        let arguments = self
            .typed
            .call(iterator)
            .filter(|call| {
                matches!(&call.callee, types::Callee::Builtin(name)
                    if name == "range" || name == "prange")
            })
            .and_then(|call| {
                call.arguments
                    .iter()
                    .map(|argument| argument.map(|key| key.0))
                    .filter(|argument| argument.is_none_or(|node| !keywords.contains(&node)))
                    .collect::<Option<Vec<NodeId>>>()
            });
        let (first, bound, stride) = match arguments.as_deref() {
            Some(&[bound]) => (None, bound, None),
            Some(&[first, bound]) => (Some(first), bound, None),
            Some(&[first, bound, stride]) => (Some(first), bound, Some(stride)),
            _ => {
                self.unsupported(iterator, "`for` loops over anything but `range`");
                return None;
            }
        };
        let Some(stride) = stride else {
            return Some((first, bound, Step::Constant(1)));
        };
        // A constant zero step is checked at run time too, so it panics as Python raises.
        match self.constant_integer(stride) {
            Some(step) if step != 0 => Some((first, bound, Step::Constant(step))),
            _ => Some((first, bound, Step::Runtime(stride))),
        }
    }

    /// The body of the loop `statement`, from the current block: `continue` and the end of the
    /// body go to `next`, `break` to `end`.
    fn repeat(&mut self, statement: NodeId, next: BlockId, end: BlockId) {
        self.loops.push(Loop { next, end });
        self.block(statement, Role::Body);
        self.loops.pop();
        self.jump(&mut Some(next), statement);
    }
}
