//! Calls: of functions and methods, with each argument in its parameter's mode; of a struct's
//! constructor; of a number type, which converts; and of the built-ins lowering handles.

use crate::builder::Builder;
use mir::{Argument, BinaryOperator, Builtin, Callee, Operand, Place, Rvalue, TerminatorKind};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Declaration, Item, Mode, TypeKind};

impl<'db> Builder<'db> {
    /// The call `node`, its result written into `place`.
    pub(crate) fn call(&mut self, place: Place, node: NodeId) {
        let Some(call) = self.typed.call(node) else {
            return self.unsupported(node, "this call");
        };
        let given: Vec<Option<NodeId>> = call
            .arguments
            .iter()
            .map(|argument| argument.map(|key| key.0))
            .collect();
        if let types::Callee::Function(item) | types::Callee::Method(item) = call.callee {
            return self.function_call(place, item, call.type_arguments.clone(), &given, node);
        }
        let Some(arguments) = given.into_iter().collect::<Option<Vec<NodeId>>>() else {
            return self.unsupported(node, "calls that leave out a defaulted argument");
        };
        let ty = self.ty(node);
        match &call.callee {
            types::Callee::Function(_) | types::Callee::Method(_) => {}
            types::Callee::Constructor(_) => {
                let operands = arguments
                    .into_iter()
                    .map(|argument| self.operand(argument))
                    .collect();
                self.assign(place, Rvalue::Aggregate(ty, operands), node);
            }
            types::Callee::Conversion(to) => match (to.kind(self.db), arguments.as_slice()) {
                (TypeKind::Primitive(_), [argument]) => {
                    let operand = self.operand_as(*argument, *to);
                    self.assign(place, Rvalue::Use(operand), node);
                }
                (&TypeKind::Fixed { element, .. }, _) => {
                    let mut elements = Vec::new();
                    for argument in arguments {
                        self.elements(argument, &mut elements);
                    }
                    let operands = elements
                        .into_iter()
                        .map(|node| self.operand_as(node, element))
                        .collect();
                    self.assign(place, Rvalue::Aggregate(*to, operands), node);
                }
                _ => self.unsupported(node, "this conversion"),
            },
            types::Callee::Builtin(name) => match (name.split_once('.'), arguments.as_slice()) {
                (None, [argument]) if name == "len" => self.len(place, *argument, node),
                (None, [argument]) if name == "round" => self.round(place, *argument, node),
                (None, _) if name == "print" => self.print(place, &arguments, node),
                (None, [first, rest @ ..])
                    if (name == "min" || name == "max")
                        && !rest.is_empty()
                        && matches!(ty.kind(self.db), TypeKind::Primitive(_)) =>
                {
                    self.min_max(place, name == "min", *first, rest, node);
                }
                (Some(("Array", function @ ("zeros" | "ones" | "fill"))), _) => {
                    self.new_array(&place, function, &arguments, node);
                }
                (Some(("Fixed", function @ ("zeros" | "ones" | "fill"))), _) => {
                    self.new_fixed(place, function, &arguments, node);
                }
                (Some(("str", "copy")), [_]) => self.joined(place, &arguments, node),
                (Some(("Array", "copy")), [array]) => self.array_copy(place, *array, node),
                (Some(("Array" | "Fixed", "sum")), [array]) => {
                    self.array_sum(place, *array, node);
                }
                (Some(("Fixed", "dot")), [left, right]) => {
                    self.fixed_dot(place, (*left, *right), node);
                }
                (Some(("Fixed", "round")), [array]) => self.fixed_round(place, *array, node),
                (Some(("math", member)), _) => self.math_function(place, member, &arguments, node),
                _ => self.unsupported(node, &format!("the built-in `{name}`")),
            },
            types::Callee::Value => self.value_call(place, &arguments, node),
        }
    }

    /// A call of the function or method `item` at `type_arguments`, with `arguments` in its
    /// parameters' order, the receiver first for a method, and `None` for each argument left
    /// out, which takes its default. A call that can fail gets an error edge (the `errors`
    /// module).
    fn function_call(
        &mut self,
        place: Place,
        item: Item<'db>,
        type_arguments: Vec<types::Type<'db>>,
        arguments: &[Option<NodeId>],
        node: NodeId,
    ) {
        let Declaration::Function(signature) = &self.db.declaration(item).declaration else {
            return self.unsupported(node, "this call");
        };
        let parameters: Vec<(Mode, types::Type<'db>)> = signature
            .parameters
            .iter()
            .map(|parameter| (parameter.mode, parameter.ty))
            .collect();
        // Each `Callable` parameter is an implicit type parameter after the written ones, and
        // its argument is the type of the function passed (decision 0026).
        let mut type_arguments = type_arguments;
        for (&argument, (_, ty)) in arguments.iter().zip(&parameters) {
            if matches!(ty.kind(self.db), TypeKind::Callable { .. }) {
                let Some(argument) = argument else {
                    return self.unsupported(node, "a default for a `Callable` parameter");
                };
                let function = self.ty(argument);
                if !matches!(function.kind(self.db), TypeKind::Function { .. }) {
                    return self.unsupported(argument, "passing on a `Callable` parameter");
                }
                type_arguments.push(function);
            }
        }
        let mut passed = Vec::with_capacity(arguments.len());
        for (position, (&argument, (mode, ty))) in arguments.iter().zip(parameters).enumerate() {
            let Some(argument) = argument else {
                if mode == Mode::InOut {
                    // The callee's changes to a default would go nowhere the caller can see.
                    return self.unsupported(node, "leaving out an `InOut` argument");
                }
                let Some(constant) = self.default_value(item, position, node) else {
                    return;
                };
                passed.push(if mode == Mode::Owned {
                    Argument::Owned(Operand::Constant(constant))
                } else {
                    let temporary = self.temporary(ty, node);
                    let value = Rvalue::Use(Operand::Constant(constant));
                    self.assign(Place::from(temporary), value, node);
                    Argument::Borrow(Place::from(temporary))
                });
                continue;
            };
            passed.push(match mode {
                Mode::Borrow => match self.borrow_as(argument, ty) {
                    Some(place) => Argument::Borrow(place),
                    None => return,
                },
                Mode::InOut => match self.place(argument) {
                    Some(place) => Argument::InOut(place),
                    None => return,
                },
                Mode::Owned => Argument::Owned(self.operand_as(argument, ty)),
            });
        }
        let errors = self.typed.errors(node);
        let edge = self.error_edge(errors, node);
        let next = self.new_block();
        self.terminate(
            TerminatorKind::Call {
                callee: Callee::Function {
                    item,
                    type_arguments,
                },
                arguments: passed,
                destination: place,
                target: next,
                error: edge.clone(),
            },
            node,
        );
        if let Some(edge) = edge {
            self.dispatch(&edge, errors, node);
        }
        self.start(next);
    }

    /// A call of a `Callable` value, such as the parameter `derivative` in
    /// `derivative(t, state)`, into `place`. Each argument is borrowed, the mode a `Callable`
    /// gives every parameter. Monomorphization turns it into a direct call of the function the
    /// instance received (decision 0026).
    fn value_call(&mut self, place: Place, arguments: &[NodeId], node: NodeId) {
        let Some(function) = self.tree.child(node, Role::Function) else {
            return self.unsupported(node, "this call");
        };
        let callee = self.operand(function);
        let passed = arguments
            .iter()
            .map(|&argument| Argument::Borrow(self.borrow(argument)))
            .collect();
        let next = self.new_block();
        self.terminate(
            TerminatorKind::Call {
                callee: Callee::Value(callee),
                arguments: passed,
                destination: place,
                target: next,
                error: None,
            },
            node,
        );
        self.start(next);
    }

    /// `print(arguments…)`, its `None` result written into `place`. A literal is passed as its
    /// constant, so a backend can print a string literal without a `str` value; anything else
    /// is borrowed, since printing does not consume it.
    fn print(&mut self, place: Place, arguments: &[NodeId], node: NodeId) {
        let keyword = self
            .tree
            .child(node, Role::Arguments)
            .into_iter()
            .flat_map(|list| self.tree.children_with(list, Role::Item))
            .any(|item| self.tree.kind(item) == SyntaxKind::Keyword);
        if keyword {
            return self.unsupported(node, "keyword arguments of `print`");
        }
        let mut passed = Vec::with_capacity(arguments.len());
        for &argument in arguments {
            passed.push(if self.is_literal(argument) {
                Argument::Owned(Operand::Constant(self.literal(argument)))
            } else {
                Argument::Borrow(self.borrow(argument))
            });
        }
        let next = self.new_block();
        self.terminate(
            TerminatorKind::Call {
                callee: Callee::Builtin(Builtin::Print),
                arguments: passed,
                destination: place,
                target: next,
                error: None,
            },
            node,
        );
        self.start(next);
    }

    /// `min(first, rest…)` when `smallest`, else `max(…)`, of numbers, its result written into
    /// `place`. As in Python, an argument replaces the best so far only when it is strictly
    /// smaller (or larger), so the first of equal values wins and a `nan` after the first
    /// argument is skipped.
    fn min_max(
        &mut self,
        place: Place,
        smallest: bool,
        first: NodeId,
        rest: &[NodeId],
        node: NodeId,
    ) {
        let ty = self.ty(node);
        let best = self.temporary(ty, node);
        let operand = self.operand_as(first, ty);
        self.assign(Place::from(best), Rvalue::Use(operand), node);
        let test = if smallest {
            BinaryOperator::Less
        } else {
            BinaryOperator::Greater
        };
        for &argument in rest {
            let candidate = self.temporary(ty, argument);
            let operand = self.operand_as(argument, ty);
            self.assign(Place::from(candidate), Rvalue::Use(operand), argument);
            let better = self.temporary(self.boolean_type(), argument);
            self.assign(
                Place::from(better),
                Rvalue::Binary(
                    test,
                    Operand::Copy(Place::from(candidate)),
                    Operand::Copy(Place::from(best)),
                ),
                argument,
            );
            let (replace, join) = (self.new_block(), self.new_block());
            self.terminate(
                TerminatorKind::Branch {
                    condition: Operand::Copy(Place::from(better)),
                    then: replace,
                    otherwise: join,
                },
                argument,
            );
            self.start(replace);
            self.assign(
                Place::from(best),
                Rvalue::Use(Operand::Copy(Place::from(candidate))),
                argument,
            );
            self.terminate(TerminatorKind::Goto(join), argument);
            self.start(join);
        }
        self.assign(place, Rvalue::Use(Operand::Copy(Place::from(best))), node);
    }

    /// A place holding `argument` as the parameter type `ty`, to borrow. An argument that
    /// changes representation, such as an `f64` passed where `f64 | None` is borrowed, is
    /// converted into a temporary first. `None` after reporting an argument of a type that is
    /// not a copy type, since wrapping it would move or copy the caller's value.
    fn borrow_as(&mut self, argument: NodeId, ty: types::Type<'db>) -> Option<Place> {
        let from = self.ty(argument);
        if !self.converts(from, ty) {
            return Some(self.borrow(argument));
        }
        if !self.is_copy(from) {
            self.unsupported(
                argument,
                "borrowing a value that is not a copy type as an optional",
            );
            return None;
        }
        let temporary = self.temporary(ty, argument);
        self.write(Place::from(temporary), ty, argument);
        Some(Place::from(temporary))
    }

    /// The elements of a `Fixed` array that `node` writes, in row-major order, onto
    /// `elements`: a row such as `(1.0, 0.0)` is its items, each of which may be a row again.
    fn elements(&self, node: NodeId, elements: &mut Vec<NodeId>) {
        if self.tree.kind(node) != SyntaxKind::Tuple {
            return elements.push(node);
        }
        for item in self.tree.children_with(node, Role::Item) {
            self.elements(item, elements);
        }
    }

    /// The value of `node` evaluated for its effect, as an expression statement: the result is
    /// computed into a temporary and dropped.
    pub(crate) fn discard(&mut self, node: NodeId) {
        let ty = self.ty(node);
        let temporary = self.temporary(ty, node);
        self.write(Place::from(temporary), ty, node);
        self.drop_later(temporary);
    }
}
