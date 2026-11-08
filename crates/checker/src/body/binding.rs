//! Assignment statements, `a = b = value`, `x: T = value` and `x += value`, and binding a
//! target to a value's type, which loops, comprehensions, `except ... as` and `case` share.

use super::{Body, relations};
use database::Definition;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Type, TypeKind};

impl<'db> Body<'db> {
    /// `a = b = value`.
    pub(super) fn assignment(&mut self, statement: NodeId) {
        let targets: Vec<NodeId> = self.tree.children_with(statement, Role::Target).collect();
        let Some(value) = self.tree.child(statement, Role::Value) else {
            return;
        };
        if let [target] = targets[..]
            && matches!(
                self.tree.kind(target),
                SyntaxKind::Attribute | SyntaxKind::Subscript
            )
        {
            // The place's type is the value's context, so `p.mass = 2` on an `f32` field
            // makes `2` an `f32`, as `x: f32 = 2` does.
            self.change(target);
            let held = self.expression(target, None);
            let ty = self.expression(value, Some(held));
            self.consume(value, ty);
            self.expect(value, held, ty, "the target holds");
            return;
        }
        let expected = match targets.as_slice() {
            [target] => self.declared(*target),
            _ => None,
        };
        let ty = self.expression(value, expected);
        self.consume(value, ty);
        for target in targets {
            self.bind(target, ty, Some(value));
        }
    }

    /// `name: annotation = value`: the annotation fixes the variable's type.
    pub(super) fn annotated(&mut self, statement: NodeId) {
        let ty = match self.tree.child(statement, Role::Annotation) {
            Some(annotation) => self.lowerer.lower(annotation),
            None => self.lowerer.unknown(),
        };
        let value = self.tree.child(statement, Role::Value);
        if let Some(target) = self.tree.child(statement, Role::Target) {
            self.bind(target, ty, None);
            if value.is_none()
                && let Some(&Definition::Local(binding)) = self.lowerer.definition(target)
            {
                // `x: f64` declares the type, and assigns nothing.
                self.state.assigned.remove(&binding);
            }
        }
        if let Some(value) = value {
            let found = self.expression(value, Some(ty));
            self.consume(value, found);
            self.expect(value, ty, found, "the annotation says");
        }
    }

    /// `target += value` and the other in-place operators: the result must keep the target's
    /// type.
    pub(super) fn augmented(&mut self, statement: NodeId) {
        let (Some(target), Some(value)) = (
            self.tree.child(statement, Role::Target),
            self.tree.child(statement, Role::Value),
        ) else {
            return;
        };
        let operator = self.tree.node(statement).operator();
        self.change(target);
        let current = self.expression(target, None);
        if self.maybe_none(target, current) {
            self.expression(value, None);
            return;
        }
        let operand = self.expression(value, self.number_context(current));
        let result = self.combine(statement, operator, current, operand);
        if !relations::compatible(self.db, current, result) {
            let message = format!(
                "`{}=` gives `{}`, but the target has type `{}`",
                operator.symbol(),
                self.show(result),
                self.show(current)
            );
            self.lowerer.error(
                statement,
                &message,
                "would change the target's type",
                Some("a variable keeps one type for its whole scope; convert the value, or use a new name"),
            );
        }
    }

    /// Binds `target` to a value of type `ty`. A new variable takes the type; a variable that
    /// has one keeps it, and a value of another type is an error at `value`, or at the target
    /// when there is no value node, as in a `for` loop.
    pub(super) fn bind(&mut self, target: NodeId, ty: Type<'db>, value: Option<NodeId>) {
        match self.tree.kind(target) {
            SyntaxKind::Name => {
                let Some(&Definition::Local(binding)) = self.lowerer.definition(target) else {
                    return;
                };
                self.change(target);
                self.state.assigned.insert(binding);
                self.state.moved.remove(&binding);
                match self.locals.get(&binding) {
                    Some(&held) => {
                        if relations::compatible(self.db, held, ty) {
                            self.assigned(binding, held, ty);
                        } else {
                            let name = self.tree.text(target, self.text);
                            let message = format!(
                                "`{name}` has type `{}`, so it cannot hold `{}`",
                                self.show(held),
                                self.show(ty)
                            );
                            let label = format!("type `{}`", self.show(ty));
                            self.lowerer.error(
                                value.unwrap_or(target),
                                &message,
                                &label,
                                Some("a variable keeps one type for its whole scope; give this value a new name"),
                            );
                        }
                    }
                    None => {
                        self.locals.insert(binding, ty);
                    }
                }
            }
            SyntaxKind::Tuple | SyntaxKind::List => {
                let targets: Vec<NodeId> = self.tree.children_with(target, Role::Item).collect();
                let items = match ty.kind(self.db) {
                    TypeKind::Tuple(items) if items.len() == targets.len() => items.clone(),
                    TypeKind::Fixed { element, shape }
                        if shape.rank() == 1
                            && usize::try_from(shape.elements()) == Ok(targets.len()) =>
                    {
                        vec![*element; targets.len()]
                    }
                    TypeKind::List(item) => vec![*item; targets.len()],
                    TypeKind::Unknown => vec![ty; targets.len()],
                    _ if relations::is_open(self.db, ty) => {
                        vec![self.lowerer.unknown(); targets.len()]
                    }
                    _ => {
                        let message = format!(
                            "a value of type `{}` cannot be unpacked into {} names",
                            self.show(ty),
                            targets.len()
                        );
                        self.lowerer
                            .error(target, &message, "does not match the value", None);
                        vec![self.lowerer.unknown(); targets.len()]
                    }
                };
                for (target, item) in targets.into_iter().zip(items) {
                    self.bind(target, item, None);
                }
            }
            SyntaxKind::Attribute | SyntaxKind::Subscript => {
                self.change(target);
                let held = self.expression(target, None);
                self.expect(value.unwrap_or(target), held, ty, "the target holds");
            }
            _ => {}
        }
    }
}
