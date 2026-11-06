//! Checks on definitions: where a function may be, its decorators, and a class's bases.

use crate::{Checker, Scope};
use syntax::{Flags, NodeId, Role, SyntaxKind};

/// The decorators the language has, both marking functions for the test runner.
const DECORATORS: [&str; 2] = ["test", "bench"];

impl Checker<'_> {
    /// Checks a function definition: where it is, `async`, and its decorators.
    pub(crate) fn function(&mut self, id: NodeId, scope: Scope) {
        let name = self.tree.child(id, Role::Name);
        let name_span = name.map_or_else(|| self.tree.span(id), |name| self.tree.span(name));
        if scope == Scope::Function {
            self.error(
                "functions cannot be defined inside functions",
                name_span,
                "a nested function",
                &[
                    "nested functions are not in version 0",
                    "move it to the top level of the module, and pass what it uses as parameters",
                ],
            );
        }
        if self.tree.node(id).flags().contains(Flags::ASYNC) {
            let from = self
                .tree
                .children_with(id, Role::Decorator)
                .last()
                .map_or(self.tree.node(id).start(), |decorator| {
                    self.tree.node(decorator).end()
                });
            let span = self.keyword("async", from, name_span.start(), name_span);
            self.report_async("async", span);
        }
        self.decorators(id);
    }

    /// Checks a definition's decorators: only `@test` and `@bench` exist.
    fn decorators(&mut self, id: NodeId) {
        let decorators: Vec<NodeId> = self.tree.children_with(id, Role::Decorator).collect();
        for decorator in decorators {
            let value = self.tree.child(decorator, Role::Value);
            let known = value.is_some_and(|value| {
                self.tree.kind(value) == SyntaxKind::Name && DECORATORS.contains(&self.text(value))
            });
            if !known {
                self.error(
                    "this decorator is not supported",
                    self.tree.span(decorator),
                    "",
                    &[
                        "the only decorators are `@test` and `@bench`, which mark test and \
                       benchmark functions",
                    ],
                );
            }
        }
    }

    /// Checks a class definition: its decorators and its bases. A class takes exactly one of
    /// the bases `Struct`, `Struct, Copy`, `Exception` and `Enum`: there is no inheritance
    /// (decision 0015).
    pub(crate) fn class(&mut self, id: NodeId) {
        self.decorators(id);
        let name = self
            .tree
            .child(id, Role::Name)
            .map_or("Name", |name| self.text(name));
        let Some(arguments) = self.tree.child(id, Role::Arguments) else {
            let span = self
                .tree
                .child(id, Role::Name)
                .map_or_else(|| self.tree.span(id), |name| self.tree.span(name));
            let suggestion = format!("for a struct, write `class {name}(Struct):`");
            self.error(
                "a class needs a base: `Struct`, `Exception` or `Enum`",
                span,
                "no base",
                &[
                    "the base says what the class is: a struct, an error type or an enum",
                    &suggestion,
                ],
            );
            return;
        };
        let mut bases = Vec::new();
        let mut valid = true;
        let items: Vec<NodeId> = self.tree.children(arguments).collect();
        for base in items {
            let text = self.text(base);
            let is_known = self.tree.kind(base) == SyntaxKind::Name
                && ["Struct", "Copy", "Exception", "Enum"].contains(&text);
            if is_known {
                bases.push(text);
            } else {
                valid = false;
                if self.tree.kind(base) == SyntaxKind::Keyword {
                    self.error(
                        format!("class keyword `{text}` is not supported"),
                        self.tree.span(base),
                        "",
                        &["metaclasses and class keywords are not in version 0"],
                    );
                } else {
                    self.error(
                        format!("a class cannot inherit from `{text}`"),
                        self.tree.span(base),
                        "not one of `Struct`, `Copy`, `Exception` or `Enum`",
                        &[
                            "there is no class inheritance (decision 0015)",
                            "share behavior through traits, and data by holding the other \
                             struct in a field",
                        ],
                    );
                }
            }
        }
        let allowed: [&[&str]; 4] = [&["Struct"], &["Struct", "Copy"], &["Exception"], &["Enum"]];
        if valid && !allowed.contains(&bases.as_slice()) {
            self.error(
                "these bases cannot be combined",
                self.tree.span(arguments),
                "",
                &["a class's bases are `Struct`, `Struct, Copy`, `Exception` or `Enum`"],
            );
        }
    }
}
