//! The syntax tree: what a parser produces and every later stage reads.
//!
//! This is an interface crate. Any parser that builds a [`SyntaxTree`] can feed the rest of the
//! compiler, which never learns which parser it was (decision 0001). Today the `parser` crate
//! builds it from an existing Python parser (decision 0004).
//!
//! # Shape
//!
//! A tree is one flat array of 16-byte [`Node`]s in preorder: each node is followed by all of
//! its descendants, then by its next sibling. A node records its [`SyntaxKind`], its [`Role`] in
//! its parent, an [`Operator`], a few [`Flags`], its byte range in the source, and how many
//! descendants it has. Finding a node's first child is one step forward and finding its next
//! sibling is one jump, so a whole tree is walked in one pass over contiguous memory with no
//! pointers and no per-node allocation. That layout is the design plan's answer to its
//! parse-speed target of a million lines a second.
//!
//! Leaves such as names and literals hold no text of their own. Their text is their span of the
//! source, read with [`SyntaxTree::text`], so the tree never copies the source.
//!
//! # Example
//!
//! Trees are normally built by a parser. Built by hand, `x = 1` looks like this:
//!
//! ```
//! use source::SourceMap;
//! use syntax::{Role, SyntaxKind, TreeBuilder};
//!
//! let mut sources = SourceMap::new();
//! let text = "x = 1";
//! let file = sources.add("main.byn", text)?;
//!
//! let mut builder = TreeBuilder::new(file);
//! builder.open(SyntaxKind::Module, Role::Root, 0, 5);
//! builder.open(SyntaxKind::Assign, Role::Body, 0, 5);
//! builder.leaf(SyntaxKind::Name, Role::Target, 0, 1);
//! builder.leaf(SyntaxKind::Integer, Role::Value, 4, 5);
//! builder.close();
//! builder.close();
//! let tree = builder.finish();
//!
//! let assign = tree.children(tree.root()).next().unwrap();
//! let value = tree.child(assign, Role::Value).unwrap();
//! assert_eq!(tree.kind(value), SyntaxKind::Integer);
//! assert_eq!(tree.text(value, text), "1");
//! assert_eq!(
//!     tree.display(text).to_string(),
//!     "Module 0..5\n  Assign @Body 0..5\n    Name @Target 0..1 \"x\"\n    Integer @Value 4..5 \"1\"\n",
//! );
//! # Ok::<(), source::SourceError>(())
//! ```

mod builder;
mod display;
mod flags;
mod kind;
mod operator;
mod role;

pub use builder::TreeBuilder;
pub use display::Display;
pub use flags::Flags;
pub use kind::SyntaxKind;
pub use operator::Operator;
pub use role::Role;

use source::{FileId, Span};

/// Identifies one node in one [`SyntaxTree`]: its index in the tree's preorder node array.
///
/// An id is only meaningful for the tree that produced it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(
    /// Index of the node in [`SyntaxTree::nodes`].
    u32,
);

impl NodeId {
    /// The node's position in preorder: 0 for the root, counting up through the file.
    #[must_use]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// One node of a syntax tree: 16 bytes, stored inline in the tree's node array.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node {
    /// What the node is.
    kind: SyntaxKind,
    /// What part it plays in its parent.
    role: Role,
    /// The operator it applies, for operator kinds.
    operator: Operator,
    /// Facts its kind does not say.
    flags: Flags,
    /// Byte offset of its first byte in the source.
    start: u32,
    /// Byte offset one past its last byte.
    end: u32,
    /// How many nodes follow it in preorder before its next sibling: its descendant count.
    descendants: u32,
}

impl Node {
    /// What the node is.
    #[must_use]
    pub fn kind(&self) -> SyntaxKind {
        self.kind
    }

    /// What part it plays in its parent.
    #[must_use]
    pub fn role(&self) -> Role {
        self.role
    }

    /// The operator it applies; [`Operator::None`] unless its kind is an operator kind.
    ///
    /// ```
    /// # use source::FileId;
    /// # use syntax::{Operator, Role, SyntaxKind, TreeBuilder};
    /// let text = "y = x + 1";
    /// # let mut builder = TreeBuilder::new(FileId::from_index(0));
    /// # builder.open(SyntaxKind::Module, Role::Root, 0, 9);
    /// # builder.open(SyntaxKind::Assign, Role::Body, 0, 9);
    /// # builder.leaf(SyntaxKind::Name, Role::Target, 0, 1);
    /// # builder.open(SyntaxKind::BinaryOperation, Role::Value, 4, 9);
    /// # builder.set_operator(Operator::Add);
    /// # builder.leaf(SyntaxKind::Name, Role::Left, 4, 5);
    /// # builder.leaf(SyntaxKind::Integer, Role::Right, 8, 9);
    /// # builder.close();
    /// # builder.close();
    /// # builder.close();
    /// # let tree = builder.finish();
    /// # let assign = tree.children(tree.root()).next().unwrap();
    /// # let sum = tree.child(assign, Role::Value).unwrap();
    ///
    /// assert_eq!(tree.node(sum).operator(), Operator::Add);
    /// assert_eq!(tree.node(assign).operator(), Operator::None);
    /// ```
    #[must_use]
    pub fn operator(&self) -> Operator {
        self.operator
    }

    /// Facts its kind does not say, such as `async`.
    #[must_use]
    pub fn flags(&self) -> Flags {
        self.flags
    }

    /// Byte offset of its first byte in the source.
    #[must_use]
    pub fn start(&self) -> u32 {
        self.start
    }

    /// Byte offset one past its last byte.
    #[must_use]
    pub fn end(&self) -> u32 {
        self.end
    }
}

/// The syntax tree of one source file. Built with a [`TreeBuilder`]; see the crate
/// documentation for an example.
///
/// # Panics
///
/// Every method that takes a [`NodeId`] panics if the id came from a different, larger tree.
/// Ids from this tree are always valid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxTree {
    /// The file the tree was parsed from.
    file: FileId,
    /// Every node in preorder. Never empty: index 0 is the root.
    nodes: Vec<Node>,
}

impl SyntaxTree {
    /// The file the tree was parsed from.
    ///
    /// ```
    /// # use source::FileId;
    /// # use syntax::{Operator, Role, SyntaxKind, TreeBuilder};
    /// let text = "y = x + 1";
    /// # let mut builder = TreeBuilder::new(FileId::from_index(0));
    /// # builder.open(SyntaxKind::Module, Role::Root, 0, 9);
    /// # builder.open(SyntaxKind::Assign, Role::Body, 0, 9);
    /// # builder.leaf(SyntaxKind::Name, Role::Target, 0, 1);
    /// # builder.open(SyntaxKind::BinaryOperation, Role::Value, 4, 9);
    /// # builder.set_operator(Operator::Add);
    /// # builder.leaf(SyntaxKind::Name, Role::Left, 4, 5);
    /// # builder.leaf(SyntaxKind::Integer, Role::Right, 8, 9);
    /// # builder.close();
    /// # builder.close();
    /// # builder.close();
    /// # let tree = builder.finish();
    /// # let assign = tree.children(tree.root()).next().unwrap();
    /// # let sum = tree.child(assign, Role::Value).unwrap();
    ///
    /// assert_eq!(tree.file(), FileId::from_index(0));
    /// ```
    #[must_use]
    pub fn file(&self) -> FileId {
        self.file
    }

    /// The root node, a [`SyntaxKind::Module`] for a parsed file.
    ///
    /// ```
    /// # use source::FileId;
    /// # use syntax::{Operator, Role, SyntaxKind, TreeBuilder};
    /// let text = "y = x + 1";
    /// # let mut builder = TreeBuilder::new(FileId::from_index(0));
    /// # builder.open(SyntaxKind::Module, Role::Root, 0, 9);
    /// # builder.open(SyntaxKind::Assign, Role::Body, 0, 9);
    /// # builder.leaf(SyntaxKind::Name, Role::Target, 0, 1);
    /// # builder.open(SyntaxKind::BinaryOperation, Role::Value, 4, 9);
    /// # builder.set_operator(Operator::Add);
    /// # builder.leaf(SyntaxKind::Name, Role::Left, 4, 5);
    /// # builder.leaf(SyntaxKind::Integer, Role::Right, 8, 9);
    /// # builder.close();
    /// # builder.close();
    /// # builder.close();
    /// # let tree = builder.finish();
    /// # let assign = tree.children(tree.root()).next().unwrap();
    /// # let sum = tree.child(assign, Role::Value).unwrap();
    ///
    /// assert_eq!(tree.kind(tree.root()), SyntaxKind::Module);
    /// assert_eq!(tree.root().index(), 0);
    /// ```
    #[must_use]
    pub fn root(&self) -> NodeId {
        NodeId(0)
    }

    /// How many nodes the tree holds.
    ///
    /// ```
    /// # use source::FileId;
    /// # use syntax::{Operator, Role, SyntaxKind, TreeBuilder};
    /// let text = "y = x + 1";
    /// # let mut builder = TreeBuilder::new(FileId::from_index(0));
    /// # builder.open(SyntaxKind::Module, Role::Root, 0, 9);
    /// # builder.open(SyntaxKind::Assign, Role::Body, 0, 9);
    /// # builder.leaf(SyntaxKind::Name, Role::Target, 0, 1);
    /// # builder.open(SyntaxKind::BinaryOperation, Role::Value, 4, 9);
    /// # builder.set_operator(Operator::Add);
    /// # builder.leaf(SyntaxKind::Name, Role::Left, 4, 5);
    /// # builder.leaf(SyntaxKind::Integer, Role::Right, 8, 9);
    /// # builder.close();
    /// # builder.close();
    /// # builder.close();
    /// # let tree = builder.finish();
    /// # let assign = tree.children(tree.root()).next().unwrap();
    /// # let sum = tree.child(assign, Role::Value).unwrap();
    ///
    /// // Module, Assign, Name, BinaryOperation, Name, Integer.
    /// assert_eq!(tree.len(), 6);
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Always `false`: a tree holds at least its root.
    ///
    /// ```
    /// # use source::FileId;
    /// # use syntax::{Operator, Role, SyntaxKind, TreeBuilder};
    /// let text = "y = x + 1";
    /// # let mut builder = TreeBuilder::new(FileId::from_index(0));
    /// # builder.open(SyntaxKind::Module, Role::Root, 0, 9);
    /// # builder.open(SyntaxKind::Assign, Role::Body, 0, 9);
    /// # builder.leaf(SyntaxKind::Name, Role::Target, 0, 1);
    /// # builder.open(SyntaxKind::BinaryOperation, Role::Value, 4, 9);
    /// # builder.set_operator(Operator::Add);
    /// # builder.leaf(SyntaxKind::Name, Role::Left, 4, 5);
    /// # builder.leaf(SyntaxKind::Integer, Role::Right, 8, 9);
    /// # builder.close();
    /// # builder.close();
    /// # builder.close();
    /// # let tree = builder.finish();
    /// # let assign = tree.children(tree.root()).next().unwrap();
    /// # let sum = tree.child(assign, Role::Value).unwrap();
    ///
    /// assert!(!tree.is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Every node in preorder, for passes that visit the whole tree.
    ///
    /// ```
    /// # use source::FileId;
    /// # use syntax::{Operator, Role, SyntaxKind, TreeBuilder};
    /// let text = "y = x + 1";
    /// # let mut builder = TreeBuilder::new(FileId::from_index(0));
    /// # builder.open(SyntaxKind::Module, Role::Root, 0, 9);
    /// # builder.open(SyntaxKind::Assign, Role::Body, 0, 9);
    /// # builder.leaf(SyntaxKind::Name, Role::Target, 0, 1);
    /// # builder.open(SyntaxKind::BinaryOperation, Role::Value, 4, 9);
    /// # builder.set_operator(Operator::Add);
    /// # builder.leaf(SyntaxKind::Name, Role::Left, 4, 5);
    /// # builder.leaf(SyntaxKind::Integer, Role::Right, 8, 9);
    /// # builder.close();
    /// # builder.close();
    /// # builder.close();
    /// # let tree = builder.finish();
    /// # let assign = tree.children(tree.root()).next().unwrap();
    /// # let sum = tree.child(assign, Role::Value).unwrap();
    ///
    /// // Count the names in the file in one pass, with no recursion.
    /// let names = tree.nodes().iter().filter(|node| node.kind() == SyntaxKind::Name).count();
    /// assert_eq!(names, 2);
    /// ```
    #[must_use]
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// The node with this id.
    ///
    /// ```
    /// # use source::FileId;
    /// # use syntax::{Operator, Role, SyntaxKind, TreeBuilder};
    /// let text = "y = x + 1";
    /// # let mut builder = TreeBuilder::new(FileId::from_index(0));
    /// # builder.open(SyntaxKind::Module, Role::Root, 0, 9);
    /// # builder.open(SyntaxKind::Assign, Role::Body, 0, 9);
    /// # builder.leaf(SyntaxKind::Name, Role::Target, 0, 1);
    /// # builder.open(SyntaxKind::BinaryOperation, Role::Value, 4, 9);
    /// # builder.set_operator(Operator::Add);
    /// # builder.leaf(SyntaxKind::Name, Role::Left, 4, 5);
    /// # builder.leaf(SyntaxKind::Integer, Role::Right, 8, 9);
    /// # builder.close();
    /// # builder.close();
    /// # builder.close();
    /// # let tree = builder.finish();
    /// # let assign = tree.children(tree.root()).next().unwrap();
    /// # let sum = tree.child(assign, Role::Value).unwrap();
    ///
    /// let node = tree.node(sum);
    /// assert_eq!((node.kind(), node.role()), (SyntaxKind::BinaryOperation, Role::Value));
    /// assert_eq!((node.start(), node.end()), (4, 9));
    /// ```
    #[must_use]
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    /// The kind of the node with this id.
    ///
    /// ```
    /// # use source::FileId;
    /// # use syntax::{Operator, Role, SyntaxKind, TreeBuilder};
    /// let text = "y = x + 1";
    /// # let mut builder = TreeBuilder::new(FileId::from_index(0));
    /// # builder.open(SyntaxKind::Module, Role::Root, 0, 9);
    /// # builder.open(SyntaxKind::Assign, Role::Body, 0, 9);
    /// # builder.leaf(SyntaxKind::Name, Role::Target, 0, 1);
    /// # builder.open(SyntaxKind::BinaryOperation, Role::Value, 4, 9);
    /// # builder.set_operator(Operator::Add);
    /// # builder.leaf(SyntaxKind::Name, Role::Left, 4, 5);
    /// # builder.leaf(SyntaxKind::Integer, Role::Right, 8, 9);
    /// # builder.close();
    /// # builder.close();
    /// # builder.close();
    /// # let tree = builder.finish();
    /// # let assign = tree.children(tree.root()).next().unwrap();
    /// # let sum = tree.child(assign, Role::Value).unwrap();
    ///
    /// assert_eq!(tree.kind(sum), SyntaxKind::BinaryOperation);
    /// ```
    #[must_use]
    pub fn kind(&self, id: NodeId) -> SyntaxKind {
        self.node(id).kind
    }

    /// The source range of the node with this id.
    ///
    /// ```
    /// # use source::FileId;
    /// # use syntax::{Operator, Role, SyntaxKind, TreeBuilder};
    /// let text = "y = x + 1";
    /// # let mut builder = TreeBuilder::new(FileId::from_index(0));
    /// # builder.open(SyntaxKind::Module, Role::Root, 0, 9);
    /// # builder.open(SyntaxKind::Assign, Role::Body, 0, 9);
    /// # builder.leaf(SyntaxKind::Name, Role::Target, 0, 1);
    /// # builder.open(SyntaxKind::BinaryOperation, Role::Value, 4, 9);
    /// # builder.set_operator(Operator::Add);
    /// # builder.leaf(SyntaxKind::Name, Role::Left, 4, 5);
    /// # builder.leaf(SyntaxKind::Integer, Role::Right, 8, 9);
    /// # builder.close();
    /// # builder.close();
    /// # builder.close();
    /// # let tree = builder.finish();
    /// # let assign = tree.children(tree.root()).next().unwrap();
    /// # let sum = tree.child(assign, Role::Value).unwrap();
    ///
    /// let span = tree.span(sum);
    /// assert_eq!((span.file(), span.start(), span.end()), (FileId::from_index(0), 4, 9));
    /// ```
    #[must_use]
    pub fn span(&self, id: NodeId) -> Span {
        let node = self.node(id);
        Span::new(self.file, node.start, node.end)
    }

    /// The source text of the node with this id. `source` must be the text the tree was
    /// parsed from; for any other text the result is meaningless, and empty if out of range.
    ///
    /// ```
    /// # use source::FileId;
    /// # use syntax::{Operator, Role, SyntaxKind, TreeBuilder};
    /// let text = "y = x + 1";
    /// # let mut builder = TreeBuilder::new(FileId::from_index(0));
    /// # builder.open(SyntaxKind::Module, Role::Root, 0, 9);
    /// # builder.open(SyntaxKind::Assign, Role::Body, 0, 9);
    /// # builder.leaf(SyntaxKind::Name, Role::Target, 0, 1);
    /// # builder.open(SyntaxKind::BinaryOperation, Role::Value, 4, 9);
    /// # builder.set_operator(Operator::Add);
    /// # builder.leaf(SyntaxKind::Name, Role::Left, 4, 5);
    /// # builder.leaf(SyntaxKind::Integer, Role::Right, 8, 9);
    /// # builder.close();
    /// # builder.close();
    /// # builder.close();
    /// # let tree = builder.finish();
    /// # let assign = tree.children(tree.root()).next().unwrap();
    /// # let sum = tree.child(assign, Role::Value).unwrap();
    ///
    /// assert_eq!(tree.text(sum, text), "x + 1");
    /// ```
    #[must_use]
    pub fn text<'a>(&self, id: NodeId, source: &'a str) -> &'a str {
        let node = self.node(id);
        source
            .get(node.start as usize..node.end as usize)
            .unwrap_or_default()
    }

    /// The node's children, in source order.
    ///
    /// ```
    /// # use source::FileId;
    /// # use syntax::{Operator, Role, SyntaxKind, TreeBuilder};
    /// let text = "y = x + 1";
    /// # let mut builder = TreeBuilder::new(FileId::from_index(0));
    /// # builder.open(SyntaxKind::Module, Role::Root, 0, 9);
    /// # builder.open(SyntaxKind::Assign, Role::Body, 0, 9);
    /// # builder.leaf(SyntaxKind::Name, Role::Target, 0, 1);
    /// # builder.open(SyntaxKind::BinaryOperation, Role::Value, 4, 9);
    /// # builder.set_operator(Operator::Add);
    /// # builder.leaf(SyntaxKind::Name, Role::Left, 4, 5);
    /// # builder.leaf(SyntaxKind::Integer, Role::Right, 8, 9);
    /// # builder.close();
    /// # builder.close();
    /// # builder.close();
    /// # let tree = builder.finish();
    /// # let assign = tree.children(tree.root()).next().unwrap();
    /// # let sum = tree.child(assign, Role::Value).unwrap();
    ///
    /// let kinds: Vec<SyntaxKind> = tree.children(sum).map(|child| tree.kind(child)).collect();
    /// assert_eq!(kinds, [SyntaxKind::Name, SyntaxKind::Integer]);
    /// ```
    #[must_use]
    pub fn children(&self, id: NodeId) -> Children<'_> {
        let node = self.node(id);
        Children {
            tree: self,
            next: id.0 + 1,
            end: id.0 + 1 + node.descendants,
        }
    }

    /// The node's first child with this role, if any.
    ///
    /// ```
    /// # use source::FileId;
    /// # use syntax::{Operator, Role, SyntaxKind, TreeBuilder};
    /// let text = "y = x + 1";
    /// # let mut builder = TreeBuilder::new(FileId::from_index(0));
    /// # builder.open(SyntaxKind::Module, Role::Root, 0, 9);
    /// # builder.open(SyntaxKind::Assign, Role::Body, 0, 9);
    /// # builder.leaf(SyntaxKind::Name, Role::Target, 0, 1);
    /// # builder.open(SyntaxKind::BinaryOperation, Role::Value, 4, 9);
    /// # builder.set_operator(Operator::Add);
    /// # builder.leaf(SyntaxKind::Name, Role::Left, 4, 5);
    /// # builder.leaf(SyntaxKind::Integer, Role::Right, 8, 9);
    /// # builder.close();
    /// # builder.close();
    /// # builder.close();
    /// # let tree = builder.finish();
    /// # let assign = tree.children(tree.root()).next().unwrap();
    /// # let sum = tree.child(assign, Role::Value).unwrap();
    ///
    /// let right = tree.child(sum, Role::Right).unwrap();
    /// assert_eq!(tree.text(right, text), "1");
    /// assert_eq!(tree.child(sum, Role::Body), None);
    /// ```
    #[must_use]
    pub fn child(&self, id: NodeId, role: Role) -> Option<NodeId> {
        self.children_with(id, role).next()
    }

    /// The node's children with this role, in source order.
    ///
    /// ```
    /// # use source::FileId;
    /// # use syntax::{Operator, Role, SyntaxKind, TreeBuilder};
    /// let text = "y = x + 1";
    /// # let mut builder = TreeBuilder::new(FileId::from_index(0));
    /// # builder.open(SyntaxKind::Module, Role::Root, 0, 9);
    /// # builder.open(SyntaxKind::Assign, Role::Body, 0, 9);
    /// # builder.leaf(SyntaxKind::Name, Role::Target, 0, 1);
    /// # builder.open(SyntaxKind::BinaryOperation, Role::Value, 4, 9);
    /// # builder.set_operator(Operator::Add);
    /// # builder.leaf(SyntaxKind::Name, Role::Left, 4, 5);
    /// # builder.leaf(SyntaxKind::Integer, Role::Right, 8, 9);
    /// # builder.close();
    /// # builder.close();
    /// # builder.close();
    /// # let tree = builder.finish();
    /// # let assign = tree.children(tree.root()).next().unwrap();
    /// # let sum = tree.child(assign, Role::Value).unwrap();
    ///
    /// // An assignment can have several targets, `a = b = 0`, each a child with `Role::Target`.
    /// let targets: Vec<&str> = tree.children_with(assign, Role::Target).map(|id| tree.text(id, text)).collect();
    /// assert_eq!(targets, ["y"]);
    /// ```
    pub fn children_with(&self, id: NodeId, role: Role) -> impl Iterator<Item = NodeId> + '_ {
        self.children(id)
            .filter(move |&child| self.node(child).role == role)
    }

    /// A value that prints the tree as indented text, one node per line, reading leaf text
    /// from `source`. Tests compare trees in this form.
    ///
    /// ```
    /// # use source::FileId;
    /// # use syntax::{Operator, Role, SyntaxKind, TreeBuilder};
    /// let text = "y = x + 1";
    /// # let mut builder = TreeBuilder::new(FileId::from_index(0));
    /// # builder.open(SyntaxKind::Module, Role::Root, 0, 9);
    /// # builder.open(SyntaxKind::Assign, Role::Body, 0, 9);
    /// # builder.leaf(SyntaxKind::Name, Role::Target, 0, 1);
    /// # builder.open(SyntaxKind::BinaryOperation, Role::Value, 4, 9);
    /// # builder.set_operator(Operator::Add);
    /// # builder.leaf(SyntaxKind::Name, Role::Left, 4, 5);
    /// # builder.leaf(SyntaxKind::Integer, Role::Right, 8, 9);
    /// # builder.close();
    /// # builder.close();
    /// # builder.close();
    /// # let tree = builder.finish();
    /// # let assign = tree.children(tree.root()).next().unwrap();
    /// # let sum = tree.child(assign, Role::Value).unwrap();
    ///
    /// assert_eq!(
    ///     tree.display(text).to_string(),
    ///     "Module 0..9\n  Assign @Body 0..9\n    Name @Target 0..1 \"y\"\n    BinaryOperation @Value + 4..9\n      Name @Left 4..5 \"x\"\n      Integer @Right 8..9 \"1\"\n",
    /// );
    /// ```
    #[must_use]
    pub fn display<'a>(&'a self, source: &'a str) -> Display<'a> {
        Display::new(self, source)
    }
}

/// The children of one node, in source order. Created by [`SyntaxTree::children`].
#[derive(Clone, Debug)]
pub struct Children<'a> {
    /// The tree being walked.
    tree: &'a SyntaxTree,
    /// Index of the next child to yield.
    next: u32,
    /// Index one past the parent's last descendant.
    end: u32,
}

impl Iterator for Children<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        if self.next >= self.end {
            return None;
        }
        let child = NodeId(self.next);
        self.next += 1 + self.tree.node(child).descendants;
        Some(child)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use source::SourceMap;

    /// `a + b` as a module holding one expression statement.
    fn addition() -> SyntaxTree {
        let mut sources = SourceMap::new();
        let file = sources.add("test.byn", "a + b").unwrap();
        let mut builder = TreeBuilder::new(file);
        builder.open(SyntaxKind::Module, Role::Root, 0, 5);
        builder.open(SyntaxKind::ExpressionStatement, Role::Body, 0, 5);
        builder.open(SyntaxKind::BinaryOperation, Role::Value, 0, 5);
        builder.set_operator(Operator::Add);
        builder.leaf(SyntaxKind::Name, Role::Left, 0, 1);
        builder.leaf(SyntaxKind::Name, Role::Right, 4, 5);
        builder.close();
        builder.close();
        builder.close();
        builder.finish()
    }

    #[test]
    fn a_node_is_sixteen_bytes() {
        assert_eq!(size_of::<Node>(), 16);
    }

    #[test]
    fn children_skip_over_grandchildren() {
        let tree = addition();
        let statements: Vec<_> = tree.children(tree.root()).collect();
        assert_eq!(statements.len(), 1);
        let operation = tree.child(statements[0], Role::Value).unwrap();
        let operands: Vec<_> = tree
            .children(operation)
            .map(|child| tree.text(child, "a + b"))
            .collect();
        assert_eq!(operands, ["a", "b"]);
        assert_eq!(tree.node(operation).operator(), Operator::Add);
    }

    #[test]
    fn child_finds_by_role() {
        let tree = addition();
        let operation = NodeId(2);
        assert_eq!(tree.child(operation, Role::Right), Some(NodeId(4)));
        assert_eq!(tree.child(operation, Role::Condition), None);
        assert_eq!(tree.children(NodeId(4)).count(), 0);
    }

    #[test]
    fn display_prints_operators_and_leaf_text() {
        assert_eq!(
            addition().display("a + b").to_string(),
            "Module 0..5\n  ExpressionStatement @Body 0..5\n    BinaryOperation @Value + 0..5\n      \
             Name @Left 0..1 \"a\"\n      Name @Right 4..5 \"b\"\n",
        );
    }
}
