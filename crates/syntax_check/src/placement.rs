//! Checks on where a statement may appear: only definitions at the top level of a module, only
//! fields, methods and documentation in a class, and only calls and documentation strings as
//! expression statements in a function. [`statements`](crate::statements) calls these for each
//! statement before its own checks.

use crate::Checker;
use syntax::{NodeId, Operator, Role, SyntaxKind};

impl Checker<'_> {
    /// Checks a statement at the top level of a module, which may only define things.
    pub(crate) fn top_level(&mut self, id: NodeId, previous: Option<NodeId>) {
        let span = self.tree.span(id);
        let kind = self.tree.kind(id);
        match kind {
            SyntaxKind::Import
            | SyntaxKind::ImportFrom
            | SyntaxKind::FunctionDefinition
            | SyntaxKind::ClassDefinition
            // Reported by their own checks, with a better message.
            | SyntaxKind::TypeAlias
            | SyntaxKind::Delete
            | SyntaxKind::Global
            | SyntaxKind::Nonlocal => {}
            SyntaxKind::ExpressionStatement if self.is_documentation(id, previous) => {}
            SyntaxKind::ExpressionStatement if self.is_string(id) => self.stray_string(
                id,
                "documentation goes first in the module, or right after the constant or type \
                 alias it documents",
            ),
            SyntaxKind::Assign if self.assigns_one_name(id) && self.is_typelike(id) => {}
            SyntaxKind::Assign | SyntaxKind::AnnotatedAssign
                if kind == SyntaxKind::AnnotatedAssign || self.assigns_one_name(id) =>
            {
                let annotation = self.tree.child(id, Role::Annotation);
                if !annotation.is_some_and(|annotation| self.is_final(annotation)) {
                    let target = self.tree.child(id, Role::Target);
                    let name = target.map_or("NAME", |target| self.text(target));
                    let suggestion = format!("mark a constant with `Final`: `{name}: Final = ...`");
                    self.error(
                        "module-level variables are not supported",
                        span,
                        "a variable at the top level of a module",
                        &[
                            "a module-level variable is shared mutable state, which a parallel \
                             loop could race on",
                            &suggestion,
                        ],
                    );
                }
            }
            _ => self.error(
                "only definitions may be at the top level of a module",
                span,
                "this is code that would run when the module loads",
                &[
                    "a module holds only imports, `Final` constants, type aliases, functions and \
                     classes; the program starts at its `main` function",
                    "move this into `main` or another function",
                ],
            ),
        }
    }

    /// Checks a statement directly in a class body, which may only declare fields, enum
    /// members and methods.
    pub(crate) fn class_level(&mut self, id: NodeId, previous: Option<NodeId>) {
        match self.tree.kind(id) {
            SyntaxKind::AnnotatedAssign
            | SyntaxKind::Assign
            | SyntaxKind::FunctionDefinition
            | SyntaxKind::Pass
            // Reported by their own checks, with a better message.
            | SyntaxKind::TypeAlias
            | SyntaxKind::Delete
            | SyntaxKind::Global
            | SyntaxKind::Nonlocal => {}
            SyntaxKind::ExpressionStatement if self.is_documentation(id, previous) => {}
            SyntaxKind::ExpressionStatement if self.is_string(id) => self.stray_string(
                id,
                "documentation goes first in the class, or right after the field it documents",
            ),
            _ => self.error(
                "a class body holds only fields, methods and documentation",
                self.tree.span(id),
                "",
                &["move this code into a method"],
            ),
        }
    }

    /// Whether `id`, an expression statement after `previous`, is a documentation string: a
    /// string literal first in a module, class or function, or right after a constant, type
    /// alias, field or enum member that it documents.
    fn is_documentation(&self, id: NodeId, previous: Option<NodeId>) -> bool {
        let follows_declaration = previous.is_some_and(|previous| {
            matches!(
                self.tree.kind(previous),
                SyntaxKind::Assign | SyntaxKind::AnnotatedAssign
            )
        });
        self.is_string(id) && (previous.is_none() || follows_declaration)
    }

    /// Whether `id`, an expression statement, is a string literal on its own line.
    fn is_string(&self, id: NodeId) -> bool {
        self.tree
            .child(id, Role::Value)
            .is_some_and(|value| self.tree.kind(value) == SyntaxKind::StringLiteral)
    }

    /// Reports a string on its own line that is not in a place where it documents anything.
    fn stray_string(&mut self, id: NodeId, hint: &str) {
        self.error(
            "this string documents nothing",
            self.tree.span(id),
            "not in a documentation position",
            &[hint],
        );
    }

    /// Whether `id`, a top-level assignment, assigns one name once, as a type alias does.
    fn assigns_one_name(&self, id: NodeId) -> bool {
        let mut targets = self.tree.children_with(id, Role::Target);
        let first = targets.next();
        targets.next().is_none()
            && first.is_some_and(|target| self.tree.kind(target) == SyntaxKind::Name)
    }

    /// Whether the value of the top-level assignment `id` is shaped like a type, so the
    /// assignment can be a type alias: a name, `a.b`, `T[...]`, `None` or `A | B`. Whether it
    /// really is a type is for the type checker to say. A value of any other shape, such as
    /// `N = 10`, makes a module-level variable, reported as one rather than as a bad type.
    fn is_typelike(&self, id: NodeId) -> bool {
        self.tree
            .child(id, Role::Value)
            .is_some_and(|value| match self.tree.kind(value) {
                SyntaxKind::Name
                | SyntaxKind::Attribute
                | SyntaxKind::Subscript
                | SyntaxKind::None => true,
                SyntaxKind::BinaryOperation => self.tree.node(value).operator() == Operator::BitOr,
                _ => false,
            })
    }

    /// Whether `annotation` is `Final` or `Final[T]`.
    fn is_final(&self, annotation: NodeId) -> bool {
        let name = match self.tree.kind(annotation) {
            SyntaxKind::Subscript => match self.tree.child(annotation, Role::Value) {
                Some(value) => value,
                None => return false,
            },
            _ => annotation,
        };
        self.tree.kind(name) == SyntaxKind::Name && self.text(name) == "Final"
    }

    /// Checks an expression statement in a function: it must be a call, or a documentation
    /// string. Anything else computes a value and throws it away, which is usually a typo.
    pub(crate) fn expression_statement(
        &mut self,
        id: NodeId,
        parent: NodeId,
        previous: Option<NodeId>,
    ) {
        let Some(value) = self.tree.child(id, Role::Value) else {
            return;
        };
        let kind = self.tree.kind(value);
        let is_block_start = matches!(
            self.tree.kind(parent),
            SyntaxKind::FunctionDefinition | SyntaxKind::ClassDefinition
        ) && previous.is_none();
        let accepted = kind == SyntaxKind::Call
            || (kind == SyntaxKind::StringLiteral && is_block_start)
            || (kind == SyntaxKind::Ellipsis && self.is_trait_method(parent))
            || Self::is_rejected_expression(kind)
            || kind == SyntaxKind::Missing;
        if accepted {
            return;
        }
        let comparison = kind == SyntaxKind::Compare
            && self
                .tree
                .children_with(value, Role::Item)
                .next()
                .is_some_and(|comparator| self.tree.node(comparator).operator() == Operator::Equal);
        if kind == SyntaxKind::StringLiteral {
            self.stray_string(id, "documentation goes first in the function");
            return;
        }
        let hint = if comparison {
            "to assign, write `=` instead of `==`"
        } else if kind == SyntaxKind::Ellipsis {
            "write `pass` for an empty block"
        } else {
            "only a call, or a documentation string, can stand on a line by itself"
        };
        self.error(
            "this expression's value is never used",
            self.tree.span(id),
            "computed, then thrown away",
            &[hint],
        );
    }

    /// Whether `function` is a method of a trait, a class with the base `Protocol`, whose body
    /// is `...` (decision 0031). The type checker checks that the body is nothing else.
    ///
    /// Cost: a look at each top-level class's statements; a module has tens of them.
    fn is_trait_method(&self, function: NodeId) -> bool {
        self.tree
            .children_with(self.tree.root(), Role::Body)
            .filter(|&statement| self.tree.kind(statement) == SyntaxKind::ClassDefinition)
            .filter(|&class| {
                self.tree
                    .child(class, Role::Arguments)
                    .into_iter()
                    .flat_map(|arguments| self.tree.children(arguments))
                    .any(|base| {
                        self.tree.kind(base) == SyntaxKind::Name && self.text(base) == "Protocol"
                    })
            })
            .any(|class| {
                self.tree
                    .children_with(class, Role::Body)
                    .any(|method| method == function)
            })
    }
}
