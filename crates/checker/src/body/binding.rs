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
            // `path.name = "x"` is reported before `change`, which would add that a borrowed
            // `path` cannot change; a part reached through a field is reported once typed.
            if self.path_part_changed(target) {
                self.expression(value, None);
                return;
            }
            if self.header_store(target, value) {
                return;
            }
            if self.netcdf_store(target, value) {
                return;
            }
            self.change(target);
            let held = self.expression(target, None);
            if self.frame_attribute_assigned(target) {
                self.expression(value, None);
                return;
            }
            if self.path_part_changed(target) {
                self.expression(value, None);
                return;
            }
            self.assigned_complex_part(target);
            self.assigned_module_member(target);
            // A slice takes a number for every element, `a[1:n] = 0.0` (reference, "Slices").
            // So do the elements an array chooses, `a[a < 0.0] = 0.0` (roadmap 3.13.1).
            let chosen = self.is_selection(target);
            let filled = match held.kind(self.db) {
                TypeKind::Array { element, .. } if self.is_view(target) || chosen => Some(*element),
                _ => None,
            };
            let ty = self.expression(value, Some(filled.unwrap_or(held)));
            if let Some(element) = filled
                && chosen
            {
                if self.is_scalar(ty) || ty == element {
                    self.expect(value, element, ty, "each chosen element holds");
                } else if !relations::is_open(self.db, ty) {
                    let message = format!(
                        "the chosen elements take one `{}`, found `{}`",
                        self.show(element),
                        self.show(ty)
                    );
                    self.lowerer.error(
                        value,
                        &message,
                        "not a single value",
                        Some("every chosen element is set to the same value, as in `a[a < 0.0] = 0.0`"),
                    );
                }
                return;
            }
            // An array is read element by element into an array slice, as `NumPy` copies it,
            // and a list slice takes copies of the elements of its list, `xs[i:j] = ys`, as
            // Python's does: nothing moves.
            let list_slice =
                self.is_view(target) && matches!(held.kind(self.db), TypeKind::List(_));
            if !list_slice && filled.is_none() {
                self.consume(value, ty);
            }
            match filled {
                Some(element) if self.is_scalar(ty) => {
                    self.expect(value, element, ty, "each element of the slice holds");
                }
                // An array of lower rank broadcasts over the slice's leading dimensions, as
                // `NumPy`'s does: `grid[:, :] = row` writes `row` into every row.
                Some(_) => {
                    let ty = match (ty.kind(self.db), held.kind(self.db)) {
                        (
                            TypeKind::Array { element, rank },
                            TypeKind::Array {
                                rank: target_rank, ..
                            },
                        ) if rank < target_rank => Type::new(
                            self.db,
                            TypeKind::Array {
                                element: *element,
                                rank: *target_rank,
                            },
                        ),
                        _ => ty,
                    };
                    self.expect(value, held, ty, "the target holds");
                }
                None => self.expect(value, held, ty, "the target holds"),
            }
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
        if self.path_part_changed(target) {
            self.expression(value, None);
            return;
        }
        self.change(target);
        let current = self.expression(target, None);
        if self.path_part_changed(target) {
            self.expression(value, None);
            return;
        }
        self.assigned_complex_part(target);
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
                            // `y = None` then `y = 3`: the Python habit of a value that comes
                            // later, which an optional type writes.
                            let advice = if matches!(held.kind(self.db), TypeKind::None) {
                                format!(
                                    "a variable keeps one type for its whole scope; to fill it in later, declare it optional: `{name}: {} | None = None`",
                                    self.show(ty)
                                )
                            } else {
                                "a variable keeps one type for its whole scope; give this value a new name".to_owned()
                            };
                            self.lowerer.error(
                                value.unwrap_or(target),
                                &message,
                                &label,
                                Some(&advice),
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
                self.assigned_complex_part(target);
                self.expect(value.unwrap_or(target), held, ty, "the target holds");
            }
            _ => {}
        }
    }
}
