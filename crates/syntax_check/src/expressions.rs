//! Checks on expressions and the other nodes inside statements: parameters, arguments and
//! comprehension clauses.

use crate::{Checker, Context};
use syntax::{Flags, NodeId, Operator, Role, SyntaxKind};

impl Checker<'_> {
    /// Checks `id`, a node that is not a statement, whose parent is `parent`. Returns whether to
    /// check its children: a rejected construct is reported once, as a whole, and what is
    /// inside it is not checked, since it will be rewritten anyway.
    pub(crate) fn expression(&mut self, id: NodeId, parent: NodeId, context: Context) -> bool {
        let kind = self.tree.kind(id);
        let parent_kind = self.tree.kind(parent);
        if Self::is_rejected_expression(kind) {
            self.rejected_expression(id, parent_kind, context);
            return false;
        }
        match kind {
            // A class's bases are checked with the class.
            SyntaxKind::Arguments if parent_kind == SyntaxKind::ClassDefinition => return false,
            SyntaxKind::DoubleStarred if parent_kind == SyntaxKind::Arguments => {
                self.error(
                    "`**` unpacking in a call is not supported",
                    self.tree.span(id),
                    "",
                    &["pass each argument by name"],
                );
                return false;
            }
            SyntaxKind::StarParameter => {
                self.error(
                    "`*args` parameters are not supported",
                    self.tree.span(id),
                    "",
                    &["declare each parameter, or take a `list[T]` or an `Array`"],
                );
                return false;
            }
            SyntaxKind::DoubleStarParameter => {
                self.error(
                    "`**kwargs` parameters are not supported",
                    self.tree.span(id),
                    "",
                    &["declare each parameter by name; one with a default value is optional"],
                );
                return false;
            }
            SyntaxKind::Comprehension if self.tree.node(id).flags().contains(Flags::ASYNC) => {
                let start = self.tree.node(id).start();
                self.report_async(
                    "async",
                    source::Span::new(self.tree.file(), start, start + 5),
                );
            }
            SyntaxKind::Compare => self.identity(id),
            _ => {}
        }
        true
    }

    /// Whether every node of this kind is rejected, wherever it appears.
    pub(crate) fn is_rejected_expression(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::Lambda
                | SyntaxKind::DictionaryComprehension
                | SyntaxKind::Set
                | SyntaxKind::SetComprehension
                | SyntaxKind::Generator
                | SyntaxKind::NamedExpression
                | SyntaxKind::Await
                | SyntaxKind::Yield
                | SyntaxKind::YieldFrom
                | SyntaxKind::Starred
        )
    }

    /// Reports `id`, whose kind [`is_rejected_expression`](Self::is_rejected_expression).
    fn rejected_expression(&mut self, id: NodeId, parent_kind: SyntaxKind, context: Context) {
        let span = self.tree.span(id);
        match self.tree.kind(id) {
            SyntaxKind::Lambda => self.error(
                "`lambda` is not supported",
                span,
                "",
                &["pass a function defined with `def` at the top level of the module instead"],
            ),
            SyntaxKind::DictionaryComprehension => self.error(
                "dict comprehensions are not supported yet",
                span,
                "",
                &["build the dict in a loop: `d[k] = v`"],
            ),
            SyntaxKind::Set | SyntaxKind::SetComprehension => self.error(
                "sets are not supported yet",
                span,
                "",
                &["sets are not in version 0; use a `list`"],
            ),
            SyntaxKind::Generator => {
                let suggestion = self.list_comprehension(id);
                self.error(
                    "generator expressions are not supported",
                    span,
                    "",
                    &["generators are not in version 0", &suggestion],
                );
            }
            SyntaxKind::NamedExpression => {
                let suggestion = match (
                    self.tree.child(id, Role::Target),
                    self.tree.child(id, Role::Value),
                ) {
                    (Some(target), Some(value)) if !self.text(value).contains('\n') => format!(
                        "assign on the line before: `{} = {}`",
                        self.text(target),
                        self.text(value)
                    ),
                    _ => "assign on the line before".to_owned(),
                };
                self.error(
                    "assignment expressions `:=` are not supported",
                    span,
                    "",
                    &[&suggestion],
                );
            }
            SyntaxKind::Await => self.report_async("await", span),
            SyntaxKind::Yield | SyntaxKind::YieldFrom => self.error(
                "`yield` is not supported",
                span,
                "",
                &["generators are not in version 0; build and return a `list` instead"],
            ),
            _ => self.starred(id, parent_kind, context),
        }
    }

    /// Reports `*value`, with a message for where it is.
    fn starred(&mut self, id: NodeId, parent_kind: SyntaxKind, context: Context) {
        let span = self.tree.span(id);
        if context.in_target {
            self.error(
                "star targets are not supported",
                span,
                "",
                &[
                    "unpack a fixed number of names, `a, b = pair`, or index and slice the \
                   sequence: `first = items[0]`",
                ],
            );
        } else if parent_kind == SyntaxKind::Arguments {
            self.error(
                "`*` unpacking in a call is not supported",
                span,
                "",
                &["pass each argument by position or by name"],
            );
        } else {
            self.error(
                "`*` unpacking is not supported",
                span,
                "",
                &["build the collection with a loop, or join lists with `+`"],
            );
        }
    }

    /// The list comprehension that replaces the generator expression `id`.
    fn list_comprehension(&self, id: NodeId) -> String {
        let text = self.text(id);
        let inner = if self.tree.node(id).flags().contains(Flags::PARENTHESIZED) {
            text.get(1..text.len().saturating_sub(1)).unwrap_or(text)
        } else {
            text
        };
        if inner.contains('\n') {
            "write a list comprehension, in `[` and `]`, instead".to_owned()
        } else {
            format!("write a list comprehension instead: `[{}]`", inner.trim())
        }
    }

    /// Checks a comparison chain: `is` and `is not` compare only with `None`.
    fn identity(&mut self, id: NodeId) {
        let mut operand = self.tree.child(id, Role::Left);
        let comparators: Vec<NodeId> = self.tree.children_with(id, Role::Item).collect();
        for comparator in comparators {
            let right = self.tree.child(comparator, Role::Right);
            let operator = self.tree.node(comparator).operator();
            let is_none = |node: Option<NodeId>| {
                node.is_some_and(|node| self.tree.kind(node) == SyntaxKind::None)
            };
            if matches!(operator, Operator::Is | Operator::IsNot)
                && !is_none(operand)
                && !is_none(right)
            {
                let equality = if operator == Operator::Is { "==" } else { "!=" };
                let suggestion = format!("to compare values, write `{equality}`");
                let start = operand.map_or(self.tree.node(comparator).start(), |operand| {
                    self.tree.node(operand).start()
                });
                let end = self.tree.node(comparator).end();
                self.error(
                    "`is` compares only with `None`",
                    source::Span::new(self.tree.file(), start, end),
                    "",
                    &["`x is None` asks whether a value is missing", &suggestion],
                );
            }
            operand = right;
        }
    }
}
