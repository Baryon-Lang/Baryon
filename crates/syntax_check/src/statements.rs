//! Checks on statements: which kinds the language rejects outright and which clauses it
//! rejects on the kinds it accepts. Where each kind may appear is checked in
//! [`placement`](crate::placement).

use crate::{Checker, Scope};
use syntax::{Flags, NodeId, Role, SyntaxKind};

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
            SyntaxKind::Delete => self.delete(id),
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

    /// Checks a `with` statement: not `async`, and each name after `as` is a single name. Which
    /// values `with` takes, `unsafe`, a file or a context manager, is the type checker's to say
    /// (decisions 0036 and 0053).
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
        for item in items {
            let Some(target) = self.tree.child(item, Role::Target) else {
                continue;
            };
            if self.tree.kind(target) == SyntaxKind::Name {
                continue;
            }
            self.error(
                "the target of `with ... as` must be a single name",
                self.tree.span(target),
                "not a name",
                &["unpack the value on the next line: `with m as pair:` then `a, b = pair`"],
            );
        }
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

    /// Reports a `del` statement unless it removes one subscript, `del d[k]`, which removes a
    /// `dict` entry (the type checker rejects it on anything else). `del x` and `del a, b` have
    /// no use: a value is destroyed when its owner goes out of scope.
    fn delete(&mut self, id: NodeId) {
        let targets: Vec<NodeId> = self.tree.children_with(id, Role::Target).collect();
        if let [target] = targets.as_slice()
            && self.tree.kind(*target) == SyntaxKind::Subscript
        {
            return;
        }
        self.error(
            "`del` is supported only on a dict entry",
            self.tree.span(id),
            "",
            &[
                "a value is destroyed when its owner goes out of scope, so nothing else needs `del`",
                "remove one dict entry with `del d[k]`",
            ],
        );
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
