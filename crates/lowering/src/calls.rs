//! Calls: of functions and methods, with each argument in its parameter's mode; of a struct's
//! constructor; of a number type, which converts; and of the built-ins lowering handles.
//! `print` is lowered with the streams it writes to (the `streams` module).

use crate::builder::Builder;
use crate::frames::erase;
use mir::{Argument, Callee, Operand, Place, Projection, Rvalue, TerminatorKind};
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
        if matches!(&call.callee, types::Callee::Builtin(name) if name == "subprocess.run") {
            return self.subprocess_run(place, &given, node);
        }
        if matches!(&call.callee, types::Callee::Builtin(name) if name == "Array.fromfile") {
            return self.array_fromfile(place, &given, node);
        }
        if let types::Callee::Builtin(name) = &call.callee
            && name.starts_with("fits.")
        {
            let name = name.clone();
            return self.fits_call(place, &name, &given, node);
        }
        if let types::Callee::Builtin(name) = &call.callee
            && name.starts_with("netCDF4.")
        {
            let name = name.clone();
            return self.netcdf_call(place, &name, &given, node);
        }
        if let types::Callee::Builtin(name) = &call.callee
            && name.starts_with("h5by.")
        {
            let name = name.clone();
            return self.h5by_call(place, &name, &given, node);
        }
        if let types::Callee::Builtin(name) = &call.callee
            && let ("Array.load" | "Array.save", Some(arguments)) = (
                name.as_str(),
                given.iter().copied().collect::<Option<Vec<_>>>(),
            )
        {
            return self.array_npy(place, name, &arguments, node);
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
                    if matches!(self.ty(*argument).kind(self.db), TypeKind::Str) =>
                {
                    self.parse_number(place, *to, *argument, node);
                }
                (TypeKind::Primitive(_), [argument])
                    if !matches!(self.ty(*argument).kind(self.db), TypeKind::Primitive(_)) =>
                {
                    let from = self.ty(*argument).display(self.db);
                    let to = to.display(self.db);
                    self.unsupported(node, &format!("converting a `{from}` to `{to}`"));
                }
                (TypeKind::Primitive(_), [real, imaginary]) if self.is_complex(*to) => {
                    self.complex_from_parts(&place, *to, (*real, *imaginary), node);
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
            types::Callee::Builtin(name) => self.named_builtin(place, name, &arguments, node),
            types::Callee::Value => self.value_call(place, &arguments, node),
        }
    }

    /// The call `node` of the built-in `name`, such as `len` or `list.append`, with
    /// `arguments`, the receiver first for a method, its result written into `place`.
    fn named_builtin(&mut self, place: Place, name: &str, arguments: &[NodeId], node: NodeId) {
        let ty = self.ty(node);
        match (name.split_once('.'), arguments) {
            (None, [argument]) if name == "len" => self.len(place, *argument, node),
            (None, [value]) if name == "round" => self.round(place, *value, node),
            (None, [value, digits]) if name == "round" => {
                self.round_digits(place, (*value, *digits), node);
            }
            (None, [list, rest @ ..]) if name == "sorted" && rest.len() <= 1 => {
                self.sorted(&place, *list, rest.first().copied(), node);
            }
            (None, [argument]) if name == "abs" => self.absolute(place, *argument, node),
            (None, _) if name == "print" => self.print(place, arguments, node),
            (None, [list]) if name == "Array" && self.is_text_array(node) => {
                self.text_array_from_list(&place, *list, node);
            }
            (None, [list]) if name == "Array" => self.array_from_list(&place, *list, node),
            (None, [table])
                if name == "DataFrame" && self.tree.kind(*table) == SyntaxKind::Dictionary =>
            {
                self.frame_new(&place, *table, node);
            }
            (None, [rows]) if name == "DataFrame" => self.frame_from_rows(place, *rows, node),
            (None, [list]) if name == "concat" => self.frame_concat(place, *list, node),
            (Some(("DataFrame", method)), [frame, rest @ ..]) => {
                self.frame_method(place, method, (*frame, rest), node);
            }
            (None, _) if name == "open" => self.open(place, arguments, node),
            (None, [value]) if name == "str" => {
                if !self.text_of_value(&place, *value, node) {
                    self.unsupported(node, "the built-in `str`");
                }
            }
            (Some(("pathlib", "Path")), _) => self.path_new(place, arguments, node),
            (None, [] | [_]) if name == "input" => {
                self.input(place, arguments.first().copied(), node);
            }
            (None, [values, rest @ ..]) if self.reduces(name, (*values, rest), node) => {
                self.reduce(place, name, (*values, rest), node);
            }
            (Some((owner @ ("Array" | "Fixed"), function @ ("zeros" | "ones" | "fill"))), _)
                if !matches!(ty.kind(self.db), TypeKind::None) =>
            {
                if owner == "Array" {
                    self.new_array(&place, function, arguments, node);
                } else {
                    self.new_fixed(place, function, arguments, node);
                }
            }
            (Some(("str", method)), _) => self.text_method(place, method, arguments, node),
            (Some(("complex", "conjugate")), [value]) => self.conjugate(place, *value, node),
            (Some(("list", "append")), [list, value]) => {
                self.list_append(place, *list, *value, node);
            }
            (Some(("list", "copy")), [list]) => self.list_copy(&place, *list, node),
            // A text array is two arrays in a tuple, as a frame is its columns.
            (Some(("Array", "copy")), [array]) if self.is_text_array(*array) => {
                self.frame_copy(place, *array, node);
            }
            (Some(("list", "pop")), [list]) => self.list_pop(place, *list, node),
            (Some(("dict", method)), _) => self.dict_method(place, method, arguments, node),
            (Some(("Array" | "Fixed", method)), _) => {
                self.array_method(place, method, arguments, node);
            }
            (Some(("math", member)), _) => self.math_function(place, member, arguments, node),
            (Some((owner @ ("random" | "Philox" | "sys" | "os" | "File" | "Path"), member)), _) => {
                self.standard_call(place, (owner, member), arguments, node);
            }
            _ => self.unsupported(node, &format!("the built-in `{name}`")),
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
            .map(|parameter| (parameter.mode, erase(self.db, parameter.ty)))
            .collect();
        let type_arguments: Vec<_> = type_arguments
            .into_iter()
            .map(|argument| erase(self.db, argument))
            .collect();
        // A method of a generic class is generic in the class's type parameters, which come
        // first: the receiver's type arguments (roadmap 1.9.18).
        let mut type_arguments = match self.class_arguments(item, arguments, node) {
            Some(class) => class.into_iter().chain(type_arguments).collect::<Vec<_>>(),
            None => return self.unsupported(node, "a method call on this receiver"),
        };
        // Each argument is moved, converted or borrowed at the parameter's type with this
        // call's type arguments in place; in a generic caller they may be its own type
        // parameters (the `generics` module).
        let generic = self.type_parameters(item, signature);
        let parameters: Vec<(Mode, types::Type<'db>)> = parameters
            .into_iter()
            .map(|(mode, ty)| match ty.kind(self.db) {
                TypeKind::Callable { .. } => (mode, ty),
                _ => (mode, self.instantiate(ty, &generic, &type_arguments)),
            })
            .collect();
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
        let mut views = Vec::new();
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
                Mode::Borrow => Argument::Borrow(self.borrow_as(argument, ty)),
                Mode::InOut => match self.changed_place(argument) {
                    Some(place) => {
                        let (passed, view) = self.in_out(place, ty, argument);
                        views.extend(view);
                        passed
                    }
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
        let call = (passed, place, next, edge.clone());
        if !self.call_terminator(item, type_arguments, arguments, call, node) {
            return;
        }
        if let Some(mut edge) = edge {
            if !views.is_empty() {
                // The callee's changes reach the views whether it returns or raises.
                let dispatch = self.new_block();
                self.start(edge.target);
                self.copy_back(&views, node);
                self.terminate(TerminatorKind::Goto(dispatch), node);
                edge.target = dispatch;
            }
            self.dispatch(&edge, errors, node);
        }
        self.start(next);
        self.copy_back(&views, node);
    }

    /// The type arguments of the class that declares `item`, for the call `node` with
    /// `arguments`: `[f64]` for `p.total()` with `p: Pair[f64]`, or `[T]` inside another
    /// generic body, which monomorphization replaces. They come from the value before the dot,
    /// which for a call through the class, `Pair.both(1.5)` or `Pair[f64].both(1.5)`, the
    /// checker types as the class with its type arguments, or else from the receiver, the
    /// first of `arguments`. Empty for a function and for a method of a class with no type
    /// parameters. `None` when the class has type parameters but neither is a struct of that
    /// class, such as a method an embedded generic struct provides (decision 0031), which is
    /// not built yet.
    fn class_arguments(
        &self,
        item: Item<'db>,
        arguments: &[Option<NodeId>],
        node: NodeId,
    ) -> Option<Vec<types::Type<'db>>> {
        let tree = self.db.parse(item.file(self.db)).tree();
        let Some((_, Some(class))) = crate::definition(tree, item.name(self.db)) else {
            return Some(Vec::new());
        };
        let Some(parameters) = tree.child(class, Role::TypeParameters) else {
            return Some(Vec::new());
        };
        let count = tree.children_with(parameters, Role::Item).count();
        let before_dot = self
            .tree
            .child(node, Role::Function)
            .filter(|&function| self.tree.kind(function) == SyntaxKind::Attribute)
            .and_then(|function| self.tree.child(function, Role::Value));
        let receiver = arguments.first().copied().flatten();
        before_dot.into_iter().chain(receiver).find_map(|value| {
            match self.ty(value).kind(self.db) {
                TypeKind::Struct(nominal)
                    if nominal.file == item.file(self.db)
                        && tree.child(class, Role::Name) == Some(nominal.name)
                        && nominal.arguments.len() == count =>
                {
                    Some(nominal.arguments.clone())
                }
                _ => None,
            }
        })
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
            Mode::InOut => Argument::InOut(project(self.changed_place(argument)?)),
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

    /// A place holding `argument` as the parameter type `ty`, to borrow. An argument that
    /// changes representation, such as an `f64` passed where `f64 | None` is borrowed, is
    /// converted into a temporary first.
    ///
    /// An argument that is not a copy type, such as an `Array` passed where `Array[f64, 1] |
    /// None` is borrowed (roadmap 1.9.29), is wrapped without moving it: the place that holds
    /// it, or the temporary it is computed into and the statement drops, is copied bit for bit
    /// into the wrapper, which is never dropped. The callee only reads the wrapper, so the
    /// caller's value is freed once, by its owner, as a loop variable that borrows a tuple's
    /// item is (the `collections` module). Cost: a copy of the value's header, never of its
    /// elements.
    fn borrow_as(&mut self, argument: NodeId, ty: types::Type<'db>) -> Place {
        let from = self.ty(argument);
        if !self.converts(from, ty) {
            return self.borrow(argument);
        }
        let temporary = self.temporary(ty, argument);
        if self.is_copy(from) {
            self.write(Place::from(temporary), ty, argument);
        } else {
            let held = self.borrow(argument);
            let wrapped = self.coerce(Operand::Copy(held), from, ty);
            self.assign(Place::from(temporary), wrapped, argument);
        }
        Place::from(temporary)
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
