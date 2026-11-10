//! Calls: of functions and methods, with each argument in its parameter's mode; of a struct's
//! constructor; of a number type, which converts; and of the built-ins lowering handles.

use crate::builder::Builder;
use mir::{
    Argument, BinaryOperator, Builtin, Callee, Constant, ConstantValue, Operand, Place, Projection,
    Rvalue, TerminatorKind,
};
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
            types::Callee::Constructor(item) => {
                let fields = self.field_types(*item);
                let operands = arguments
                    .into_iter()
                    .enumerate()
                    .map(|(position, argument)| match fields.get(position) {
                        Some(&field) if !self.is_generic(field) => self.operand_as(argument, field),
                        _ => self.operand(argument),
                    })
                    .collect();
                self.assign(place, Rvalue::Aggregate(ty, operands), node);
            }
            types::Callee::Conversion(to) => match (to.kind(self.db), arguments.as_slice()) {
                (TypeKind::Primitive(_), [argument])
                    if !matches!(self.ty(*argument).kind(self.db), TypeKind::Primitive(_)) =>
                {
                    // `float("nan")` parses text, which no backend does yet.
                    let from = self.ty(*argument).display(self.db);
                    let to = to.display(self.db);
                    self.unsupported(node, &format!("converting a `{from}` to `{to}`"));
                }
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
                (None, [argument]) if name == "abs" => self.absolute(place, *argument, node),
                (None, _) if name == "print" => self.print(place, &arguments, node),
                (None, [list]) if name == "Array" => self.array_from_list(&place, *list, node),
                (None, [first, rest @ ..])
                    if (name == "min" || name == "max")
                        && !rest.is_empty()
                        && matches!(ty.kind(self.db), TypeKind::Primitive(_)) =>
                {
                    self.min_max(place, name == "min", *first, rest, node);
                }
                (
                    Some((owner @ ("Array" | "Fixed"), function @ ("zeros" | "ones" | "fill"))),
                    _,
                ) if !matches!(ty.kind(self.db), TypeKind::None) => {
                    if owner == "Array" {
                        self.new_array(&place, function, &arguments, node);
                    } else {
                        self.new_fixed(place, function, &arguments, node);
                    }
                }
                (Some(("str", "copy")), [_]) => self.joined(place, &arguments, node),
                (Some(("list", "append")), [list, value]) => {
                    self.list_append(place, *list, *value, node);
                }
                (Some(("Array" | "Fixed", method)), _) => {
                    self.array_method(place, method, &arguments, node);
                }
                (Some(("math", member)), _) => self.math_function(place, member, &arguments, node),
                (Some(("random", "Philox")), [seed, rest @ ..]) if rest.len() <= 1 => {
                    self.philox(place, *seed, rest.first().copied(), node);
                }
                (Some(("Philox", "uniform")), [generator]) => self.uniform(place, *generator, node),
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
        // A method of a generic class is generic in the class's type parameters, which come
        // first: the receiver's type arguments (roadmap 1.9.18).
        let mut type_arguments = match self.class_arguments(item, arguments) {
            Some(class) => class.into_iter().chain(type_arguments).collect::<Vec<_>>(),
            None => return self.unsupported(node, "a method call on this receiver"),
        };
        // Each `Callable` parameter is an implicit type parameter after the written ones, and
        // its argument is the type of the function passed (decision 0026).
        for (&argument, (_, ty)) in arguments.iter().zip(&parameters) {
            if matches!(ty.kind(self.db), TypeKind::Callable { .. }) {
                let Some(argument) = argument else {
                    return self.unsupported(node, "a default for a `Callable` parameter");
                };
                // A function's own type, or the `Callable` type of a parameter this body
                // received and passes on, which monomorphization replaces by the function the
                // instance received (`monomorphize::forward_callables`).
                type_arguments.push(self.ty(argument));
            }
        }
        // The arguments passed come first, then each default left out, in parameter order
        // (decision 0027).
        let mut passed = Vec::with_capacity(arguments.len());
        for (position, (&argument, &(mode, ty))) in arguments.iter().zip(&parameters).enumerate() {
            let Some(argument) = argument else {
                passed.push(None);
                continue;
            };
            if position == 0
                && let Some(receiver) = self.promoted_receiver(argument, mode, node)
            {
                passed.push(Some(receiver));
                continue;
            }
            passed.push(Some(match mode {
                Mode::Borrow => match self.borrow_as(argument, ty) {
                    Some(place) => Argument::Borrow(place),
                    None => return,
                },
                Mode::InOut => match self.place(argument) {
                    Some(place) => Argument::InOut(place),
                    None => return,
                },
                Mode::Owned => Argument::Owned(self.operand_as(argument, ty)),
            }));
        }
        let mut complete = Vec::with_capacity(passed.len());
        for (position, (argument, parameter)) in passed.into_iter().zip(parameters).enumerate() {
            complete.push(match argument {
                Some(argument) => argument,
                None => match self.default_argument(item, position, parameter, node) {
                    Some(argument) => argument,
                    None => return,
                },
            });
        }
        let passed = complete;
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

    /// The type arguments of the class that declares `item`, for a call whose receiver is the
    /// first of `arguments`: `[f64]` for `p.total()` with `p: Pair[f64]`, or `[T]` inside
    /// another generic body, which monomorphization replaces. Empty for a function and for a
    /// method of a class with no type parameters. `None` when the class has type parameters
    /// but the receiver is not a struct of that class, such as a method an embedded generic
    /// struct provides (decision 0031), which is not built yet.
    fn class_arguments(
        &self,
        item: Item<'db>,
        arguments: &[Option<NodeId>],
    ) -> Option<Vec<types::Type<'db>>> {
        let tree = self.db.parse(item.file(self.db)).tree();
        let Some((_, Some(class))) = crate::definition(tree, item.name(self.db)) else {
            return Some(Vec::new());
        };
        let Some(parameters) = tree.child(class, Role::TypeParameters) else {
            return Some(Vec::new());
        };
        let count = tree.children_with(parameters, Role::Item).count();
        let receiver = arguments.first().copied().flatten()?;
        match self.ty(receiver).kind(self.db) {
            TypeKind::Struct(nominal)
                if nominal.file == item.file(self.db)
                    && tree.child(class, Role::Name) == Some(nominal.name)
                    && nominal.arguments.len() == count =>
            {
                Some(nominal.arguments.clone())
            }
            _ => None,
        }
    }

    /// The receiver of the method call `node` when the checker found the method through
    /// embedded fields (decision 0031): the receiver `argument`'s place, stepped into each
    /// embedded field, so `ion.energy()` passes `ion.particle` as `self`. `None` for any other
    /// call. The checker rejects a promoted method that takes `self` as `Owned`.
    fn promoted_receiver(
        &mut self,
        argument: NodeId,
        mode: Mode,
        node: NodeId,
    ) -> Option<Argument<'db>> {
        let function = self.tree.child(node, Role::Function)?;
        let path = self.typed.promotion(function).to_vec();
        if path.is_empty() {
            return None;
        }
        let project = |base: Place| {
            path.iter().fold(base, |place, &field| {
                place.project(Projection::Field(field))
            })
        };
        Some(match mode {
            Mode::Borrow => Argument::Borrow(project(self.borrow(argument))),
            Mode::InOut => Argument::InOut(project(self.place(argument)?)),
            Mode::Owned => {
                self.unsupported(argument, "moving an embedded field out of its struct");
                return None;
            }
        })
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

    /// The declared types of the fields of the struct `item`, in declaration order; empty when
    /// `item` is not a struct. A generic struct's fields keep their type parameters.
    fn field_types(&self, item: Item<'db>) -> Vec<types::Type<'db>> {
        match &self.db.declaration(item).declaration {
            Declaration::Struct { fields, .. } => fields.iter().map(|field| field.ty).collect(),
            _ => Vec::new(),
        }
    }

    /// `print(arguments…, sep=…, end=…)`, its `None` result written into `place`. The call
    /// writes its arguments with nothing between them ([`Builtin::Print`]), so the separator
    /// goes between each pair of values and the line end after the last: a space and `"\n"`
    /// unless `sep=` or `end=` gives another `str`, and also when it gives `None`, as in Python.
    ///
    /// Arguments are evaluated as written, the values then the keywords, each once: a separator
    /// that is not a literal is borrowed once and passed at every gap. A literal is passed as
    /// its constant, so a backend can print a string literal without a `str` value; anything
    /// else is borrowed, since printing does not consume it.
    fn print(&mut self, place: Place, arguments: &[NodeId], node: NodeId) {
        let keywords: Vec<NodeId> = self
            .tree
            .child(node, Role::Arguments)
            .into_iter()
            .flat_map(|list| self.tree.children_with(list, Role::Item))
            .filter(|&item| self.tree.kind(item) == SyntaxKind::Keyword)
            .collect();
        let values = &arguments[..arguments.len().saturating_sub(keywords.len())];
        let values: Vec<Argument<'db>> = values
            .iter()
            .map(|&argument| self.print_argument(argument))
            .collect();
        let (mut separator, mut end) = (self.text(" "), self.text("\n"));
        for keyword in keywords {
            let (Some(name), Some(value)) = (
                self.tree.child(keyword, Role::Name),
                self.tree.child(keyword, Role::Value),
            ) else {
                return self.unsupported(keyword, "this keyword argument");
            };
            let given = if matches!(self.ty(value).kind(self.db), TypeKind::None) {
                if !self.is_literal(value) {
                    self.borrow(value);
                }
                None
            } else {
                Some(self.print_argument(value))
            };
            match (self.text_of(name), given) {
                ("sep", Some(given)) => separator = given,
                ("end", Some(given)) => end = given,
                ("sep" | "end", None) => {}
                _ => return self.unsupported(keyword, "this keyword argument of `print`"),
            }
        }
        let mut passed = Vec::with_capacity(2 * values.len() + 1);
        for (position, value) in values.into_iter().enumerate() {
            if position > 0 {
                passed.push(separator.clone());
            }
            passed.push(value);
        }
        passed.push(end);
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

    /// One value `print` writes: a literal as its constant, anything else borrowed.
    fn print_argument(&mut self, argument: NodeId) -> Argument<'db> {
        if self.is_literal(argument) {
            Argument::Owned(Operand::Constant(self.literal(argument)))
        } else {
            Argument::Borrow(self.borrow(argument))
        }
    }

    /// The `str` constant `text`, passed `Owned`, as `print` takes its separator and line end.
    fn text(&self, text: &str) -> Argument<'db> {
        Argument::Owned(Operand::Constant(Constant {
            ty: types::Type::new(self.db, TypeKind::Str),
            value: ConstantValue::Str(text.to_owned()),
        }))
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
