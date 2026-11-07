//! Calls: to functions, methods, struct constructors, aliases of `Fixed` vectors, number
//! conversions and the language's built-in functions.

use super::exclusive::Passed;
use super::members::Member;
use super::{Body, relations};
use crate::outline::{Base, Kind, outline};
use database::{Definition, File};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Declaration, Item, Nominal, Parameter, Signature, Type, TypeKind};

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
        match resolved {
            Some(&Definition::Item(file, name)) => {
                self.call_item(node, &callee, file, name, &arguments)
            }
            Some(Definition::Builtin("Array")) => self.array_from_list(node, &arguments, expected),
            Some(Definition::Builtin(builtin)) => self.builtin(node, builtin, &arguments),
            Some(Definition::Local(_)) => {
                let ty = self.expression(function, None);
                self.call_value(node, &callee, ty, &arguments)
            }
            None if self.tree.kind(function) == SyntaxKind::Attribute => {
                self.method_call(node, function, &arguments, expected)
            }
            Some(_) | None => self.untyped(&arguments),
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
    fn call_item(
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
            (Kind::Function, Declaration::Function(signature)) => {
                self.check_call(node, callee, &signature, None, arguments)
            }
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
                        mode: types::Mode::Owned,
                        default: false,
                    })
                    .collect();
                let constructor = Signature {
                    type_parameters,
                    parameters,
                    returns: Type::new(self.db, TypeKind::Struct(nominal)),
                };
                self.check_call(node, callee, &constructor, None, arguments)
            }
            (Kind::Alias, Declaration::Alias(ty)) => self.construct(node, callee, ty, arguments),
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
    fn call_value(
        &mut self,
        node: NodeId,
        callee: &str,
        ty: Type<'db>,
        arguments: &Arguments,
    ) -> Type<'db> {
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
                            mode: types::Mode::Borrow,
                            default: false,
                        })
                        .collect(),
                    returns: *returns,
                };
                self.check_call(node, callee, &signature, None, arguments)
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

    /// `value.method(arguments)` on a struct.
    fn method_call(
        &mut self,
        node: NodeId,
        function: NodeId,
        arguments: &Arguments,
        expected: Option<Type<'db>>,
    ) -> Type<'db> {
        let (Some(value), Some(attribute)) = (
            self.tree.child(function, Role::Value),
            self.tree.child(function, Role::Attribute),
        ) else {
            return self.untyped(arguments);
        };
        let callee = self.tree.text(attribute, self.text).to_owned();
        match self.lowerer.definition(value) {
            Some(Definition::Builtin("Array")) => {
                return self.array_function(node, &callee, arguments, expected);
            }
            Some(Definition::Item(..)) => {
                // `Vec3.zeros()` on an alias of a `Fixed` type; anything else on a class has no
                // functions in version 0.
                let ty = self.lowerer.lower(value);
                return match ty.kind(self.db) {
                    TypeKind::Fixed { .. } => self.fixed_function(node, ty, &callee, arguments),
                    _ => self.untyped(arguments),
                };
            }
            _ => {}
        }
        let base = self.expression(value, None);
        if self.maybe_none(value, base) {
            self.untyped(arguments);
            return self.lowerer.unknown();
        }
        if let Some(result) = self.container_method(node, base, &callee, arguments) {
            if matches!(base.kind(self.db), TypeKind::List(_))
                && matches!(callee.as_str(), "append" | "pop")
            {
                self.change(value);
            }
            return result;
        }
        let copy = Signature {
            type_parameters: Vec::new(),
            parameters: Vec::new(),
            returns: base,
        };
        let nominal = match base.kind(self.db) {
            TypeKind::Str if callee == "copy" => {
                return self.check_call(node, &callee, &copy, None, arguments);
            }
            TypeKind::Struct(nominal) => nominal.clone(),
            _ => return self.untyped(arguments),
        };
        match self.member(function, &nominal, base, attribute) {
            Some(Member::Method {
                item,
                parameters,
                arguments: type_arguments,
            }) => {
                let Declaration::Function(signature) = &self.db.declaration(item).declaration
                else {
                    return self.untyped(arguments);
                };
                let substitute = |ty| {
                    relations::substitute(self.db, ty, nominal.file, &parameters, &type_arguments)
                };
                let signature = Signature {
                    type_parameters: signature.type_parameters.clone(),
                    parameters: signature
                        .parameters
                        .iter()
                        .map(|parameter| Parameter {
                            ty: substitute(parameter.ty),
                            ..parameter.clone()
                        })
                        .collect(),
                    returns: substitute(signature.returns),
                };
                let receiver = signature.parameters.first().map(|parameter| Passed {
                    node: value,
                    mode: parameter.mode,
                    ty: base,
                });
                self.check_call(node, &callee, &signature, receiver, arguments)
            }
            Some(Member::Field(ty)) => self.call_value(node, &callee, ty, arguments),
            Some(Member::Copy) => self.check_call(node, &callee, &copy, None, arguments),
            None => self.untyped(arguments),
        }
    }

    /// Checks a call's arguments against `signature`, and returns the call's type. A method
    /// call's `receiver` is already checked, and given for the first parameter, `self`. A
    /// generic signature's type arguments are inferred from the arguments. Then the
    /// arguments, the receiver among them, are checked for exclusivity (the `exclusive`
    /// module).
    fn check_call(
        &mut self,
        node: NodeId,
        callee: &str,
        signature: &Signature<'db>,
        receiver: Option<Passed<'db>>,
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
        let (returns, found) = if signature.type_parameters.is_empty() {
            let found = pairs
                .iter()
                .map(|&(argument, index)| self.argument(argument, &parameters[index]))
                .collect();
            (signature.returns, found)
        } else {
            self.generic_call(node, callee, signature, parameters, &pairs)
        };
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
        }
        found
    }
}
