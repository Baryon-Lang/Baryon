//! The tree as indented text: how tests and the parser corpus write expected trees.

use crate::{NodeId, Role, SyntaxTree};
use std::fmt;

/// A syntax tree ready to print as text. Created by [`SyntaxTree::display`].
///
/// One line per node, indented two spaces per level:
///
/// ```text
/// Kind @Role operator flags start..end "leaf text"
/// ```
///
/// The role is left out when it is `Item` or `Root`, and the operator and flags when the node
/// has none. Leaf text is printed as a Rust string literal, so newlines and quotes stay on one
/// line.
///
/// ```
/// use source::FileId;
/// use syntax::{Role, SyntaxKind, TreeBuilder};
///
/// let text = "print(\"hi\")";
/// let mut builder = TreeBuilder::new(FileId::from_index(0));
/// builder.open(SyntaxKind::Module, Role::Root, 0, 11);
/// builder.open(SyntaxKind::ExpressionStatement, Role::Body, 0, 11);
/// builder.open(SyntaxKind::Call, Role::Value, 0, 11);
/// builder.leaf(SyntaxKind::Name, Role::Function, 0, 5);
/// builder.open(SyntaxKind::Arguments, Role::Arguments, 5, 11);
/// builder.leaf(SyntaxKind::StringLiteral, Role::Item, 6, 10);
/// builder.close();
/// builder.close();
/// builder.close();
/// builder.close();
/// let tree = builder.finish();
///
/// // The `Item` role is left out, and the string's quotes are escaped.
/// let printed = tree.display(text).to_string();
/// assert_eq!(
///     printed,
///     "Module 0..11\n  ExpressionStatement @Body 0..11\n    Call @Value 0..11\n      \
///      Name @Function 0..5 \"print\"\n      Arguments @Arguments 5..11\n        \
///      StringLiteral 6..10 \"\\\"hi\\\"\"\n",
/// );
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Display<'a> {
    /// The tree to print.
    tree: &'a SyntaxTree,
    /// The text the tree was parsed from.
    source: &'a str,
}

impl<'a> Display<'a> {
    /// Wraps `tree` for printing, reading leaf text from `source`.
    pub(crate) fn new(tree: &'a SyntaxTree, source: &'a str) -> Self {
        Self { tree, source }
    }

    /// Writes `id` and its subtree at `depth` levels of indentation.
    fn write_node(
        &self,
        formatter: &mut fmt::Formatter<'_>,
        id: NodeId,
        depth: usize,
    ) -> fmt::Result {
        let node = self.tree.node(id);
        write!(
            formatter,
            "{:indent$}{:?}",
            "",
            node.kind(),
            indent = depth * 2
        )?;
        if !matches!(node.role(), Role::Item | Role::Root) {
            write!(formatter, " @{:?}", node.role())?;
        }
        let operator = node.operator().symbol();
        if !operator.is_empty() {
            write!(formatter, " {operator}")?;
        }
        if !node.flags().is_empty() {
            write!(formatter, " {}", node.flags())?;
        }
        write!(formatter, " {}..{}", node.start(), node.end())?;
        let mut children = self.tree.children(id).peekable();
        if children.peek().is_none() {
            write!(formatter, " {:?}", self.tree.text(id, self.source))?;
        }
        writeln!(formatter)?;
        for child in children {
            self.write_node(formatter, child, depth + 1)?;
        }
        Ok(())
    }
}

impl fmt::Display for Display<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_node(formatter, self.tree.root(), 0)
    }
}
