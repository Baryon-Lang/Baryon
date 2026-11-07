//! Calls: of functions and methods, with each argument in its parameter's mode; of a struct's
//! constructor; and of a number type, which converts.

use crate::builder::Builder;
use mir::{Argument, Callee, Place, Rvalue, TerminatorKind};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Declaration, Item, Mode, TypeKind};

impl<'db> Builder<'db> {
    /// The call `node`, its result written into `place`.
    pub(crate) fn call(&mut self, place: Place, node: NodeId) {
        let Some(call) = self.typed.call(node) else {
            return self.unsupported(node, "this call");
        };
        let mut arguments = Vec::with_capacity(call.arguments.len());
        for argument in &call.arguments {
            match argument {
                Some(key) => arguments.push(key.0),
                None => return self.unsupported(node, "calls that leave out a defaulted argument"),
            }
        }
        let ty = self.ty(node);
        match &call.callee {
            types::Callee::Function(item) | types::Callee::Method(item) => {
                if !self.typed.errors(node).is_empty() {
                    return self.unsupported(node, "calls that can fail");
                }
                self.function_call(place, *item, call.type_arguments.clone(), &arguments, node);
            }
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
                (Some(("Array", function @ ("zeros" | "ones" | "fill"))), _) => {
                    self.new_array(&place, function, &arguments, node);
                }
                (Some(("Fixed", function @ ("zeros" | "ones" | "fill"))), _) => {
                    self.new_fixed(place, function, &arguments, node);
                }
                _ => self.unsupported(node, &format!("the built-in `{name}`")),
            },
            types::Callee::Value => self.unsupported(node, "calls of a function value"),
        }
    }

    /// A call of the function or method `item` at `type_arguments`, with `arguments` in its
    /// parameters' order, the receiver first for a method.
    fn function_call(
        &mut self,
        place: Place,
        item: Item<'db>,
        type_arguments: Vec<types::Type<'db>>,
        arguments: &[NodeId],
        node: NodeId,
    ) {
        let Declaration::Function(signature) = &self.db.declaration(item).declaration else {
            return self.unsupported(node, "this call");
        };
        let modes: Vec<Mode> = signature
            .parameters
            .iter()
            .map(|parameter| parameter.mode)
            .collect();
        let mut passed = Vec::with_capacity(arguments.len());
        for (&argument, mode) in arguments.iter().zip(modes) {
            passed.push(match mode {
                Mode::Borrow => Argument::Borrow(self.borrow(argument)),
                Mode::InOut => match self.place(argument) {
                    Some(place) => Argument::InOut(place),
                    None => return,
                },
                Mode::Owned => Argument::Owned(self.operand(argument)),
            });
        }
        self.then(node, |target| TerminatorKind::Call {
            callee: Callee::Function {
                item,
                type_arguments,
            },
            arguments: passed,
            destination: place,
            target,
            error: None,
        });
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
