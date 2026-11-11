//! The tree itself: its nodes in preorder and the queries every later stage reads it with.

use crate::{Display, Node, NodeId, Role, SyntaxKind};
use source::{FileId, Span};

/// The syntax tree of one source file. Built with a [`TreeBuilder`](crate::TreeBuilder); see
/// the crate documentation for an example.
///
/// # Panics
///
/// Every method that takes a [`NodeId`] panics if the id came from a different, larger tree.
/// Ids from this tree are always valid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxTree {
    /// The file the tree was parsed from.
    pub(crate) file: FileId,
    /// Every node in preorder. Never empty: index 0 is the root.
    pub(crate) nodes: Vec<Node>,
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
