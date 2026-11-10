//! Checks on statements: which kinds the language rejects outright, which clauses it rejects on
//! the kinds it accepts, and where each kind may appear.

use crate::{Checker, Scope};
use syntax::{Flags, NodeId, Operator, Role, SyntaxKind};

impl Checker<'_> {
    /// Checks the statement `id`, a child of `parent` in a block of `scope`, after the statement
    /// `previous` in that block. Returns whether to check its children, which is always: a
    /// statement's body is code like any other, whatever is wrong with the statement.
    pub(crate) fn statement(
        &mut self,
        id: NodeId,
        parent: NodeId,
        previous: Option<NodeId>,
        scope: Scope,
    ) -> bool {
        match self.tree.kind(parent) {
            SyntaxKind::Module => self.top_level(id, previous),
            SyntaxKind::ClassDefinition => self.class_level(id, previous),
            _ => {}
        }
        match self.tree.kind(id) {
            SyntaxKind::FunctionDefinition => self.function(id, scope),
            SyntaxKind::ClassDefinition => self.class(id),
            SyntaxKind::For => self.for_loop(id),
            SyntaxKind::While => self.loop_else(id, "while"),
            SyntaxKind::Raise => self.raise_from(id),
            SyntaxKind::With => self.with(id),
            SyntaxKind::Try => self.try_statement(id),
            SyntaxKind::ImportFrom => self.import_from(id),
            SyntaxKind::TypeAlias => self.type_statement(id),
            SyntaxKind::Delete => self.error(
                "`del` is not supported",
                self.tree.span(id),
                "",
                &["a value is destroyed when its owner goes out of scope, so nothing needs `del`"],
            ),
            SyntaxKind::Global => self.error(
                "`global` is not supported",
                self.tree.span(id),
                "",
                &[
                    "there are no module-level variables",
                    "pass the value as a parameter, declared `InOut[T]` if the function changes it",
                ],
            ),
            SyntaxKind::Nonlocal => self.error(
                "`nonlocal` is not supported",
                self.tree.span(id),
                "",
                &[
                    "functions do not nest, so there is no enclosing function's variable to change",
                    "pass the value as a parameter, declared `InOut[T]` if the function changes it",
                ],
            ),
            SyntaxKind::ExpressionStatement if scope == Scope::Function => {
                self.expression_statement(id, parent, previous);
            }
            _ => {}
        }
        true
    }

    /// Checks a statement at the top level of a module, which may only define things.
    fn top_level(&mut self, id: NodeId, previous: Option<NodeId>) {
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
    fn class_level(&mut self, id: NodeId, previous: Option<NodeId>) {
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
    fn expression_statement(&mut self, id: NodeId, parent: NodeId, previous: Option<NodeId>) {
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

    /// Checks a `for` loop: no `async for`, no `else`.
    fn for_loop(&mut self, id: NodeId) {
        let node = self.tree.node(id);
        if node.flags().contains(Flags::ASYNC) {
            let start = node.start();
            self.report_async(
                "async",
                source::Span::new(self.tree.file(), start, start + 5),
            );
        }
        self.loop_else(id, "for");
    }

    /// Reports the `else` clause of a `for` or `while` loop, if it has one.
    fn loop_else(&mut self, id: NodeId, keyword: &str) {
        let Some(clause) = self.tree.child(id, Role::Else) else {
            return;
        };
        let span = self.clause_keyword("else", id, clause);
        self.error(
            format!("`{keyword}` with `else` is not supported"),
            span,
            "this `else` belongs to the loop",
            &[
                "a loop's `else` runs when the loop ends without `break`, which readers often \
                 misread",
                "set a flag before `break` and test it after the loop",
            ],
        );
    }

    /// Checks a `raise`: `raise E(...) from cause` is rejected, because an error is a value that
    /// reaches the caller alone and never carries another error (decision 0034). A cause the
    /// program needs is kept in a field of the new error, where it is visible in the type.
    fn raise_from(&mut self, id: NodeId) {
        let Some(cause) = self.tree.child(id, Role::Cause) else {
            return;
        };
        let span = self.clause_keyword("from", id, cause);
        let note = if self.tree.kind(cause) == SyntaxKind::None {
            "errors never chain, so there is nothing for `from None` to hide: remove it"
        } else {
            "keep the cause in a field of the new error: `raise LoadError(path, cause=error)`"
        };
        self.error(
            "`raise ... from` is not supported",
            span,
            "errors do not chain",
            &[
                "an error is a value that reaches the caller on its own, without the error that \
                 led to it",
                note,
            ],
        );
    }

    /// Checks a `with` statement: the only form is `with unsafe:`.
    fn with(&mut self, id: NodeId) {
        let node = self.tree.node(id);
        if node.flags().contains(Flags::ASYNC) {
            let start = node.start();
            self.report_async(
                "async",
                source::Span::new(self.tree.file(), start, start + 5),
            );
        }
        let items: Vec<NodeId> = self.tree.children_with(id, Role::Item).collect();
        let is_unsafe = |item: NodeId| {
            self.tree.child(item, Role::Target).is_none()
                && self.tree.child(item, Role::Value).is_some_and(|value| {
                    self.tree.kind(value) == SyntaxKind::Name && self.text(value) == "unsafe"
                })
        };
        if items.len() == 1 && is_unsafe(items[0]) {
            return;
        }
        let span = match (items.first(), items.last()) {
            (Some(&first), Some(&last)) => source::Span::new(
                self.tree.file(),
                self.tree.node(first).start(),
                self.tree.node(last).end(),
            ),
            _ => self.tree.span(id),
        };
        self.error(
            "`with` is supported only as `with unsafe:`",
            span,
            "",
            &[
                "context managers are not in version 0",
                "cleanup is automatic: every value is destroyed when its owner goes out of scope",
            ],
        );
    }

    /// Checks a `try` statement: every `except` names its types, and there is no `except*`,
    /// `else` or `finally`.
    fn try_statement(&mut self, id: NodeId) {
        let handlers: Vec<NodeId> = self.tree.children_with(id, Role::Handler).collect();
        if self.tree.node(id).flags().contains(Flags::STAR) {
            let span = handlers.first().map_or_else(
                || self.tree.span(id),
                |&handler| {
                    let start = self.tree.node(handler).start();
                    source::Span::new(self.tree.file(), start, start + 6)
                },
            );
            self.error(
                "`except*` is not supported",
                span,
                "",
                &["exception groups are not in version 0; write `except` instead"],
            );
        }
        for handler in handlers {
            if self.tree.child(handler, Role::Type).is_none() {
                let start = self.tree.node(handler).start();
                self.error(
                    "`except` must name the error types it handles",
                    source::Span::new(self.tree.file(), start, start + 6),
                    "catches every error",
                    &[
                        "a function's signature lists every error it can raise, so the handler \
                         can name them",
                        "write `except ErrorType:`, or `except (FirstError, SecondError):`",
                    ],
                );
            }
        }
        if let Some(clause) = self.tree.child(id, Role::Else) {
            let span = self.clause_keyword("else", id, clause);
            self.error(
                "`try` with `else` is not supported",
                span,
                "",
                &["move this code to the end of the `try` body"],
            );
        }
        if let Some(clause) = self.tree.child(id, Role::Finally) {
            let span = self.clause_keyword("finally", id, clause);
            self.error(
                "`try` with `finally` is not supported",
                span,
                "",
                &[
                    "cleanup is automatic: every value is destroyed when its owner goes out of \
                   scope, on every path out of a function",
                ],
            );
        }
    }

    /// Checks `from module import ...`: no `*`.
    fn import_from(&mut self, id: NodeId) {
        let star = self.tree.children_with(id, Role::Item).find(|&alias| {
            self.tree
                .child(alias, Role::Name)
                .is_some_and(|name| self.text(name) == "*")
        });
        if let Some(alias) = star {
            let module = self
                .tree
                .child(id, Role::Module)
                .map_or("module", |module| self.text(module));
            let suggestion = format!("name each import: `from {module} import first, second`");
            self.error(
                "`import *` is not supported",
                self.tree.span(alias),
                "imports every name",
                &[
                    "a name's origin should be visible in the file that uses it",
                    &suggestion,
                ],
            );
        }
    }

    /// Reports the `type X = ...` statement and gives the assignment that replaces it.
    fn type_statement(&mut self, id: NodeId) {
        let name = self.tree.child(id, Role::Name);
        let value = self.tree.child(id, Role::Value);
        let suggestion = match (name, value) {
            (Some(name), Some(value)) if !self.text(value).contains('\n') => {
                format!("write it as `{} = {}`", self.text(name), self.text(value))
            }
            _ => "write it as an assignment: `Name = type`".to_owned(),
        };
        self.error(
            "the `type` statement is not supported",
            self.tree.span(id),
            "",
            &[
                "a type alias is an assignment, so there is one spelling for one thing",
                &suggestion,
            ],
        );
    }

    /// Reports `keyword`, which is `async` or `await`, at `span`.
    pub(crate) fn report_async(&mut self, keyword: &str, span: source::Span) {
        self.error(
            format!("`{keyword}` is not supported"),
            span,
            "",
            &["version 0 has no asynchronous code; parallel work uses `prange`"],
        );
    }
}
