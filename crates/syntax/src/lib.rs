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
mod tree;

pub use builder::TreeBuilder;
pub use display::Display;
pub use flags::Flags;
pub use kind::SyntaxKind;
pub use operator::Operator;
pub use role::Role;
pub use tree::{Children, SyntaxTree};

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

#[cfg(test)]
mod tests;
