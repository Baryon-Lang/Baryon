//! Calls to the language's built-in names: number conversions such as `f64(n)`, the built-in
//! functions, and the iterators `range`, `prange`, `enumerate` and `zip` in a `for` loop.

use super::calls::Arguments;
use super::{Body, numbers, relations};
use database::Definition;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Call, Callee, Integer, Key, Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// A call to one of the language's names: a number conversion such as `f64(n)`, or a
    /// built-in function.
    pub(super) fn builtin(&mut self, node: NodeId, name: &str, arguments: &Arguments) -> Type<'db> {
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
            ("print", _) => {
                self.print_keywords(arguments, &types[arguments.positional.len()..]);
                Type::new(self.db, TypeKind::None)
            }
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

    /// Checks `print`'s keyword arguments, whose types are `types`: only `sep=` and `end=`, each
    /// a `str` or `None`, as Python allows. Python's `file=` and `flush=` are not part of the
    /// language yet: `print` always writes to standard output.
    fn print_keywords(&mut self, arguments: &Arguments, types: &[Type<'db>]) {
        for (&(name, value), &ty) in arguments.keywords.iter().zip(types) {
            let written = self.tree.text(name, self.text);
            if written != "sep" && written != "end" {
                let message = format!("`print` has no parameter `{written}`");
                self.lowerer.error(
                    name,
                    &message,
                    "unknown parameter",
                    Some("`print` takes the values to write, then `sep=` and `end=`"),
                );
                continue;
            }
            let text = matches!(ty.kind(self.db), TypeKind::Str | TypeKind::None);
            if !text && !relations::is_open(self.db, ty) {
                let message = format!(
                    "`{written}=` takes a `str` or `None`, found `{}`",
                    self.show(ty)
                );
                self.lowerer.error(value, &message, "not a `str`", None);
            }
        }
    }

    /// Whether `ty` is a number type.
    fn is_number(&self, ty: Type<'db>) -> bool {
        matches!(ty.kind(self.db), TypeKind::Primitive(primitive) if numbers::is_number(*primitive))
    }

    /// The element type of a `for` loop over `range`, `prange`, `simd(range(...))`, `enumerate` or
    /// `zip`, or
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
                if *name == "prange" {
                    self.prange_keywords(&arguments);
                } else {
                    for &(_, value) in &arguments.keywords {
                        self.expression(value, None);
                    }
                }
                Some(element.unwrap_or(int))
            }
            "simd" => {
                let range = match arguments.positional.as_slice() {
                    [range]
                        if arguments.keywords.is_empty() && self.calls_builtin(*range, "range") =>
                    {
                        Some(*range)
                    }
                    _ => None,
                };
                if let Some(range) = range {
                    // The inner `range` call is recorded too, for lowering to read its bounds.
                    let element = self.builtin_iteration(range)?;
                    if let Some(function) = self.tree.child(range, Role::Function) {
                        let inner = self.arguments(range);
                        self.record_builtin(range, function, &inner, element);
                    }
                    return Some(element);
                }
                for &argument in &arguments.positional {
                    self.expression(argument, None);
                }
                for &(_, value) in &arguments.keywords {
                    self.expression(value, None);
                }
                self.lowerer.error(
                    iterator,
                    "`simd` takes one `range(...)`",
                    "not `simd(range(...))`",
                    Some("write the loop as `for i in simd(range(n)):`"),
                );
                Some(int)
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

    /// Records the call `node` of `function` for the stages after the checker, when it calls
    /// something the language provides rather than a declared function: a conversion such as
    /// `f64(x)` or `Vec3(1.0, 0.0, 0.0)`, which builds `ty`, a built-in function, a `math`
    /// function such as `math.sqrt`, an `Array` or `Fixed` function, or a method of a list or
    /// array, whose receiver comes first. A call the checker could not resolve, after an error,
    /// is not recorded.
    pub(super) fn record_builtin(
        &mut self,
        node: NodeId,
        function: NodeId,
        arguments: &Arguments,
        ty: Type<'db>,
    ) {
        let mut receiver = None;
        let callee = match (self.tree.kind(function), self.lowerer.definition(function)) {
            (_, Some(Definition::Builtin(name))) if Primitive::from_name(name).is_some() => {
                Callee::Conversion(ty)
            }
            (_, Some(Definition::Builtin(name))) => Callee::Builtin((*name).to_owned()),
            (
                _,
                Some(Definition::Standard {
                    module: "math",
                    member: Some(member),
                }),
            ) if !matches!(ty.kind(self.db), TypeKind::Unknown) => {
                Callee::Builtin(format!("math.{member}"))
            }
            (
                _,
                Some(Definition::Standard {
                    module: "random",
                    member: Some(member),
                }),
            ) if !matches!(ty.kind(self.db), TypeKind::Unknown) => {
                Callee::Builtin(format!("random.{member}"))
            }
            (_, Some(Definition::Item(..)))
                if matches!(ty.kind(self.db), TypeKind::Fixed { .. }) =>
            {
                Callee::Conversion(ty)
            }
            (SyntaxKind::Attribute, None) => {
                let (Some(value), Some(attribute)) = (
                    self.tree.child(function, Role::Value),
                    self.tree.child(function, Role::Attribute),
                ) else {
                    return;
                };
                let method = self.tree.text(attribute, self.text);
                let owner = match self.lowerer.definition(value) {
                    Some(Definition::Builtin("Array")) => "Array",
                    Some(Definition::Item(..)) => "Fixed",
                    _ => {
                        receiver = Some(value);
                        match self.typed.expression(value).map(|ty| ty.kind(self.db)) {
                            Some(TypeKind::List(_)) => "list",
                            Some(TypeKind::Array { .. }) => "Array",
                            Some(TypeKind::Fixed { .. }) => "Fixed",
                            Some(TypeKind::Standard(standard)) => standard.name(),
                            _ => return,
                        }
                    }
                };
                Callee::Builtin(format!("{owner}.{method}"))
            }
            _ => return,
        };
        let written = receiver
            .into_iter()
            .chain(arguments.positional.iter().copied())
            .chain(arguments.keywords.iter().map(|&(_, value)| value))
            .map(|argument| Some(Key(argument)))
            .collect();
        let call = Call {
            callee,
            type_arguments: Vec::new(),
            arguments: written,
        };
        self.typed.calls.insert(Key(node), call);
    }
}
