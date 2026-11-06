//! Lowering annotations: turning a type as written, such as `Array[Vec3, 1]` or `f64 | None`,
//! into the [`Type`] it means.
//!
//! Each name in an annotation is looked up in the file's [`Resolution`], so `Vec3` finds the
//! alias it names wherever that alias is defined, and a name that refers to nothing has already
//! been reported by the resolver: it lowers to [`TypeKind::Unknown`] without a second error.

use crate::outline::{Base, Kind, outline};
use database::{Definition, File, Package, Resolution};
use diagnostics::Diagnostic;
use syntax::{Flags, NodeId, Operator, Role, SyntaxKind, SyntaxTree};
use types::{Declaration, Item, Mode, Nominal, Primitive, Shape, Type, TypeDb, TypeKind, Value};

/// Lowers the annotations and computes the constants of one definition, collecting the errors
/// found. The `constant` module adds constant evaluation to it.
pub(crate) struct Lowerer<'db> {
    /// The database, for other definitions' declarations and other files' outlines.
    pub(crate) db: &'db dyn TypeDb,
    /// The package the definition belongs to.
    pub(crate) package: Package,
    /// The file the definition is in.
    pub(crate) file: File,
    /// That file's tree.
    pub(crate) tree: &'db SyntaxTree,
    /// That file's text, for names in messages.
    pub(crate) text: &'db str,
    /// What each name in the file refers to.
    resolution: &'db Resolution,
    /// What `Self` means: the enclosing class's type, inside a class.
    self_type: Option<Type<'db>>,
    /// The `Name` nodes of the type parameters in scope: the class's, then the function's.
    type_parameters: Vec<NodeId>,
    /// The errors found so far, in the order found.
    pub(crate) diagnostics: Vec<Diagnostic>,
}

/// The builtin types that take arguments, each with an example of how to write it.
const GENERIC_BUILTINS: [(&str, &str); 8] = [
    ("list", "list[f64]"),
    ("tuple", "tuple[f64, i64]"),
    ("Array", "Array[f64, 1]"),
    ("Fixed", "Fixed[f64, (3,)]"),
    ("Callable", "Callable[[f64], f64]"),
    ("Raises", "Raises[f64, SolverError]"),
    ("InOut", "InOut[Array[f64, 1]]"),
    ("Owned", "Owned[list[f64]]"),
];

impl<'db> Lowerer<'db> {
    /// A lowerer for a definition in `file` of `package`. `class` is the `Name` node of the
    /// enclosing class, or of the class itself, which gives `Self` its meaning; `definition`
    /// is the statement whose type parameters are in scope besides the class's.
    pub(crate) fn new(
        db: &'db dyn TypeDb,
        package: Package,
        file: File,
        class: Option<NodeId>,
        definition: NodeId,
    ) -> Self {
        let tree = db.parse(file).tree();
        let mut type_parameters = Vec::new();
        let self_type = class.map(|class| {
            let entry = outline(db, file).get(class);
            let parameters = match entry.map(|entry| &entry.kind) {
                Some(Kind::Class(class)) => class.type_parameters.clone(),
                _ => Vec::new(),
            };
            type_parameters.extend_from_slice(&parameters);
            let arguments = parameters
                .iter()
                .map(|&name| Type::new(db, TypeKind::Parameter { file, name }))
                .collect();
            let nominal = Nominal {
                file,
                name: class,
                arguments,
            };
            let enumeration = matches!(
                entry.map(|entry| &entry.kind),
                Some(Kind::Class(class)) if class.base == Base::Enum
            );
            Type::new(
                db,
                if enumeration {
                    TypeKind::Enum(nominal)
                } else {
                    TypeKind::Struct(nominal)
                },
            )
        });
        if tree.kind(definition) == SyntaxKind::FunctionDefinition {
            type_parameters.extend(crate::outline::type_parameters(tree, definition));
        }
        Self {
            db,
            package,
            file,
            tree,
            text: file.source(db).text(),
            resolution: db.resolve(package, file),
            self_type,
            type_parameters,
            diagnostics: Vec::new(),
        }
    }

    /// The type parameters of `definition`, as types, in order.
    pub(crate) fn parameter_types(&self, definition: NodeId) -> Vec<Type<'db>> {
        crate::outline::type_parameters(self.tree, definition)
            .into_iter()
            .map(|name| {
                Type::new(
                    self.db,
                    TypeKind::Parameter {
                        file: self.file,
                        name,
                    },
                )
            })
            .collect()
    }

    /// What `Self` means here, or an unknown type outside a class.
    pub(crate) fn self_type(&self) -> Type<'db> {
        self.self_type.unwrap_or_else(|| self.unknown())
    }

    /// The type of a parameter and how it is passed: `InOut[T]` and `Owned[T]` set the mode.
    pub(crate) fn parameter(&mut self, annotation: NodeId) -> (Type<'db>, Mode) {
        if self.tree.kind(annotation) == SyntaxKind::Subscript
            && let Some(value) = self.tree.child(annotation, Role::Value)
            && let Some(Definition::Builtin(name @ ("InOut" | "Owned"))) = self.definition(value)
        {
            let mode = if *name == "InOut" {
                Mode::InOut
            } else {
                Mode::Owned
            };
            let arguments = self.arguments(annotation);
            if let [inner] = arguments.as_slice() {
                return (self.lower(*inner), mode);
            }
            return (self.arity(annotation, name, 1, arguments.len()), mode);
        }
        (self.lower(annotation), Mode::Borrow)
    }

    /// The type an annotation means. Reports what is wrong with it and lowers to
    /// [`TypeKind::Unknown`] when it means no type.
    pub(crate) fn lower(&mut self, node: NodeId) -> Type<'db> {
        match self.tree.kind(node) {
            SyntaxKind::None => Type::new(self.db, TypeKind::None),
            SyntaxKind::Name => self.named(node, node, None),
            SyntaxKind::Attribute => match self.tree.child(node, Role::Attribute) {
                Some(attribute) => self.named(node, attribute, None),
                None => self.unknown(),
            },
            SyntaxKind::Subscript => match self.name_of(node) {
                Some(name) => self.named(node, name, Some(node)),
                None => self.not_a_type(node),
            },
            SyntaxKind::BinaryOperation if self.tree.node(node).operator() == Operator::BitOr => {
                self.optional(node)
            }
            SyntaxKind::Missing => self.unknown(),
            _ => self.not_a_type(node),
        }
    }

    /// `T | None` or `None | T`.
    fn optional(&mut self, node: NodeId) -> Type<'db> {
        let (Some(left), Some(right)) = (
            self.tree.child(node, Role::Left),
            self.tree.child(node, Role::Right),
        ) else {
            return self.unknown();
        };
        let value = match (self.tree.kind(left), self.tree.kind(right)) {
            (_, SyntaxKind::None) => left,
            (SyntaxKind::None, _) => right,
            _ => {
                return self.error(
                    node,
                    "a union type can only add `None` to a type",
                    "only `T | None` is a type",
                    Some(
                        "a value that may be one of several types is not supported; use \
                         separate functions, or an enum for a fixed set of choices",
                    ),
                );
            }
        };
        let value = self.lower(value);
        Type::new(self.db, TypeKind::Optional(value))
    }

    /// The `Name` node that names the type being subscripted: `Array` in `Array[f64, 1]`, or
    /// `Particle` in `physics.Particle[f64]`.
    fn name_of(&self, subscript: NodeId) -> Option<NodeId> {
        let value = self.tree.child(subscript, Role::Value)?;
        match self.tree.kind(value) {
            SyntaxKind::Name => Some(value),
            SyntaxKind::Attribute => self.tree.child(value, Role::Attribute),
            _ => None,
        }
    }

    /// What the `Name` node `name` refers to, if the resolver found it.
    pub(crate) fn definition(&self, name: NodeId) -> Option<&'db Definition> {
        let name = match self.tree.kind(name) {
            SyntaxKind::Attribute => self.tree.child(name, Role::Attribute)?,
            _ => name,
        };
        self.resolution.definition(name)
    }

    /// The type a name means. `node` is the whole annotation, where errors point; `name` the
    /// `Name` node to look up; `subscript` the subscript that gives it arguments, if any.
    fn named(&mut self, node: NodeId, name: NodeId, subscript: Option<NodeId>) -> Type<'db> {
        let written = self.tree.text(name, self.text);
        match self.resolution.definition(name) {
            None | Some(Definition::Standard { .. }) => self.unknown(),
            Some(Definition::Builtin(builtin)) => self.builtin(node, builtin, subscript),
            Some(&Definition::Item(file, item)) => self.item(node, written, file, item, subscript),
            Some(Definition::Local(local)) if self.type_parameters.contains(local) => {
                if subscript.is_some() {
                    return self.no_arguments(node, written);
                }
                Type::new(
                    self.db,
                    TypeKind::Parameter {
                        file: self.file,
                        name: *local,
                    },
                )
            }
            Some(Definition::Local(_)) => {
                let message = format!("`{written}` is a variable, not a type");
                self.error(node, &message, "expected a type", None)
            }
            Some(Definition::Module(_)) => {
                let message = format!("`{written}` is a module, not a type");
                self.error(node, &message, "expected a type", None)
            }
        }
    }

    /// The type a builtin name means.
    fn builtin(&mut self, node: NodeId, name: &str, subscript: Option<NodeId>) -> Type<'db> {
        let simple = match name {
            "str" => Some(TypeKind::Str),
            "Self" => {
                if self.self_type.is_none() {
                    return self.error(
                        node,
                        "`Self` is only a type inside a class",
                        "outside any class",
                        None,
                    );
                }
                None
            }
            _ => Primitive::from_name(name).map(TypeKind::Primitive),
        };
        if let Some(kind) = simple {
            return match subscript {
                Some(_) => self.no_arguments(node, name),
                None => Type::new(self.db, kind),
            };
        }
        if name == "Self" {
            return match subscript {
                Some(_) => self.no_arguments(node, name),
                None => self.self_type(),
            };
        }
        let Some(&(_, example)) = GENERIC_BUILTINS
            .iter()
            .find(|(generic, _)| *generic == name)
        else {
            let message = format!("`{name}` is not a type");
            let hint = match name {
                "Final" => "`Final` marks a module-level constant: `N: Final = 64`",
                "Number" | "Float" | "VectorSpace" | "Copy" => {
                    "a trait bounds a type parameter: `def f[T: Number](x: T)`"
                }
                "Struct" | "Enum" | "Exception" => "it is a base of a class, not a type",
                _ => "it is a function or a name the language provides",
            };
            return self.error(node, &message, "expected a type", Some(hint));
        };
        let Some(subscript) = subscript else {
            let message = format!("`{name}` needs type arguments");
            let hint = format!("for example `{example}`");
            return self.error(node, &message, "no type arguments", Some(&hint));
        };
        let arguments = self.arguments(subscript);
        match (name, arguments.as_slice()) {
            ("list", &[item]) => {
                let item = self.lower(item);
                Type::new(self.db, TypeKind::List(item))
            }
            ("tuple", items) => {
                let items = items.iter().map(|&item| self.lower(item)).collect();
                Type::new(self.db, TypeKind::Tuple(items))
            }
            ("Array", &[element, rank]) => self.array(element, rank),
            ("Fixed", &[element, shape]) => self.fixed(element, shape),
            ("Callable", &[parameters, returns]) => self.callable(parameters, returns),
            ("Raises", &[value, errors]) => self.raises(value, errors),
            ("InOut" | "Owned", &[inner]) => {
                let message = format!("`{name}` can only wrap the type of a parameter");
                self.error(
                    node,
                    &message,
                    "not a parameter's type",
                    Some("it says how an argument is passed: `def f(out: InOut[Array[f64, 1]])`"),
                );
                self.lower(inner)
            }
            ("list" | "InOut" | "Owned", found) => self.arity(node, name, 1, found.len()),
            (_, found) => self.arity(node, name, 2, found.len()),
        }
    }

    /// `Array[element, rank]`.
    fn array(&mut self, element: NodeId, rank_node: NodeId) -> Type<'db> {
        let element = self.lower(element);
        let rank = match self.evaluate(rank_node) {
            Some(Value::Integer(rank)) if rank >= 1 => u32::try_from(rank).ok(),
            None => return self.unknown(),
            _ => None,
        };
        let Some(rank) = rank else {
            return self.error(
                rank_node,
                "an array's rank is a whole number of dimensions, at least 1",
                "not a valid rank",
                Some("for example `Array[f64, 1]` for a vector or `Array[f64, 2]` for a grid"),
            );
        };
        Type::new(self.db, TypeKind::Array { element, rank })
    }

    /// `Fixed[element, shape]`, where the shape is a tuple of constant lengths.
    fn fixed(&mut self, element: NodeId, shape: NodeId) -> Type<'db> {
        let element = self.lower(element);
        let note = "a shape is a tuple of lengths, such as `(3,)` or `(3, 3)`";
        if self.tree.kind(shape) != SyntaxKind::Tuple {
            return self.error(shape, "expected a shape", "not a tuple", Some(note));
        }
        let mut lengths = Vec::new();
        for length in self.tree.children_with(shape, Role::Item) {
            match self.evaluate(length) {
                Some(Value::Integer(value)) if value >= 0 => lengths.push(value.cast_unsigned()),
                None => return self.unknown(),
                Some(_) => {
                    return self.error(
                        length,
                        "a length is a whole number, 0 or more",
                        "not a valid length",
                        None,
                    );
                }
            }
        }
        match Shape::new(lengths) {
            Some(shape) => Type::new(self.db, TypeKind::Fixed { element, shape }),
            None => self.error(
                shape,
                "this shape is empty or has more elements than fit in memory",
                "not a valid shape",
                Some(note),
            ),
        }
    }

    /// `Callable[[parameters], returns]`.
    fn callable(&mut self, parameters: NodeId, returns: NodeId) -> Type<'db> {
        if self.tree.kind(parameters) != SyntaxKind::List {
            return self.error(
                parameters,
                "a `Callable`'s parameter types go in a list",
                "not a list",
                Some("for example `Callable[[f64, Vec3], Vec3]`"),
            );
        }
        let items: Vec<NodeId> = self.tree.children_with(parameters, Role::Item).collect();
        let parameters = items.into_iter().map(|item| self.lower(item)).collect();
        let returns = self.lower(returns);
        Type::new(
            self.db,
            TypeKind::Callable {
                parameters,
                returns,
            },
        )
    }

    /// `Raises[value, errors]`, where the errors are one error type or several joined by `|`.
    fn raises(&mut self, value: NodeId, errors: NodeId) -> Type<'db> {
        let value = self.lower(value);
        let mut nodes = Vec::new();
        self.union_members(errors, &mut nodes);
        let mut lowered = Vec::new();
        for node in nodes {
            let error = self.lower(node);
            if !self.is_error_type(error) {
                let message = format!("`{}` is not an error type", error.display(self.db));
                self.error(
                    node,
                    &message,
                    "expected an error type",
                    Some("an error type is a class with the base `Exception`"),
                );
            }
            lowered.push(error);
        }
        Type::new(
            self.db,
            TypeKind::Fallible {
                value,
                errors: lowered,
            },
        )
    }

    /// The members of `A | B | C`, in order, into `members`.
    fn union_members(&self, node: NodeId, members: &mut Vec<NodeId>) {
        if self.tree.kind(node) == SyntaxKind::BinaryOperation
            && self.tree.node(node).operator() == Operator::BitOr
        {
            for side in [Role::Left, Role::Right] {
                if let Some(side) = self.tree.child(node, side) {
                    self.union_members(side, members);
                }
            }
        } else {
            members.push(node);
        }
    }

    /// Whether `ty` is an error type, or unknown because of an error already reported.
    fn is_error_type(&self, ty: Type<'db>) -> bool {
        match ty.kind(self.db) {
            TypeKind::Unknown => true,
            TypeKind::Struct(nominal) => matches!(
                outline(self.db, nominal.file).get(nominal.name).map(|entry| &entry.kind),
                Some(Kind::Class(class)) if class.base == Base::Error
            ),
            _ => false,
        }
    }

    /// The type a module-level definition means when used as a type.
    fn item(
        &mut self,
        node: NodeId,
        written: &str,
        file: File,
        name: NodeId,
        subscript: Option<NodeId>,
    ) -> Type<'db> {
        let Some(entry) = outline(self.db, file).get(name) else {
            return self.unknown();
        };
        match &entry.kind {
            Kind::Class(class) => {
                let expected = class.type_parameters.len();
                let arguments = subscript.map(|subscript| self.arguments(subscript));
                let found = arguments.as_ref().map_or(0, Vec::len);
                if found != expected {
                    return match subscript {
                        Some(_) if expected == 0 => self.no_arguments(node, written),
                        _ => self.arity(node, written, expected, found),
                    };
                }
                let arguments = arguments
                    .unwrap_or_default()
                    .into_iter()
                    .map(|argument| self.lower(argument))
                    .collect();
                let nominal = Nominal {
                    file,
                    name,
                    arguments,
                };
                let kind = if class.base == Base::Enum {
                    TypeKind::Enum(nominal)
                } else {
                    TypeKind::Struct(nominal)
                };
                Type::new(self.db, kind)
            }
            Kind::Alias => {
                if subscript.is_some() {
                    return self.no_arguments(node, written);
                }
                let item = Item::new(self.db, self.package, file, name);
                match &self.db.declaration(item).declaration {
                    Declaration::Alias(ty) => *ty,
                    _ => self.unknown(),
                }
            }
            Kind::Constant => {
                let message = format!("`{written}` is a constant, not a type");
                self.error(node, &message, "expected a type", None)
            }
            Kind::Function => {
                let message = format!("`{written}` is a function, not a type");
                self.error(node, &message, "expected a type", None)
            }
            Kind::Unsupported => self.unknown(),
        }
    }

    /// The type arguments of a subscript: the items of `X[A, B]`, or the one index of `X[A]`.
    fn arguments(&self, subscript: NodeId) -> Vec<NodeId> {
        let Some(index) = self.tree.child(subscript, Role::Index) else {
            return Vec::new();
        };
        if self.tree.kind(index) == SyntaxKind::Tuple
            && !self.tree.node(index).flags().contains(Flags::PARENTHESIZED)
        {
            self.tree.children_with(index, Role::Item).collect()
        } else {
            vec![index]
        }
    }

    /// Reports a type given arguments it does not take.
    fn no_arguments(&mut self, node: NodeId, name: &str) -> Type<'db> {
        let message = format!("`{name}` takes no type arguments");
        self.error(node, &message, "remove the brackets", None)
    }

    /// Reports a type given the wrong number of arguments.
    fn arity(&mut self, node: NodeId, name: &str, expected: usize, found: usize) -> Type<'db> {
        let plural = if expected == 1 { "" } else { "s" };
        let message = format!("`{name}` takes {expected} type argument{plural}, found {found}");
        let example = GENERIC_BUILTINS
            .iter()
            .find(|(generic, _)| *generic == name)
            .map(|(_, example)| format!("for example `{example}`"));
        self.error(
            node,
            &message,
            "wrong number of arguments",
            example.as_deref(),
        )
    }

    /// Reports an expression that is not a type.
    fn not_a_type(&mut self, node: NodeId) -> Type<'db> {
        self.error(
            node,
            "this is not a type",
            "expected a type",
            Some("a type is a name such as `f64` or `Particle`, or one built from them, such as `Array[f64, 1]`"),
        )
    }

    /// Records an error pointing at `node`, with `label` under it and `hint` as a note, and
    /// returns the unknown type, which every later stage accepts without a further error.
    pub(crate) fn error(
        &mut self,
        node: NodeId,
        message: &str,
        label: &str,
        hint: Option<&str>,
    ) -> Type<'db> {
        let mut diagnostic = Diagnostic::error(message).with_primary(self.tree.span(node), label);
        if let Some(hint) = hint {
            diagnostic = diagnostic.with_note(hint);
        }
        self.diagnostics.push(diagnostic);
        self.unknown()
    }

    /// The unknown type.
    pub(crate) fn unknown(&self) -> Type<'db> {
        Type::new(self.db, TypeKind::Unknown)
    }
}
