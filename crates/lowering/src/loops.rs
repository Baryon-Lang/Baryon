//! Loops: `while`, and `for` over `range`, over a rank-1 `Array` or `Fixed` array, over a list
//! of a copy type, and over a tuple whose items have one type.
//!
//! Every `for` loop keeps a hidden counter, so the body may assign the loop variable without
//! changing which pass comes next, as in Python. A loop over `prange` is a parallel loop, which
//! the `parallel` module lowers.
//!
//! A loop over a list or tuple of values that are not copied, such as `list[str]`, borrows each
//! element, as the reference's values and references chapter says: the loop variable is the
//! element, read in place, never a copy and never dropped. Lowering makes each read of the
//! variable a read of the element's place, which is exact while every use of the variable is
//! inside the loop; a variable also used outside it is reported as not lowered yet.
//!
//! What runs on each pass is a [`Pass`]: a `for` statement's body, or the rest of a list
//! comprehension, which the `comprehensions` module lowers with the same loops.

use crate::builder::{Builder, Loop};
use database::Definition;
use mir::{
    BinaryOperator, BlockId, Check, Constant, Local, Operand, Place, Projection, Rvalue,
    TerminatorKind,
};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Type, TypeKind};

/// What one pass of a loop runs, from the current block: the body of a `for` statement, or the
/// rest of a comprehension. It gets the block that starts the next pass and the block after the
/// loop, and must end by going to one of them or to a block that cannot return.
pub(crate) type Pass<'pass, 'db> = &'pass mut dyn FnMut(&mut Builder<'db>, BlockId, BlockId);

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
        let pass: Pass<'_, 'db> = &mut |builder, next, end| builder.repeat(statement, next, end);
        if self.is_range(iterator) || self.simd_range(iterator).is_some() {
            self.range_loop(statement, target, iterator, pass);
        } else {
            self.collection_loop(statement, target, iterator, pass);
        }
    }

    /// Whether `iterator` is a call of the built-in `range` or `prange`.
    pub(crate) fn is_range(&self, iterator: NodeId) -> bool {
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
    ///
    /// `statement` is the `for` statement, or a comprehension's `for` clause, which never reaches
    /// the `prange` and `simd` paths: those need a statement whose body they can outline or
    /// vectorize.
    pub(crate) fn range_loop(
        &mut self,
        statement: NodeId,
        target: NodeId,
        iterator: NodeId,
        pass: Pass<'_, 'db>,
    ) {
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
        pass(self, next, end);
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
            check: Check::NonZeroStep { slice: false },
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

    /// `for target in collection: body`, over a rank-1 `Array` or `Fixed` array, a list, or a
    /// tuple whose items have one type. An element of a copy type is copied into `target`; one
    /// that is not copied is borrowed (the module documentation). The collection is computed once and borrowed for the loop, as
    /// the reference's values and references chapter says, so the body cannot change it. Each
    /// pass copies the next element into `target`, which may unpack it: `for x, y in points`.
    ///
    /// An array's or list's element is read at the counter without a bounds check, since the
    /// counter stays below the length, which cannot change while the loop borrows the collection. A tuple's items have no index to read at, so each pass switches
    /// on the counter to the block that copies its item; LLVM unrolls the short loop.
    pub(crate) fn collection_loop(
        &mut self,
        statement: NodeId,
        target: NodeId,
        iterator: NodeId,
        pass: Pass<'_, 'db>,
    ) {
        let ty = self.ty(iterator);
        let (element, items) = match ty.kind(self.db) {
            TypeKind::Array { element, rank: 1 } | TypeKind::List(element) => (*element, None),
            TypeKind::Fixed { element, shape } if shape.rank() == 1 => (*element, None),
            TypeKind::Tuple(items) if !items.is_empty() => (items[0], Some(items.len())),
            _ => {
                return self.unsupported(iterator, "`for` loops over values of this type");
            }
        };
        let borrowed = !self.is_copy(element);
        if borrowed && !self.confined(statement, target) {
            return self.unsupported(
                target,
                "a loop variable that borrows an element and is also used outside the loop",
            );
        }
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
        let aliases = self.aliases.clone();
        match items {
            None => {
                let place = collection.project(Projection::Index(vec![counter]));
                self.bind_element(target, place, element);
            }
            Some(count) if borrowed => {
                // The item is copied bit for bit into a temporary that is never dropped, so the
                // switch's arms meet at one place the variable can borrow.
                let item = self.temporary(element, target);
                self.tuple_item(
                    target,
                    (&collection, count),
                    counter,
                    &mut |builder, place| {
                        builder.assign(
                            Place::from(item),
                            Rvalue::Use(Operand::Copy(place)),
                            target,
                        );
                    },
                );
                self.bind_element(target, Place::from(item), element);
            }
            Some(count) => {
                self.tuple_item(
                    target,
                    (&collection, count),
                    counter,
                    &mut |builder, place| {
                        builder.assign_part(target, place, element);
                    },
                );
            }
        }
        pass(self, next, end);
        self.aliases = aliases;
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

    /// Runs `write` on item number `counter` of the tuple at `tuple`, which has `count` items,
    /// for the loop target `target`: a switch on the counter goes to one block per item, each
    /// writes its item, and each joins the rest of the loop body.
    fn tuple_item(
        &mut self,
        target: NodeId,
        (tuple, count): (&Place, usize),
        counter: Local,
        write: &mut dyn FnMut(&mut Self, Place),
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
            write(self, place);
            self.jump(&mut join, target);
        }
        self.join(join);
    }

    /// Binds the loop target `target` to the element at `place`, of type `ty`: copies an element
    /// of a copy type into it, and makes each name that would hold a value that is not copied
    /// borrow its part of the element in place, until the loop ends. A target that unpacks
    /// does this for each part, so `for name, mass in species` borrows `name` and copies `mass`.
    fn bind_element(&mut self, target: NodeId, place: Place, ty: Type<'db>) {
        if self.is_copy(ty) {
            return self.assign_part(target, place, ty);
        }
        match (self.tree.kind(target), ty.kind(self.db)) {
            (SyntaxKind::Name, _) => {
                if let Some(&Definition::Local(binding)) = self.definition(target) {
                    self.aliases.insert(binding, place);
                }
            }
            (SyntaxKind::Tuple, TypeKind::Tuple(items)) => {
                let parts: Vec<NodeId> = self.tree.children_with(target, Role::Item).collect();
                for ((field, part), &item) in (0u32..).zip(parts).zip(items) {
                    let at = place.clone().project(Projection::Field(field));
                    self.bind_element(part, at, item);
                }
            }
            _ => self.unsupported(target, "this loop target"),
        }
    }

    /// Whether every use of the variables `target` binds lies inside `statement`, so reading
    /// each one as the element it borrows is exact. A comprehension's variables belong to the
    /// comprehension alone, so they always are.
    fn confined(&self, statement: NodeId, target: NodeId) -> bool {
        if self.tree.kind(statement) != SyntaxKind::For {
            return true;
        }
        let (mut everywhere, mut inside, mut names) = (Vec::new(), Vec::new(), Vec::new());
        self.descendants(self.function, &mut everywhere);
        self.descendants(statement, &mut inside);
        self.descendants(target, &mut names);
        let uses = |nodes: &[NodeId], binding: NodeId| {
            nodes
                .iter()
                .filter(|&&node| self.definition(node) == Some(&Definition::Local(binding)))
                .count()
        };
        names.into_iter().all(|name| match self.definition(name) {
            Some(&Definition::Local(binding)) => {
                uses(&everywhere, binding) == uses(&inside, binding)
            }
            _ => true,
        })
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
