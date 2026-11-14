//! Calls: to functions, struct constructors, aliases of `Fixed` vectors, number conversions
//! and the language's built-in functions. Method calls are in the `methods` module.

use super::exclusive::Passed;
use super::{Body, relations};
use crate::outline::{Base, Kind, outline};
use database::{Definition, File};
use syntax::{NodeId, Role, SyntaxKind};
use types::{
    Call, Callee, Declaration, Item, Key, Mode, Nominal, Parameter, Signature, Type, TypeKind,
};

/// The arguments of one call, as written.
pub(super) struct Arguments {
    /// The positional arguments, in order.
    pub(super) positional: Vec<NodeId>,
    /// The keyword arguments: each `Keyword` node's `Name` and `Value`.
    pub(super) keywords: Vec<(NodeId, NodeId)>,
}

impl<'db> Body<'db> {
    /// The type of a call, after checking its arguments. `expected` is the type the context
    /// needs, which gives `Array.zeros(n)` its element type.
    pub(super) fn call(&mut self, node: NodeId, expected: Option<Type<'db>>) -> Type<'db> {
        let arguments = self.arguments(node);
        let Some(function) = self.tree.child(node, Role::Function) else {
            return self.untyped(&arguments);
        };
        let resolved = match self.tree.kind(function) {
            SyntaxKind::Name => self.lowerer.definition(function),
            SyntaxKind::Attribute => self
                .tree
                .child(function, Role::Attribute)
                .and_then(|attribute| self.lowerer.definition(attribute)),
            _ => None,
        };
        let callee = self.tree.text(function, self.text).to_owned();
        let ty = match resolved {
            Some(&Definition::Item(file, name)) if self.is_path_class(file, name) => {
                self.path_new(node, &arguments)
            }
            Some(&Definition::Item(file, name)) if self.is_dataset_class(file, name) => {
                self.netcdf_call(node, "Dataset", &arguments)
            }
            Some(&Definition::Item(file, name)) if self.is_h5by_file_class(file, name) => {
                self.h5by_call(node, "File", &arguments)
            }
            Some(&Definition::Item(file, name)) => {
                let (file, name) = self.text_mode_open(file, name, &arguments);
                self.call_item(node, &callee, file, name, &arguments)
            }
            Some(Definition::Builtin("Array")) => self.array_from_list(node, &arguments, expected),
            Some(Definition::Builtin("DataFrame")) => self.frame_new(node, &arguments, expected),
            Some(Definition::Builtin("open")) => {
                let ty = self.open(node, &arguments);
                self.record_open(node, &arguments, ty);
                ty
            }
            Some(Definition::Builtin(builtin)) => self.builtin(node, builtin, &arguments),
            Some(Definition::Standard {
                module: "math",
                member: Some(member),
            }) if member == "fsum" => self.fsum(node, &arguments),
            Some(Definition::Standard {
                module: "math",
                member: Some(member),
            }) => {
                let member = member.clone();
                match self.math(node, &member, &arguments) {
                    Some(ty) => ty,
                    None => self.untyped(&arguments),
                }
            }
            Some(Definition::Standard {
                module: "random",
                member: Some(member),
            }) if member == "Philox" => self.philox(node, &arguments),
            Some(Definition::Standard {
                module: module @ ("sys" | "os"),
                member: Some(member),
            }) => {
                let (module, member) = (*module, member.clone());
                self.process_call(node, module, &member, &arguments)
            }
            Some(Definition::Standard {
                module: module @ ("fits" | "netCDF4" | "h5by" | "subprocess"),
                member: Some(member),
            }) => {
                let (module, member) = (*module, member.clone());
                self.module_call(node, (module, &member), &arguments, expected)
            }
            Some(Definition::Local(_)) => {
                let ty = self.expression(function, None);
                self.call_value(node, &callee, ty, &arguments)
            }
            None if self.environ_value(function) => {
                let method = self.tree.child(function, Role::Attribute);
                let method = method.map_or("", |method| self.tree.text(method, self.text));
                let method = method.to_owned();
                let ty = self.environ_method(node, &method, &arguments);
                self.record_environ(node, &method, &arguments, ty);
                ty
            }
            None if self.tree.kind(function) == SyntaxKind::Attribute => {
                let grouped = self.grouped_frame(function);
                let grouped = grouped.and_then(|frame| self.frame_agg(node, frame, &arguments));
                grouped.unwrap_or_else(|| self.method_call(node, function, &arguments, expected))
            }
            Some(_) | None => self.untyped(&arguments),
        };
        if !self.typed.calls.contains_key(&Key(node)) {
            self.record_builtin(node, function, &arguments, ty);
        }
        ty
    }

    /// The call `node` of `module.member(arguments)`, where `module` is one of the standard
    /// modules whose calls the checker types itself: `fits` (decision 0044), `netCDF4`
    /// (decision 0047), `h5by` (decision 0052) or `subprocess` (decision 0036). `expected` is
    /// the type the context needs, which types `fits.getdata`.
    fn module_call(
        &mut self,
        node: NodeId,
        (module, member): (&str, &str),
        arguments: &Arguments,
        expected: Option<Type<'db>>,
    ) -> Type<'db> {
        match module {
            "fits" => self.fits_call(node, member, arguments, expected),
            "netCDF4" => self.netcdf_call(node, member, arguments),
            "h5by" => self.h5by_call(node, member, arguments),
            _ => self.subprocess_call(node, member, arguments),
        }
    }

    /// The arguments of a call, split into positional and keyword.
    pub(super) fn arguments(&self, call: NodeId) -> Arguments {
        let mut arguments = Arguments {
            positional: Vec::new(),
            keywords: Vec::new(),
        };
        let items = self
            .tree
            .child(call, Role::Arguments)
            .into_iter()
            .flat_map(|list| self.tree.children_with(list, Role::Item));
        for item in items {
            if self.tree.kind(item) == SyntaxKind::Keyword {
                if let (Some(name), Some(value)) = (
                    self.tree.child(item, Role::Name),
                    self.tree.child(item, Role::Value),
                ) {
                    arguments.keywords.push((name, value));
                }
            } else {
                arguments.positional.push(item);
            }
        }
        arguments
    }

    /// Types every argument with no expectation, for a call this slice cannot check, and gives
    /// the call an unknown type.
    pub(super) fn untyped(&mut self, arguments: &Arguments) -> Type<'db> {
        for &argument in &arguments.positional {
            self.expression(argument, None);
        }
        for &(_, value) in &arguments.keywords {
            self.expression(value, None);
        }
        self.lowerer.unknown()
    }

    /// A call to a module-level definition: a function, a struct's constructor, or an alias.
    pub(super) fn call_item(
        &mut self,
        node: NodeId,
        callee: &str,
        file: File,
        name: NodeId,
        arguments: &Arguments,
    ) -> Type<'db> {
        let Some(kind) = outline(self.db, file)
            .get(name)
            .map(|entry| entry.kind.clone())
        else {
            return self.untyped(arguments);
        };
        let item = Item::new(self.db, self.lowerer.package, file, name);
        let declaration = self.db.declaration(item).declaration.clone();
        match (kind, declaration) {
            (Kind::Function, Declaration::Function(signature)) => self.check_call(
                node,
                callee,
                &signature,
                None,
                Callee::Function(item),
                arguments,
            ),
            (Kind::Class(class), Declaration::Struct { fields, .. })
                if class.base != Base::Enum =>
            {
                let type_parameters: Vec<Type<'db>> = class
                    .type_parameters
                    .iter()
                    .map(|&parameter| {
                        Type::new(
                            self.db,
                            TypeKind::Parameter {
                                file,
                                name: parameter,
                            },
                        )
                    })
                    .collect();
                let nominal = Nominal {
                    file,
                    name,
                    arguments: type_parameters.clone(),
                };
                let parameters = fields
                    .into_iter()
                    .map(|field| Parameter {
                        name: field.name,
                        ty: field.ty,
                        mode: Mode::Owned,
                        default: false,
                    })
                    .collect();
                let constructor = Signature {
                    type_parameters,
                    parameters,
                    returns: Type::new(self.db, TypeKind::Struct(nominal)),
                };
                self.check_call(
                    node,
                    callee,
                    &constructor,
                    None,
                    Callee::Constructor(item),
                    arguments,
                )
            }
            (Kind::Alias, Declaration::Alias(ty)) => self.construct(node, callee, ty, arguments),
            (Kind::Class(_), Declaration::Trait { .. }) => {
                self.untyped(arguments);
                let message = format!("`{callee}` is a trait, which cannot be built");
                self.lowerer.error(
                    node,
                    &message,
                    "not a struct",
                    Some("build a struct that implements it instead"),
                )
            }
            (Kind::Constant, _) => {
                self.untyped(arguments);
                let message = format!("`{callee}` is a constant, which cannot be called");
                self.lowerer.error(node, &message, "not a function", None)
            }
            _ => self.untyped(arguments),
        }
    }

    /// A call to a type an alias names: `Vec3(1.0, 0.0, 0.0)` builds a `Fixed` vector from
    /// its elements, and an alias of a struct builds the struct.
    fn construct(
        &mut self,
        node: NodeId,
        callee: &str,
        ty: Type<'db>,
        arguments: &Arguments,
    ) -> Type<'db> {
        match ty.kind(self.db) {
            TypeKind::Fixed { element, shape } => {
                let (element, shape) = (*element, shape.clone());
                for &(_, value) in &arguments.keywords {
                    self.expression(value, None);
                }
                let matched = arguments.keywords.is_empty()
                    && self.fixed_elements(&arguments.positional, element, shape.lengths());
                if !matched {
                    let message = format!("`{callee}` needs its elements in the shape `{shape}`");
                    let hint = if shape.rank() == 1 {
                        format!(
                            "for example `{callee}({})`",
                            vec!["0.0"; usize::try_from(shape.elements()).unwrap_or(0).min(4)]
                                .join(", ")
                        )
                    } else {
                        "write each row as a tuple: `Mat3((1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0))`".to_owned()
                    };
                    return self.lowerer.error(
                        node,
                        &message,
                        "wrong number of elements",
                        Some(&hint),
                    );
                }
                ty
            }
            _ => self.untyped(arguments),
        }
    }

    /// A call to a value, such as a `Callable` parameter.
    pub(super) fn call_value(
        &mut self,
        node: NodeId,
        callee: &str,
        ty: Type<'db>,
        arguments: &Arguments,
    ) -> Type<'db> {
        if let TypeKind::Function { callable, .. } = ty.kind(self.db) {
            return self.call_value(node, callee, *callable, arguments);
        }
        match ty.kind(self.db) {
            TypeKind::Callable {
                parameters,
                returns,
            } => {
                let signature = Signature {
                    type_parameters: Vec::new(),
                    parameters: parameters
                        .iter()
                        .enumerate()
                        .map(|(index, &ty)| Parameter {
                            name: format!("argument {}", index + 1),
                            ty,
                            mode: Mode::Borrow,
                            default: false,
                        })
                        .collect(),
                    returns: *returns,
                };
                self.check_call(node, callee, &signature, None, Callee::Value, arguments)
            }
            _ if relations::is_open(self.db, ty) || !self.is_value_scalar(ty) => {
                self.untyped(arguments)
            }
            _ => {
                self.untyped(arguments);
                let message = format!(
                    "`{callee}` has type `{}`, which cannot be called",
                    self.show(ty)
                );
                self.lowerer.error(node, &message, "not a function", None)
            }
        }
    }

    /// Whether `ty` is a number, `bool`, `str` or `None`.
    fn is_value_scalar(&self, ty: Type<'db>) -> bool {
        matches!(
            ty.kind(self.db),
            TypeKind::Primitive(_) | TypeKind::Str | TypeKind::None
        )
    }

    /// Checks a call's arguments against `signature`, and returns the call's type. A method
    /// call's `receiver` is already checked, and given for the first parameter, `self`. A
    /// generic signature's type arguments are inferred from the arguments. Then the
    /// arguments, the receiver among them, are checked for exclusivity (the `exclusive`
    /// module), and the call is recorded as calling `target` for the stages after the checker.
    pub(super) fn check_call(
        &mut self,
        node: NodeId,
        callee: &str,
        signature: &Signature<'db>,
        receiver: Option<Passed<'db>>,
        target: Callee<'db>,
        arguments: &Arguments,
    ) -> Type<'db> {
        let skip = usize::from(receiver.is_some());
        let parameters = signature.parameters.get(skip..).unwrap_or_default();
        if arguments.positional.len() > parameters.len() {
            let message = format!(
                "`{callee}` takes {} argument{}, found {}",
                parameters.len(),
                if parameters.len() == 1 { "" } else { "s" },
                arguments.positional.len() + arguments.keywords.len()
            );
            self.untyped(arguments);
            return self
                .lowerer
                .error(node, &message, "too many arguments", None);
        }
        let mut pairs: Vec<(NodeId, usize)> =
            arguments.positional.iter().copied().zip(0..).collect();
        let mut given = vec![false; parameters.len()];
        given[..pairs.len()].fill(true);
        for &(name, value) in &arguments.keywords {
            let written = self.tree.text(name, self.text);
            match parameters
                .iter()
                .position(|parameter| parameter.name == written)
            {
                Some(index) if given[index] => {
                    self.expression(value, None);
                    let message = format!("`{written}` is given twice");
                    self.lowerer
                        .error(name, &message, "already given by position", None);
                }
                Some(index) => {
                    given[index] = true;
                    pairs.push((value, index));
                }
                None => {
                    self.expression(value, None);
                    let message = format!("`{callee}` has no parameter `{written}`");
                    self.lowerer
                        .error(name, &message, "unknown parameter", None);
                }
            }
        }
        let missing: Vec<String> = parameters
            .iter()
            .zip(&given)
            .filter(|(parameter, given)| !**given && !parameter.default)
            .map(|(parameter, _)| format!("`{}`", parameter.name))
            .collect();
        if !missing.is_empty() {
            let plural = if missing.len() == 1 { "" } else { "s" };
            let message = format!(
                "`{callee}` is missing the argument{plural} {}",
                missing.join(", ")
            );
            self.lowerer
                .error(node, &message, "missing arguments", None);
        }
        let (returns, found, type_arguments) = if signature.type_parameters.is_empty() {
            let found = pairs
                .iter()
                .map(|&(argument, index)| self.argument(argument, &parameters[index]))
                .collect();
            (signature.returns, found, Vec::new())
        } else {
            self.generic_call(node, callee, signature, parameters, &pairs)
        };
        let mut slots = vec![None; signature.parameters.len()];
        for (slot, argument) in receiver
            .as_ref()
            .map(|receiver| (0, receiver.node))
            .into_iter()
            .chain(
                pairs
                    .iter()
                    .map(|&(argument, index)| (index + skip, argument)),
            )
        {
            if let Some(slot) = slots.get_mut(slot) {
                *slot = Some(Key(argument));
            }
        }
        self.typed.calls.insert(
            Key(node),
            Call {
                callee: target,
                type_arguments,
                arguments: slots,
            },
        );
        let passed: Vec<Passed<'db>> = receiver
            .into_iter()
            .chain(pairs.iter().zip(found).map(|(&(node, index), ty)| Passed {
                node,
                mode: parameters[index].mode,
                ty,
            }))
            .collect();
        self.pass(&passed);
        returns
    }

    /// Checks one argument against its parameter, and returns the argument's type.
    fn argument(&mut self, argument: NodeId, parameter: &Parameter<'db>) -> Type<'db> {
        let found = self.expression(argument, Some(parameter.ty));
        if !relations::compatible(self.db, parameter.ty, found)
            && !self.unchecked(argument, parameter.ty, found)
        {
            let message = format!(
                "`{}` has type `{}`, found `{}`",
                parameter.name,
                self.show(parameter.ty),
                self.show(found)
            );
            let label = format!("type `{}`", self.show(found));
            self.lowerer.error(argument, &message, &label, None);
        } else if parameter.mode == Mode::InOut {
            self.changed_exactly(argument, parameter, found);
        }
        found
    }
}
