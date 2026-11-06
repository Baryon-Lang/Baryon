//! The type of each expression: literals, names, fields, indexing, operators, comparisons and
//! collections.

use super::{Body, numbers, relations};
use crate::declare::fits;
use crate::outline::{Kind, outline};
use database::Definition;
use syntax::{Flags, NodeId, Operator, Role, SyntaxKind};
use types::{Complex, Declaration, Float, Integer, Item, Primitive, Type, TypeKind, Value};

impl<'db> Body<'db> {
    /// The type of `node`. `expected` is the type the context needs, which gives a number
    /// literal its type (decision 0014): in `x: f32 = 0.5`, the `0.5` is an `f32`.
    pub(super) fn expression(&mut self, node: NodeId, expected: Option<Type<'db>>) -> Type<'db> {
        let primitive = |kind| Type::new(self.db, TypeKind::Primitive(kind));
        match self.tree.kind(node) {
            _ if self.is_number_literal(node) => self.literal(node, expected),
            SyntaxKind::True | SyntaxKind::False => primitive(Primitive::Bool),
            SyntaxKind::None => Type::new(self.db, TypeKind::None),
            SyntaxKind::StringLiteral => Type::new(self.db, TypeKind::Str),
            SyntaxKind::FormattedString => {
                self.interpolations(node);
                Type::new(self.db, TypeKind::Str)
            }
            SyntaxKind::Name => self.name(node),
            SyntaxKind::Attribute => self.attribute(node),
            SyntaxKind::Subscript => self.subscript(node),
            SyntaxKind::Call => self.call(node, expected),
            SyntaxKind::BinaryOperation => self.binary(node),
            SyntaxKind::UnaryOperation => self.unary(node),
            SyntaxKind::BooleanOperation => self.boolean(node),
            SyntaxKind::Compare => self.compare(node),
            SyntaxKind::Conditional => self.conditional(node, expected),
            SyntaxKind::Tuple => {
                let expected_items = match expected.map(|ty| ty.kind(self.db)) {
                    Some(TypeKind::Tuple(items)) => items.clone(),
                    _ => Vec::new(),
                };
                let items: Vec<NodeId> = self.tree.children_with(node, Role::Item).collect();
                let items = items
                    .into_iter()
                    .enumerate()
                    .map(|(index, item)| self.expression(item, expected_items.get(index).copied()))
                    .collect();
                Type::new(self.db, TypeKind::Tuple(items))
            }
            SyntaxKind::List => self.list(node, expected),
            SyntaxKind::ListComprehension => {
                self.comprehensions(node);
                let item = self.child_type(node, Role::Value);
                Type::new(self.db, TypeKind::List(item))
            }
            _ => {
                // Constructs a later slice types, or the syntax check rejects: their parts are
                // still checked.
                let children: Vec<NodeId> = self.tree.children(node).collect();
                if matches!(
                    self.tree.kind(node),
                    SyntaxKind::Generator
                        | SyntaxKind::SetComprehension
                        | SyntaxKind::DictionaryComprehension
                ) {
                    self.comprehensions(node);
                }
                for child in children {
                    if self.tree.kind(child).is_expression()
                        && self.tree.kind(child) != SyntaxKind::Comprehension
                    {
                        self.expression(child, None);
                    }
                }
                self.lowerer.unknown()
            }
        }
    }

    /// The type of the child of `node` with `role`, or unknown if it is missing.
    pub(super) fn child_type(&mut self, node: NodeId, role: Role) -> Type<'db> {
        match self.tree.child(node, role) {
            Some(child) => self.expression(child, None),
            None => self.lowerer.unknown(),
        }
    }

    /// Whether `node` is a number literal, possibly signed: `3`, `-0.5`, `2j`.
    pub(super) fn is_number_literal(&self, node: NodeId) -> bool {
        match self.tree.kind(node) {
            SyntaxKind::Integer | SyntaxKind::Float | SyntaxKind::Imaginary => true,
            SyntaxKind::UnaryOperation => {
                matches!(
                    self.tree.node(node).operator(),
                    Operator::Negate | Operator::Plus
                ) && self
                    .tree
                    .child(node, Role::Value)
                    .is_some_and(|value| self.is_number_literal(value))
            }
            _ => false,
        }
    }

    /// The type of a number literal: the number type the context expects, if it can hold the
    /// literal, or the literal's default, `i64`, `f64` or `complex128`. Under a type parameter
    /// whose bound allows literals, the literal takes the type parameter.
    pub(super) fn literal(&mut self, node: NodeId, expected: Option<Type<'db>>) -> Type<'db> {
        if let Some(parameter) = expected.filter(|&ty| self.trait_of(ty).is_some()) {
            return self.generic_literal_type(node, parameter);
        }
        let wanted = expected.and_then(|ty| match ty.kind(self.db) {
            TypeKind::Primitive(primitive) if numbers::is_number(*primitive) => Some(*primitive),
            TypeKind::Optional(value) => match value.kind(self.db) {
                TypeKind::Primitive(primitive) if numbers::is_number(*primitive) => {
                    Some(*primitive)
                }
                _ => None,
            },
            _ => None,
        });
        let primitive = if self.is_imaginary(node) {
            match wanted {
                Some(Primitive::Complex(complex)) => Primitive::Complex(complex),
                _ => Primitive::Complex(Complex::C128),
            }
        } else {
            match (self.lowerer.evaluate(node), wanted) {
                (None, _) => return self.lowerer.unknown(),
                (Some(Value::Integer(number)), Some(Primitive::Integer(integer))) => {
                    if !fits(integer, number) {
                        let message = format!("`{number}` does not fit in `{}`", integer.name());
                        return self.lowerer.error(node, &message, "out of range", None);
                    }
                    Primitive::Integer(integer)
                }
                (Some(_), Some(wanted @ (Primitive::Float(_) | Primitive::Complex(_)))) => wanted,
                (Some(Value::Integer(_)), _) => Primitive::Integer(Integer::I64),
                (Some(Value::Float(_)), _) => Primitive::Float(Float::F64),
            }
        };
        Type::new(self.db, TypeKind::Primitive(primitive))
    }

    /// Whether a number literal is imaginary, `2j` or `-2j`.
    pub(super) fn is_imaginary(&self, node: NodeId) -> bool {
        match self.tree.kind(node) {
            SyntaxKind::Imaginary => true,
            SyntaxKind::UnaryOperation => self
                .tree
                .child(node, Role::Value)
                .is_some_and(|value| self.is_imaginary(value)),
            _ => false,
        }
    }

    /// Types the values interpolated into an f-string.
    fn interpolations(&mut self, node: NodeId) {
        let parts: Vec<NodeId> = self.tree.children(node).collect();
        for part in parts {
            match self.tree.kind(part) {
                SyntaxKind::Interpolation => {
                    if let Some(value) = self.tree.child(part, Role::Value) {
                        self.expression(value, None);
                    }
                    if let Some(format) = self.tree.child(part, Role::Format) {
                        self.interpolations(format);
                    }
                }
                SyntaxKind::FormatSpecification => self.interpolations(part),
                _ => {}
            }
        }
    }

    /// The type of a name read as a value.
    pub(super) fn name(&mut self, node: NodeId) -> Type<'db> {
        match self.lowerer.definition(node) {
            Some(Definition::Local(binding)) => self
                .locals
                .get(binding)
                .copied()
                .unwrap_or_else(|| self.lowerer.unknown()),
            Some(&Definition::Item(file, name)) => self.item_value(file, name),
            _ => self.lowerer.unknown(),
        }
    }

    /// The type of a module-level definition read as a value: a constant's type, or a
    /// function's type as a `Callable`. A class or alias read as a value, which only a later
    /// slice gives a type, is unknown.
    fn item_value(&mut self, file: database::File, name: NodeId) -> Type<'db> {
        let kind = outline(self.db, file)
            .get(name)
            .map(|entry| entry.kind.clone());
        let item = Item::new(self.db, self.lowerer.package, file, name);
        match (kind, &self.db.declaration(item).declaration) {
            (Some(Kind::Constant), Declaration::Constant { ty, .. }) => *ty,
            (Some(Kind::Function), Declaration::Function(signature))
                if signature.type_parameters.is_empty() =>
            {
                let parameters = signature.parameters.iter().map(|p| p.ty).collect();
                Type::new(
                    self.db,
                    TypeKind::Callable {
                        parameters,
                        returns: signature.returns,
                    },
                )
            }
            _ => self.lowerer.unknown(),
        }
    }

    /// `value[index]`: an element of a list, a string, a tuple, an `Array` or a `Fixed`.
    fn subscript(&mut self, node: NodeId) -> Type<'db> {
        let base = self.child_type(node, Role::Value);
        let Some(index) = self.tree.child(node, Role::Index) else {
            return self.lowerer.unknown();
        };
        let indices: Vec<NodeId> = if self.tree.kind(index) == SyntaxKind::Tuple
            && !self.tree.node(index).flags().contains(Flags::PARENTHESIZED)
        {
            self.tree.children_with(index, Role::Item).collect()
        } else {
            vec![index]
        };
        let sliced = indices
            .iter()
            .any(|&index| self.tree.kind(index) == SyntaxKind::Slice);
        for &index in &indices {
            self.index(index);
        }
        let unknown = self.lowerer.unknown();
        match (base.kind(self.db), indices.as_slice()) {
            (TypeKind::List(_), [_]) if sliced => base,
            (TypeKind::List(item), [_]) => *item,
            (TypeKind::Str, [_]) => base,
            (TypeKind::Tuple(items), [index]) => match self.lowerer.evaluate_quietly(*index) {
                Some(Value::Integer(position)) => usize::try_from(position)
                    .ok()
                    .and_then(|position| items.get(position).copied())
                    .unwrap_or(unknown),
                _ => unknown,
            },
            (TypeKind::Array { element, rank }, indices) => {
                self.array_index(node, base, *element, *rank as usize, indices)
            }
            (TypeKind::Fixed { element, shape }, indices) => {
                self.array_index(node, base, *element, shape.rank(), indices)
            }
            (TypeKind::Primitive(_) | TypeKind::None, _) => {
                let message = format!("a value of type `{}` cannot be indexed", self.show(base));
                self.lowerer.error(node, &message, "not a sequence", None)
            }
            _ => unknown,
        }
    }

    /// `a[i, j]` or `a[i, :]` on an array of `rank` dimensions: one index or slice per
    /// dimension (language reference, "Arrays", "Indexing"). All indices give the element; each
    /// slice keeps its dimension in the view the subscript gives.
    fn array_index(
        &mut self,
        node: NodeId,
        base: Type<'db>,
        element: Type<'db>,
        rank: usize,
        indices: &[NodeId],
    ) -> Type<'db> {
        if indices.len() != rank {
            let message = format!(
                "`{}` takes {rank} ind{}, found {}",
                self.show(base),
                if rank == 1 { "ex" } else { "ices" },
                indices.len()
            );
            return self.lowerer.error(
                node,
                &message,
                "one index per dimension",
                Some("take a row or column with a slice, such as `a[i, :]`"),
            );
        }
        let slices = indices
            .iter()
            .filter(|&&index| self.tree.kind(index) == SyntaxKind::Slice)
            .count();
        match u32::try_from(slices) {
            Ok(0) => element,
            Ok(rank) => Type::new(self.db, TypeKind::Array { element, rank }),
            Err(_) => self.lowerer.unknown(),
        }
    }

    /// Checks one index of a subscript: an integer, or a slice of integers.
    fn index(&mut self, index: NodeId) {
        let parts: Vec<NodeId> = if self.tree.kind(index) == SyntaxKind::Slice {
            self.tree.children(index).collect()
        } else {
            vec![index]
        };
        let int = Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::I64)),
        );
        for part in parts {
            let ty = self.expression(part, Some(int));
            let is_index = match ty.kind(self.db) {
                TypeKind::Primitive(Primitive::Integer(_)) => true,
                _ => relations::is_open(self.db, ty) || !self.is_scalar(ty),
            };
            if !is_index {
                let message = format!("an index must be an integer, found `{}`", self.show(ty));
                self.lowerer.error(part, &message, "not an integer", None);
            }
        }
    }

    /// Whether `ty` is a number, `bool`, `str` or `None`: a type whose operators this slice
    /// knows completely. Operators on other types, such as array arithmetic, are a later
    /// slice's.
    pub(super) fn is_scalar(&self, ty: Type<'db>) -> bool {
        matches!(
            ty.kind(self.db),
            TypeKind::Primitive(_) | TypeKind::Str | TypeKind::None
        )
    }

    /// `value if condition else other`: both branches have one type.
    fn conditional(&mut self, node: NodeId, expected: Option<Type<'db>>) -> Type<'db> {
        self.condition(self.tree.child(node, Role::Condition));
        let value = match self.tree.child(node, Role::Value) {
            Some(value) => self.expression(value, expected),
            None => return self.lowerer.unknown(),
        };
        let Some(other_node) = self.tree.child(node, Role::Else) else {
            return value;
        };
        let other = self.expression(other_node, Some(value));
        if relations::compatible(self.db, value, other) {
            return value;
        }
        let message = format!(
            "both branches must have one type, found `{}` and `{}`",
            self.show(value),
            self.show(other)
        );
        self.lowerer.error(node, &message, "branches differ", None)
    }

    /// `[a, b, c]`: a list whose items share one type. An empty list takes its type from the
    /// context, and is an error without one.
    fn list(&mut self, node: NodeId, expected: Option<Type<'db>>) -> Type<'db> {
        let mut item_type = match expected.map(|ty| ty.kind(self.db)) {
            Some(TypeKind::List(item)) => Some(*item),
            _ => None,
        };
        let items: Vec<NodeId> = self.tree.children_with(node, Role::Item).collect();
        if items.is_empty() {
            return match item_type {
                Some(item) => Type::new(self.db, TypeKind::List(item)),
                None => self.lowerer.error(
                    node,
                    "an empty list needs its type written",
                    "the item type is unknown",
                    Some("for example `values: list[f64] = []`"),
                ),
            };
        }
        for item in items {
            let found = self.expression(item, item_type);
            match item_type {
                None => item_type = Some(found),
                Some(first) if !relations::compatible(self.db, first, found) => {
                    let message = format!(
                        "list items must share one type, found `{}` and `{}`",
                        self.show(first),
                        self.show(found)
                    );
                    self.lowerer.error(item, &message, "a different type", None);
                }
                Some(_) => {}
            }
        }
        let item = item_type.unwrap_or_else(|| self.lowerer.unknown());
        Type::new(self.db, TypeKind::List(item))
    }

    /// Binds the targets of a comprehension's `for` clauses and checks its conditions.
    fn comprehensions(&mut self, node: NodeId) {
        let clauses: Vec<NodeId> = self.tree.children_with(node, Role::Item).collect();
        for clause in clauses {
            let element = match self.tree.child(clause, Role::Iterator) {
                Some(iterator) => self.iteration(iterator),
                None => self.lowerer.unknown(),
            };
            if let Some(target) = self.tree.child(clause, Role::Target) {
                self.bind(target, element, None);
            }
            let conditions: Vec<NodeId> =
                self.tree.children_with(clause, Role::Condition).collect();
            for condition in conditions {
                self.condition(Some(condition));
            }
        }
    }

    /// The type of each element a `for` loop over `iterator` sees.
    pub(super) fn iteration(&mut self, iterator: NodeId) -> Type<'db> {
        if let Some(element) = self.builtin_iteration(iterator) {
            return element;
        }
        let ty = self.expression(iterator, None);
        let unknown = self.lowerer.unknown();
        match ty.kind(self.db) {
            TypeKind::List(item) => *item,
            TypeKind::Str => ty,
            TypeKind::Array { element, rank: 1 } => *element,
            TypeKind::Fixed { element, shape } if shape.rank() == 1 => *element,
            TypeKind::Tuple(items) if items.windows(2).all(|pair| pair[0] == pair[1]) => {
                items.first().copied().unwrap_or(unknown)
            }
            TypeKind::Primitive(_) | TypeKind::None => {
                let message = format!("a value of type `{}` cannot be looped over", self.show(ty));
                self.lowerer
                    .error(iterator, &message, "not a sequence", None)
            }
            _ => unknown,
        }
    }
}
