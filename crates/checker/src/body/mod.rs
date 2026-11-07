//! Function bodies: the type of every expression, and the errors in how values are used.
//!
//! Inference is local to one function (language reference, "Types", "Type inference"): a
//! function's signature is all its callers see, and inside the body each variable takes the
//! type of the first value assigned to it, or its annotation, and keeps that type for its whole
//! scope. One walk over the body in source order types every statement.
//!
//! What it checks: number literals, number mixing, operators and comparisons, `bool`
//! conditions, calls to functions, constructors and methods against their signatures, field
//! access, assignments and returns (roadmap 1.7.3); arrays: creating them, indexing,
//! slicing, methods, whole-array arithmetic and `@` (1.7.4, the `arrays` module); and
//! generics: type arguments inferred at calls, bounds checked, and inside a generic body only
//! the operations a bound allows (1.7.5, the `generics` module); and `match`: patterns
//! against the subject, and cases that cover every value (1.7.6, the `patterns` and
//! `exhaustive` modules); and errors: each error a call or `raise` can fail with is caught by a
//! `try` or declared in the function's `Raises` (1.7.7, the `errors` module). What a later
//! slice checks, and this one lets through without an error: the `None` check that narrows
//! `T | None` to `T` (1.7.8).

mod arrays;
mod builtins;
mod calls;
mod errors;
mod exhaustive;
mod expressions;
mod generics;
mod members;
mod numbers;
mod operators;
mod patterns;
mod relations;

use crate::lower::Lowerer;
use crate::outline::Entry;
use database::{Definition, File, Package};
use diagnostics::Diagnostic;
use std::collections::BTreeMap;
use syntax::{NodeId, Role, SyntaxKind, SyntaxTree};
use types::{Declaration, Item, Primitive, Signature, Type, TypeDb, TypeKind};

/// Checks the body of the function or method `entry` in `file`, returning the errors found.
pub(crate) fn check(
    db: &dyn TypeDb,
    package: Package,
    file: File,
    entry: &Entry,
) -> Vec<Diagnostic> {
    let item = Item::new(db, package, file, entry.name);
    let Declaration::Function(signature) = &db.declaration(item).declaration else {
        return Vec::new();
    };
    let lowerer = Lowerer::new(db, package, file, entry.class, entry.statement);
    let mut body = Body {
        db,
        tree: lowerer.tree,
        text: lowerer.text,
        lowerer,
        signature: signature.clone(),
        function: entry.statement,
        locals: BTreeMap::new(),
        handled: Vec::new(),
        caught: Vec::new(),
    };
    body.parameters(entry.statement);
    body.block(entry.statement, Role::Body);
    body.lowerer.diagnostics
}

/// The state of checking one function body.
struct Body<'db> {
    /// The database.
    db: &'db dyn TypeDb,
    /// The file's tree.
    tree: &'db SyntaxTree,
    /// The file's text.
    text: &'db str,
    /// Lowers local annotations and computes literals; holds the errors found.
    lowerer: Lowerer<'db>,
    /// The function's signature.
    signature: Signature<'db>,
    /// The function's definition statement.
    function: NodeId,
    /// The type of each local variable, by the `Name` node of its first binding, which the
    /// resolver gives every use of the variable.
    locals: BTreeMap<NodeId, Type<'db>>,
    /// The error types each enclosing `try` handles, innermost last.
    handled: Vec<Vec<Type<'db>>>,
    /// The error types each enclosing `except` block caught, innermost last, which a bare
    /// `raise` raises again.
    caught: Vec<Vec<Type<'db>>>,
}

impl<'db> Body<'db> {
    /// Binds each parameter to its declared type, and checks each default value against it.
    fn parameters(&mut self, function: NodeId) {
        let nodes: Vec<NodeId> = self
            .tree
            .child(function, Role::Parameters)
            .into_iter()
            .flat_map(|parameters| self.tree.children(parameters))
            .filter(|&parameter| self.tree.kind(parameter) == SyntaxKind::Parameter)
            .filter(|&parameter| self.tree.child(parameter, Role::Name).is_some())
            .collect();
        let types: Vec<Type<'db>> = self.signature.parameters.iter().map(|p| p.ty).collect();
        for (node, ty) in nodes.into_iter().zip(types) {
            if let Some(name) = self.tree.child(node, Role::Name) {
                self.locals.insert(name, ty);
            }
            if let Some(default) = self.tree.child(node, Role::Default) {
                let found = self.expression(default, Some(ty));
                self.expect(default, ty, found, "the parameter's type is");
            }
        }
    }

    /// Checks every statement of `parent` with the role `role`, in order.
    fn block(&mut self, parent: NodeId, role: Role) {
        let statements: Vec<NodeId> = self.tree.children_with(parent, role).collect();
        for statement in statements {
            self.statement(statement);
        }
    }

    /// Checks one statement.
    fn statement(&mut self, statement: NodeId) {
        let child = |role| self.tree.child(statement, role);
        match self.tree.kind(statement) {
            SyntaxKind::Raise => self.raise(statement),
            SyntaxKind::ExpressionStatement => {
                if let Some(value) = child(Role::Value) {
                    self.expression(value, None);
                }
            }
            SyntaxKind::Assign => self.assignment(statement),
            SyntaxKind::AnnotatedAssign => self.annotated(statement),
            SyntaxKind::AugmentedAssign => self.augmented(statement),
            SyntaxKind::Return => self.return_statement(statement),
            SyntaxKind::If => {
                self.condition(child(Role::Condition));
                self.block(statement, Role::Body);
                let clauses: Vec<NodeId> = self.tree.children_with(statement, Role::Else).collect();
                for clause in clauses {
                    self.condition(self.tree.child(clause, Role::Condition));
                    self.block(clause, Role::Body);
                }
            }
            SyntaxKind::While => {
                self.condition(child(Role::Condition));
                self.block(statement, Role::Body);
                self.block(statement, Role::Else);
            }
            SyntaxKind::For => {
                let element = match child(Role::Iterator) {
                    Some(iterator) => self.iteration(iterator),
                    None => self.lowerer.unknown(),
                };
                if let Some(target) = child(Role::Target) {
                    self.bind(target, element, None);
                }
                self.block(statement, Role::Body);
                self.block(statement, Role::Else);
            }
            SyntaxKind::With => {
                let items: Vec<NodeId> = self.tree.children_with(statement, Role::Item).collect();
                for item in items {
                    if let Some(value) = self.tree.child(item, Role::Value) {
                        self.expression(value, None);
                    }
                }
                self.block(statement, Role::Body);
            }
            SyntaxKind::Match => self.match_statement(statement),
            SyntaxKind::Try => self.try_statement(statement),
            SyntaxKind::Assert => {
                self.condition(child(Role::Condition));
                if let Some(message) = child(Role::Message) {
                    self.expression(message, None);
                }
            }
            _ => {}
        }
    }

    /// `a = b = value`.
    fn assignment(&mut self, statement: NodeId) {
        let targets: Vec<NodeId> = self.tree.children_with(statement, Role::Target).collect();
        let Some(value) = self.tree.child(statement, Role::Value) else {
            return;
        };
        let expected = match targets.as_slice() {
            [target] => self.declared(*target),
            _ => None,
        };
        let ty = self.expression(value, expected);
        for target in targets {
            self.bind(target, ty, Some(value));
        }
    }

    /// `name: annotation = value`: the annotation fixes the variable's type.
    fn annotated(&mut self, statement: NodeId) {
        let ty = match self.tree.child(statement, Role::Annotation) {
            Some(annotation) => self.lowerer.lower(annotation),
            None => self.lowerer.unknown(),
        };
        if let Some(target) = self.tree.child(statement, Role::Target) {
            self.bind(target, ty, None);
        }
        if let Some(value) = self.tree.child(statement, Role::Value) {
            let found = self.expression(value, Some(ty));
            self.expect(value, ty, found, "the annotation says");
        }
    }

    /// `target += value` and the other in-place operators: the result must keep the target's
    /// type.
    fn augmented(&mut self, statement: NodeId) {
        let (Some(target), Some(value)) = (
            self.tree.child(statement, Role::Target),
            self.tree.child(statement, Role::Value),
        ) else {
            return;
        };
        let operator = self.tree.node(statement).operator();
        let current = self.expression(target, None);
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

    /// `return value`, checked against the function's return type. In a function that returns
    /// `Raises[T, E]`, a `return` gives the `T`.
    fn return_statement(&mut self, statement: NodeId) {
        let expected = match self.signature.returns.kind(self.db) {
            TypeKind::Fallible { value, .. } => *value,
            _ => self.signature.returns,
        };
        if let Some(value) = self.tree.child(statement, Role::Value) {
            let found = self.expression(value, Some(expected));
            if self.tree.child(self.function, Role::Returns).is_none()
                && !relations::compatible(self.db, expected, found)
            {
                let message = format!(
                    "this function has no return type, so it cannot return `{}`",
                    self.show(found)
                );
                let note = format!("declare what it returns: `-> {}`", self.show(found));
                self.lowerer
                    .error(value, &message, "returns a value", Some(&note));
            } else {
                self.expect(value, expected, found, "the function returns");
            }
        } else {
            let nothing = Type::new(self.db, TypeKind::None);
            if !relations::compatible(self.db, expected, nothing) {
                let message = format!(
                    "this function returns `{}`, so `return` needs a value",
                    self.show(expected)
                );
                self.lowerer.error(statement, &message, "no value", None);
            }
        }
    }

    /// Checks that a condition is a `bool`. A missing condition, after a syntax error, is
    /// skipped.
    fn condition(&mut self, condition: Option<NodeId>) {
        let Some(condition) = condition else {
            return;
        };
        let boolean = Type::new(self.db, TypeKind::Primitive(Primitive::Bool));
        let found = self.expression(condition, Some(boolean));
        if !relations::compatible(self.db, boolean, found) {
            let message = format!("a condition must be a `bool`, found `{}`", self.show(found));
            self.lowerer.error(
                condition,
                &message,
                "not a `bool`",
                Some("compare explicitly, such as `n != 0` or `x is not None`"),
            );
        }
    }

    /// The type a target already holds, which gives a literal assigned to it its type: in
    /// `x: f32 = 0` then `x = 1`, the `1` is an `f32`.
    fn declared(&self, target: NodeId) -> Option<Type<'db>> {
        match self.lowerer.definition(target) {
            Some(Definition::Local(binding)) if self.tree.kind(target) == SyntaxKind::Name => {
                self.locals.get(binding).copied()
            }
            _ => None,
        }
    }

    /// Binds `target` to a value of type `ty`. A new variable takes the type; a variable that
    /// has one keeps it, and a value of another type is an error at `value`, or at the target
    /// when there is no value node, as in a `for` loop.
    fn bind(&mut self, target: NodeId, ty: Type<'db>, value: Option<NodeId>) {
        match self.tree.kind(target) {
            SyntaxKind::Name => {
                let Some(&Definition::Local(binding)) = self.lowerer.definition(target) else {
                    return;
                };
                match self.locals.get(&binding) {
                    Some(&held) => {
                        if !relations::compatible(self.db, held, ty) {
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
                let held = self.expression(target, None);
                self.expect(value.unwrap_or(target), held, ty, "the target holds");
            }
            _ => {}
        }
    }

    /// Reports `found` where `expected` is needed, at `node`. `context` says where the expected
    /// type comes from, such as "the function returns".
    fn expect(&mut self, node: NodeId, expected: Type<'db>, found: Type<'db>, context: &str) {
        if relations::compatible(self.db, expected, found) {
            return;
        }
        let message = format!(
            "expected `{}`, found `{}`",
            self.show(expected),
            self.show(found)
        );
        let label = format!("{context} `{}`", self.show(expected));
        self.lowerer.error(node, &message, &label, None);
    }

    /// The type a number literal on the other side of an operator should take: the other
    /// side's type when it is a number, its element type when it is an array, so `2.0 * a`
    /// with `a: Array[f32, 1]` multiplies by an `f32`, and what a type parameter's bound
    /// gives a literal (the `generics` module).
    fn number_context(&self, ty: Type<'db>) -> Option<Type<'db>> {
        match ty.kind(self.db) {
            TypeKind::Primitive(primitive) if numbers::is_number(*primitive) => Some(ty),
            TypeKind::Array { element, .. } | TypeKind::Fixed { element, .. } => {
                self.number_context(*element)
            }
            _ => self.generic_literal(ty),
        }
    }

    /// A type as a program writes it.
    fn show(&self, ty: Type<'db>) -> String {
        ty.display(self.db).to_string()
    }
}
