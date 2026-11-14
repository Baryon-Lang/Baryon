//! The type of each collection a program writes and loops over: a list literal, whose
//! items share one type or are that type and `None`, the clauses of a comprehension, and
//! what a `for` loop's iterator gives.

use super::{Body, relations};
use syntax::{NodeId, Role};
use types::{Type, TypeKind};

impl<'db> Body<'db> {
    /// `[a, b, c]`: a list whose items share one type. An empty list takes its type from the
    /// context, and is an error without one.
    ///
    /// With no type from the context, items of one type `T` and `None` make a `list[T | None]`
    /// (decision 0045, part 3), so `[1.5, None, 2.5]` needs no annotation. Lowering wraps each
    /// item into the element type. Two different types other than `None` stay an error.
    pub(super) fn list(&mut self, node: NodeId, expected: Option<Type<'db>>) -> Type<'db> {
        let written = match expected.map(|ty| ty.kind(self.db)) {
            Some(TypeKind::List(item)) => Some(*item),
            _ => None,
        };
        let items: Vec<NodeId> = self.tree.children_with(node, Role::Item).collect();
        if items.is_empty() {
            return match written {
                Some(item) => Type::new(self.db, TypeKind::List(item)),
                None => self.lowerer.error(
                    node,
                    "an empty list needs its type written",
                    "the item type is unknown",
                    Some("for example `values: list[f64] = []`"),
                ),
            };
        }
        let mut item_type = written;
        let mut gaps = false;
        for item in items {
            let found = self.expression(item, item_type);
            self.consume(item, found);
            if written.is_none() && matches!(found.kind(self.db), TypeKind::None) {
                gaps = true;
                continue;
            }
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
        let nothing = Type::new(self.db, TypeKind::None);
        let item = match item_type {
            Some(item) if gaps => types::union(self.db, [item, nothing]),
            Some(item) => item,
            None => nothing,
        };
        Type::new(self.db, TypeKind::List(item))
    }

    /// Binds the targets of a comprehension's `for` clauses and checks its conditions.
    pub(super) fn comprehensions(&mut self, node: NodeId) {
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
        let view = self.dict_iteration(iterator);
        if let Some(element) = view.or_else(|| self.rows_iteration(iterator)) {
            return element;
        }
        if let Some(element) = self.builtin_iteration(iterator) {
            if let Some(function) = self.tree.child(iterator, Role::Function) {
                let arguments = self.arguments(iterator);
                self.record_builtin(iterator, function, &arguments, element);
            }
            return element;
        }
        let ty = self.expression(iterator, None);
        let statement = self.iterating.take();
        if let Some(element) = self.user_iteration(iterator, ty, statement) {
            return element;
        }
        let unknown = self.lowerer.unknown();
        match ty.kind(self.db) {
            TypeKind::List(item) | TypeKind::Dict { key: item, .. } => *item,
            TypeKind::Str => ty,
            TypeKind::Array { element, rank: 1 } => *element,
            TypeKind::Fixed { element, shape } if shape.rank() == 1 => *element,
            TypeKind::Tuple(items) if items.windows(2).all(|pair| pair[0] == pair[1]) => {
                items.first().copied().unwrap_or(unknown)
            }
            // Each pass reads the next line of a file (the `files` module), or borrows the next
            // element of a text array.
            TypeKind::Standard(types::StandardType::File) | TypeKind::TextArray => {
                Type::new(self.db, TypeKind::Str)
            }
            TypeKind::OptionalArray(_) => self.optional_array_unbuilt(iterator, "a `for` loop"),
            TypeKind::Primitive(_) | TypeKind::None => {
                let message = format!("a value of type `{}` cannot be looped over", self.show(ty));
                self.lowerer
                    .error(iterator, &message, "not a sequence", None)
            }
            _ => unknown,
        }
    }
}
