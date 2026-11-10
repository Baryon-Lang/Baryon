//! The syntax check: reports every Python construct the language does not support.
//!
//! Every source file is valid Python 3.12 (decision 0004), and the parser accepts all of it. Not
//! all of Python is in the language, though: `lambda`, `global`, `try` with `finally` and the
//! rest of the "no" rows of the [syntax chapter] of the language reference are rejected. This
//! crate is the stage that rejects them. It runs right after the parser, reads the parser's
//! [`SyntaxTree`] and returns one [`Diagnostic`] for each unsupported construct, saying what it
//! is, why the language leaves it out, and what to write instead where there is a spelling.
//!
//! Keeping the list here, not in the parser, means it can grow or shrink as the language does
//! without touching the parser, and a replacement parser gets the same checks for free. The
//! check is an implementation crate that reads only the `syntax` interface (decision 0001).
//!
//! # What it rejects
//!
//! - Statements: star targets `a, *rest = e`, `else` on `for` and `while`, functions nested in
//!   functions, `del`, `global`, `nonlocal`, `async`, the `type` statement, `from m import *`,
//!   bare `except:`, `except*`, `try` with `else` or `finally`, `raise ... from`, and `with`
//!   other than `with unsafe:`.
//! - Expressions: `lambda`, `await`, `yield`, dictionary and set literals and comprehensions,
//!   generator expressions, `:=`, `*` and `**` unpacking, and `is` with anything but `None`.
//! - Definitions: `*args` and `**kwargs` parameters, function decorators other than `@test`
//!   and `@bench`, class decorators other than `@layout("C")` on a struct, and class bases
//!   other than `Struct`, `Struct, Copy`, `Exception` and `Enum`: there is no class inheritance
//!   (decision 0015).
//! - Placement: top-level statements other than imports, `Final` constants, type aliases,
//!   functions, classes and documentation strings; class bodies holding anything but fields,
//!   methods and documentation; and an expression on its own line that is not a call.
//!
//! What it does not check: anything that needs names or types, such as whether a module-level
//! `X = ...` is really a type, or whether a `raise` is in a function declared to fail. Those
//! belong to name resolution and the type checker.
//!
//! [syntax chapter]: https://github.com/Baryon-Lang/Baryon/blob/main/docs/reference/syntax.md
//!
//! # Example
//!
//! Trees normally come from the parser. Built by hand, `del x` is rejected like this:
//!
//! ```
//! use source::SourceMap;
//! use syntax::{Role, SyntaxKind, TreeBuilder};
//!
//! let mut sources = SourceMap::new();
//! let text = "del x\n";
//! let file = sources.add("main.byn", text)?;
//!
//! let mut builder = TreeBuilder::new(file);
//! builder.open(SyntaxKind::Module, Role::Root, 0, 6);
//! builder.open(SyntaxKind::Delete, Role::Body, 0, 5);
//! builder.leaf(SyntaxKind::Name, Role::Target, 4, 5);
//! builder.close();
//! builder.close();
//! let tree = builder.finish();
//!
//! let diagnostics = syntax_check::check(&tree, text);
//! assert_eq!(diagnostics.len(), 1);
//! assert_eq!(diagnostics[0].message(), "`del` is not supported");
//! # Ok::<(), source::SourceError>(())
//! ```

mod definitions;
mod expressions;
mod statements;

use diagnostics::Diagnostic;
use source::Span;
use syntax::{NodeId, Role, SyntaxKind, SyntaxTree};

/// Checks `tree`, parsed from `source`, and returns one error per unsupported construct, in
/// source order. An empty result means the file uses only what the language supports.
///
/// `source` must be the text the tree was parsed from: some checks read names from it, such as
/// the bases of a class. A tree with syntax errors can be checked too; its
/// [`SyntaxKind::Missing`] nodes are skipped, so the result holds only this stage's findings.
///
/// Cost: one pass over the tree, linear in its size, with no allocation for a file that passes.
#[must_use]
pub fn check(tree: &SyntaxTree, source: &str) -> Vec<Diagnostic> {
    let mut checker = Checker {
        tree,
        source,
        diagnostics: Vec::new(),
    };
    let context = Context {
        scope: Scope::Module,
        in_target: false,
    };
    checker.children(tree.root(), context);
    // The walk is in preorder, so diagnostics are almost sorted already; a few, such as an
    // `else` reported on its keyword after the loop's body, come out of order.
    checker
        .diagnostics
        .sort_by_key(|diagnostic| diagnostic.primary().map(|label| label.span().start()));
    checker.diagnostics
}

/// The kind of block a statement sits in, which decides what it may be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    /// The top level of a module, including blocks nested in a top-level statement.
    Module,
    /// A class body.
    Class,
    /// A function body, including every block nested in it.
    Function,
}

/// What a node's ancestors say about it.
#[derive(Clone, Copy, Debug)]
struct Context {
    /// The kind of block the node is in.
    scope: Scope,
    /// Whether the node is, or is inside, the target of an assignment or `for` loop, where `*`
    /// means a star target rather than unpacking.
    in_target: bool,
}

/// One run of the check over one tree.
struct Checker<'a> {
    /// The tree being checked.
    tree: &'a SyntaxTree,
    /// The text the tree was parsed from.
    source: &'a str,
    /// What has been found so far, in the order it was found.
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Checker<'a> {
    /// Visits every child of `parent`, which sits in `context`.
    fn children(&mut self, parent: NodeId, context: Context) {
        let parent_kind = self.tree.kind(parent);
        let scope = match parent_kind {
            SyntaxKind::FunctionDefinition => Scope::Function,
            SyntaxKind::ClassDefinition => Scope::Class,
            _ => context.scope,
        };
        let mut previous = None;
        for child in self.tree.children(parent) {
            let role = self.tree.node(child).role();
            let in_target = match parent_kind {
                SyntaxKind::Assign
                | SyntaxKind::AugmentedAssign
                | SyntaxKind::AnnotatedAssign
                | SyntaxKind::For
                | SyntaxKind::Comprehension => role == Role::Target,
                SyntaxKind::Tuple | SyntaxKind::List | SyntaxKind::Starred => context.in_target,
                _ => false,
            };
            let child_context = Context { scope, in_target };
            self.visit(child, parent, previous, child_context);
            if self.tree.kind(child).is_statement() {
                previous = Some(child);
            }
        }
    }

    /// Checks one node, then its children unless the node itself was rejected as a whole.
    ///
    /// `previous` is the statement before it in the same block, if any, which decides whether a
    /// string on its own line is documentation.
    fn visit(&mut self, id: NodeId, parent: NodeId, previous: Option<NodeId>, context: Context) {
        let kind = self.tree.kind(id);
        let descend = if kind.is_statement() {
            self.statement(id, parent, previous, context.scope)
        } else {
            self.expression(id, parent, context)
        };
        if descend {
            self.children(id, context);
        }
    }

    /// Records an error at `span` with a label and notes.
    fn error(&mut self, message: impl Into<String>, span: Span, label: &str, notes: &[&str]) {
        let mut diagnostic = Diagnostic::error(message).with_primary(span, label);
        for note in notes {
            diagnostic = diagnostic.with_note(*note);
        }
        self.diagnostics.push(diagnostic);
    }

    /// The source text of a node.
    fn text(&self, id: NodeId) -> &'a str {
        self.tree.text(id, self.source)
    }

    /// The span of the first `keyword` that starts a line between byte offsets `from` and `to`,
    /// or `fallback` if there is none.
    ///
    /// Keywords such as `else` and `finally` are not nodes, so to point at one the check finds
    /// it in the text between the nodes around it. Only a keyword that starts its line counts,
    /// which skips the same word in a comment or a string.
    fn keyword(&self, keyword: &str, from: u32, to: u32, fallback: Span) -> Span {
        let region = self
            .source
            .get(from as usize..to as usize)
            .unwrap_or_default();
        let mut offset = from as usize;
        for line in region.split_inclusive('\n') {
            let trimmed = line.trim_start();
            let start = offset + (line.len() - trimmed.len());
            offset += line.len();
            let Some(rest) = trimmed.strip_prefix(keyword) else {
                continue;
            };
            if rest
                .chars()
                .next()
                .is_none_or(|next| !(next.is_alphanumeric() || next == '_'))
            {
                // Offsets come from a tree over a source of at most 4 GiB, so they fit.
                let start = u32::try_from(start).unwrap_or(from);
                let end = start.saturating_add(u32::try_from(keyword.len()).unwrap_or(0));
                return Span::new(fallback.file(), start, end);
            }
        }
        fallback
    }

    /// The span of the keyword that opens the clause starting at `clause`, the first child of
    /// the clause in `parent`, such as the `else` before a loop's first `else` statement.
    fn clause_keyword(&self, keyword: &str, parent: NodeId, clause: NodeId) -> Span {
        let before = self
            .tree
            .children(parent)
            .take_while(|&child| child != clause)
            .last()
            .map_or(self.tree.node(parent).start(), |child| {
                self.tree.node(child).end()
            });
        let fallback = self.tree.span(clause);
        self.keyword(keyword, before, fallback.start(), fallback)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use source::SourceMap;
    use syntax::{Operator, TreeBuilder};

    /// Checks a tree built by `build` over `text`, returning each diagnostic's message.
    fn messages(text: &str, build: impl FnOnce(&mut TreeBuilder)) -> Vec<String> {
        let file = SourceMap::new().add("test.byn", text).unwrap();
        let mut builder = TreeBuilder::new(file);
        build(&mut builder);
        let tree = builder.finish();
        check(&tree, text)
            .iter()
            .map(|diagnostic| diagnostic.message().to_owned())
            .collect()
    }

    #[test]
    fn an_empty_module_passes() {
        let found = messages("", |builder| {
            builder.open(SyntaxKind::Module, Role::Root, 0, 0);
            builder.close();
        });
        assert!(found.is_empty());
    }

    #[test]
    fn a_module_docstring_passes_but_a_top_level_call_does_not() {
        let text = "\"doc\"\nmain()\n";
        let found = messages(text, |builder| {
            builder.open(SyntaxKind::Module, Role::Root, 0, 14);
            builder.open(SyntaxKind::ExpressionStatement, Role::Body, 0, 5);
            builder.leaf(SyntaxKind::StringLiteral, Role::Value, 0, 5);
            builder.close();
            builder.open(SyntaxKind::ExpressionStatement, Role::Body, 6, 12);
            builder.open(SyntaxKind::Call, Role::Value, 6, 12);
            builder.leaf(SyntaxKind::Name, Role::Function, 6, 10);
            builder.open(SyntaxKind::Arguments, Role::Arguments, 10, 12);
            builder.close();
            builder.close();
            builder.close();
            builder.close();
        });
        assert_eq!(
            found,
            ["only definitions may be at the top level of a module"]
        );
    }

    #[test]
    fn is_passes_with_none_and_fails_without() {
        let text = "def f():\n    g(a is None, a is b)\n";
        let found = messages(text, |builder| {
            builder.open(SyntaxKind::Module, Role::Root, 0, 34);
            builder.open(SyntaxKind::FunctionDefinition, Role::Body, 0, 33);
            builder.leaf(SyntaxKind::Name, Role::Name, 4, 5);
            builder.open(SyntaxKind::Parameters, Role::Parameters, 5, 7);
            builder.close();
            builder.open(SyntaxKind::ExpressionStatement, Role::Body, 13, 33);
            builder.open(SyntaxKind::Call, Role::Value, 13, 33);
            builder.leaf(SyntaxKind::Name, Role::Function, 13, 14);
            builder.open(SyntaxKind::Arguments, Role::Arguments, 14, 33);
            for (left, right, kind) in [(15, 24, SyntaxKind::None), (26, 32, SyntaxKind::Name)] {
                builder.open(SyntaxKind::Compare, Role::Item, left, right);
                builder.leaf(SyntaxKind::Name, Role::Left, left, left + 1);
                builder.open(SyntaxKind::Comparator, Role::Item, left + 1, right);
                builder.set_operator(Operator::Is);
                let operand = if kind == SyntaxKind::None { 4 } else { 1 };
                builder.leaf(kind, Role::Right, right - operand, right);
                builder.close();
                builder.close();
            }
            for _ in 0..5 {
                builder.close();
            }
        });
        assert_eq!(found, ["`is` compares only with `None`"]);
    }

    #[test]
    fn a_keyword_is_found_only_at_the_start_of_a_line() {
        let text = "x = 1  # else\n  else:\n";
        let file = SourceMap::new().add("test.byn", text).unwrap();
        let mut builder = TreeBuilder::new(file);
        builder.leaf(SyntaxKind::Module, Role::Root, 0, 22);
        let tree = builder.finish();
        let checker = Checker {
            tree: &tree,
            source: text,
            diagnostics: Vec::new(),
        };
        let fallback = Span::new(file, 0, 1);
        let found = checker.keyword("else", 0, 22, fallback);
        assert_eq!((found.start(), found.end()), (16, 20));
        let missing = checker.keyword("finally", 0, 22, fallback);
        assert_eq!(missing, fallback);
    }
}
