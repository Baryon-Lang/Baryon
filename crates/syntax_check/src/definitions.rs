//! Checks on definitions: where a function may be, its decorators, and a class's bases and
//! decorators.

use crate::{Checker, Scope};
use syntax::{Flags, NodeId, Role, SyntaxKind};

/// The function decorators the language has, both marking functions for the test runner.
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

    /// Checks a function's decorators: only `@test` and `@bench` exist.
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
                        "the only function decorators are `@test` and `@bench`, which mark test \
                         and benchmark functions",
                    ],
                );
            }
        }
    }

    /// Checks a class definition: its bases and its decorators. A class takes exactly one of
    /// the bases `Struct`, `Struct, Copy`, `Exception` and `Enum`: there is no inheritance
    /// (decision 0015).
    pub(crate) fn class(&mut self, id: NodeId) {
        self.class_decorators(id);
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

    /// Checks a class's decorators: only `@layout("C")` exists, and only on a class with the
    /// base `Struct` (decision 0017). Which bases may be combined is checked with the bases.
    fn class_decorators(&mut self, id: NodeId) {
        let is_struct = self
            .tree
            .child(id, Role::Arguments)
            .into_iter()
            .flat_map(|arguments| self.tree.children(arguments))
            .any(|base| self.tree.kind(base) == SyntaxKind::Name && self.text(base) == "Struct");
        let decorators: Vec<NodeId> = self.tree.children_with(id, Role::Decorator).collect();
        for decorator in decorators {
            let span = self.tree.span(decorator);
            let Some(value) = self.tree.child(decorator, Role::Value) else {
                continue;
            };
            let call = (self.tree.kind(value) == SyntaxKind::Call)
                .then(|| self.tree.child(value, Role::Function))
                .flatten();
            let named = call.unwrap_or(value);
            if self.tree.kind(named) != SyntaxKind::Name || self.text(named) != "layout" {
                self.error(
                    "this decorator is not supported",
                    span,
                    "",
                    &[
                        "the only class decorator is `@layout(\"C\")`, which keeps a struct's \
                         fields in the order written without the padding warning",
                    ],
                );
                continue;
            }
            if !self.is_c_layout(value) {
                self.error(
                    "`@layout` takes one argument, `\"C\"`",
                    span,
                    "",
                    &[
                        "C layout is the only one: fields in the order written, each at the next \
                         offset its alignment allows",
                    ],
                );
                continue;
            }
            if !is_struct {
                self.error(
                    "`@layout` applies only to structs",
                    span,
                    "",
                    &[
                        "enums and error types are never shared with other programs byte for \
                         byte, so their layout is the compiler's",
                    ],
                );
            }
        }
    }

    /// Whether the decorator expression `value` is exactly `layout("C")`.
    fn is_c_layout(&self, value: NodeId) -> bool {
        if self.tree.kind(value) != SyntaxKind::Call {
            return false;
        }
        let items: Vec<NodeId> = self
            .tree
            .child(value, Role::Arguments)
            .into_iter()
            .flat_map(|arguments| self.tree.children(arguments))
            .collect();
        matches!(items.as_slice(), [only]
            if self.tree.kind(*only) == SyntaxKind::StringLiteral
                && ["\"C\"", "'C'"].contains(&self.text(*only)))
    }
}
