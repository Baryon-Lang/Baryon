//! Calls to the language's built-in names: number conversions such as `f64(n)`, the built-in
//! functions, and the iterators `range`, `prange`, `enumerate` and `zip` in a `for` loop.

use super::calls::Arguments;
use super::{Body, numbers, relations};
use database::Definition;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Integer, Primitive, Type, TypeKind};

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
