//! Calls: to functions, methods, struct constructors, aliases of `Fixed` vectors, number
//! conversions and the language's built-in functions.

use super::members::Member;
use super::{Body, numbers, relations};
use crate::outline::{Base, Kind, outline};
use database::{Definition, File};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Declaration, Integer, Item, Nominal, Parameter, Primitive, Signature, Type, TypeKind};

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
    fn arguments(&self, call: NodeId) -> Arguments {
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
                self.check_call(node, callee, &signature, 0, arguments)
            }
            (Kind::Class(class), Declaration::Struct { fields, .. })
                if class.base != Base::Enum =>
            {
                let nominal = Nominal {
                    file,
                    name,
                    arguments: Vec::new(),
                };
                if !class.type_parameters.is_empty() {
                    // Type arguments are inferred from the fields by generic checking (1.7.5).
                    return self.untyped(arguments);
                }
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
                    type_parameters: Vec::new(),
                    parameters,
                    returns: Type::new(self.db, TypeKind::Struct(nominal)),
                };
                self.check_call(node, callee, &constructor, 0, arguments)
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
                self.check_call(node, callee, &signature, 0, arguments)
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
        if let Some(result) = self.container_method(node, base, &callee, arguments) {
            return result;
        }
        let TypeKind::Struct(nominal) = base.kind(self.db) else {
            return self.untyped(arguments);
        };
        let nominal = nominal.clone();
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
                self.check_call(node, &callee, &signature, 1, arguments)
            }
            Some(Member::Field(ty)) => self.call_value(node, &callee, ty, arguments),
            None => self.untyped(arguments),
        }
    }

    /// Checks a call's arguments against `signature`, whose first `skip` parameters are
    /// already given (a method's `self`), and returns the call's type.
    fn check_call(
        &mut self,
        node: NodeId,
        callee: &str,
        signature: &Signature<'db>,
        skip: usize,
        arguments: &Arguments,
    ) -> Type<'db> {
        let parameters = signature.parameters.get(skip..).unwrap_or_default();
        let generic = !signature.type_parameters.is_empty();
        let mut given = vec![false; parameters.len()];
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
        for (index, &argument) in arguments.positional.iter().enumerate() {
            given[index] = true;
            self.argument(argument, &parameters[index], generic);
        }
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
                    self.argument(value, &parameters[index], generic);
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
        if generic {
            // Type arguments are inferred at calls by generic checking (1.7.5).
            return self.lowerer.unknown();
        }
        signature.returns
    }

    /// Checks one argument against its parameter. A generic function's parameters are
    /// checked by a later slice.
    fn argument(&mut self, argument: NodeId, parameter: &Parameter<'db>, generic: bool) {
        let expected = (!generic).then_some(parameter.ty);
        let found = self.expression(argument, expected);
        if let Some(expected) = expected
            && !relations::compatible(self.db, expected, found)
        {
            let message = format!(
                "`{}` has type `{}`, found `{}`",
                parameter.name,
                self.show(expected),
                self.show(found)
            );
            let label = format!("type `{}`", self.show(found));
            self.lowerer.error(argument, &message, &label, None);
        }
    }

    /// A call to one of the language's names: a number conversion such as `f64(n)`, or a
    /// built-in function.
    fn builtin(&mut self, node: NodeId, name: &str, arguments: &Arguments) -> Type<'db> {
        let all: Vec<NodeId> = arguments
            .positional
            .iter()
            .copied()
            .chain(arguments.keywords.iter().map(|&(_, value)| value))
            .collect();
        let types: Vec<Type<'db>> = all
            .iter()
            .map(|&argument| self.expression(argument, None))
            .collect();
        let primitive = |db, kind| Type::new(db, TypeKind::Primitive(kind));
        if let Some(target) = Primitive::from_name(name) {
            if types.len() != 1 {
                let message = format!("`{name}(...)` converts one value, found {}", types.len());
                return self
                    .lowerer
                    .error(node, &message, "wrong number of arguments", None);
            }
            return primitive(self.db, target);
        }
        let int = primitive(self.db, Primitive::Integer(Integer::I64));
        match (name, types.as_slice()) {
            ("str", [_]) => Type::new(self.db, TypeKind::Str),
            ("print", _) => Type::new(self.db, TypeKind::None),
            ("len", [_]) => int,
            ("round", [value]) if self.is_number(*value) => int,
            ("abs", [value]) | ("round", [value, _]) => *value,
            ("min" | "max", [first, rest @ ..]) if !rest.is_empty() => {
                let mut result = *first;
                for (&ty, &argument) in rest.iter().zip(&all[1..]) {
                    result = match (result.kind(self.db), ty.kind(self.db)) {
                        (TypeKind::Primitive(a), TypeKind::Primitive(b)) => {
                            match numbers::common(*a, *b) {
                                Some(common) => primitive(self.db, common),
                                None => self.mixing_error(argument, name, *a, *b),
                            }
                        }
                        _ => self.lowerer.unknown(),
                    };
                }
                result
            }
            _ => self.lowerer.unknown(),
        }
    }

    /// Whether `ty` is a number type.
    fn is_number(&self, ty: Type<'db>) -> bool {
        matches!(ty.kind(self.db), TypeKind::Primitive(primitive) if numbers::is_number(*primitive))
    }

    /// The element type of a `for` loop over `range`, `prange`, `enumerate` or `zip`, or
    /// `None` when the iterator is not a call to one of them.
    pub(super) fn builtin_iteration(&mut self, iterator: NodeId) -> Option<Type<'db>> {
        if self.tree.kind(iterator) != SyntaxKind::Call {
            return None;
        }
        let function = self.tree.child(iterator, Role::Function)?;
        let Some(Definition::Builtin(name)) = self.lowerer.definition(function) else {
            return None;
        };
        let arguments = self.arguments(iterator);
        let int = Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::I64)),
        );
        match *name {
            "range" | "prange" => {
                let mut element: Option<Type<'db>> = None;
                for &argument in &arguments.positional {
                    let found = self.expression(argument, Some(element.unwrap_or(int)));
                    let is_integer = matches!(
                        found.kind(self.db),
                        TypeKind::Primitive(Primitive::Integer(_))
                    );
                    if !is_integer && !relations::is_open(self.db, found) {
                        let message =
                            format!("`{name}` counts in integers, found `{}`", self.show(found));
                        self.lowerer
                            .error(argument, &message, "not an integer", None);
                    } else if is_integer && element.is_none() {
                        element = Some(found);
                    }
                }
                for &(_, value) in &arguments.keywords {
                    self.expression(value, None);
                }
                Some(element.unwrap_or(int))
            }
            "enumerate" => {
                let element = match arguments.positional.first() {
                    Some(&sequence) => self.iteration(sequence),
                    None => self.lowerer.unknown(),
                };
                Some(Type::new(self.db, TypeKind::Tuple(vec![int, element])))
            }
            "zip" => {
                let elements = arguments
                    .positional
                    .iter()
                    .map(|&sequence| self.iteration(sequence))
                    .collect();
                Some(Type::new(self.db, TypeKind::Tuple(elements)))
            }
            _ => None,
        }
    }
}
