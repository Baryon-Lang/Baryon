//! Building a tree in one pass, the way a parser or a lowering produces it.

use crate::{Flags, Node, Operator, Role, SyntaxKind, SyntaxTree};
use source::FileId;

/// Builds a [`SyntaxTree`] in preorder: open a node, add its children, close it.
///
/// Misusing the builder, by closing more nodes than were opened or finishing with nodes still
/// open, is a bug in the parser, so those methods panic instead of returning errors. See the
/// crate documentation for an example.
#[derive(Debug)]
pub struct TreeBuilder {
    /// The file the tree is for.
    file: FileId,
    /// Nodes added so far, in preorder.
    nodes: Vec<Node>,
    /// Indices into `nodes` of the nodes opened and not yet closed, innermost last.
    open: Vec<usize>,
}

impl TreeBuilder {
    /// Starts an empty tree for `file`.
    #[must_use]
    pub fn new(file: FileId) -> Self {
        Self {
            file,
            nodes: Vec::new(),
            open: Vec::new(),
        }
    }

    /// Opens a node spanning `start..end`. Nodes added until the matching [`close`](Self::close)
    /// become its children.
    pub fn open(&mut self, kind: SyntaxKind, role: Role, start: u32, end: u32) {
        self.open.push(self.nodes.len());
        self.push(kind, role, start, end);
    }

    /// Adds a node with no children.
    pub fn leaf(&mut self, kind: SyntaxKind, role: Role, start: u32, end: u32) {
        self.push(kind, role, start, end);
    }

    /// Sets the operator of the innermost open node.
    ///
    /// # Panics
    ///
    /// Panics if no node is open.
    pub fn set_operator(&mut self, operator: Operator) {
        self.innermost().operator = operator;
    }

    /// Adds `flags` to the innermost open node.
    ///
    /// # Panics
    ///
    /// Panics if no node is open.
    pub fn set_flags(&mut self, flags: Flags) {
        let node = self.innermost();
        node.flags = node.flags | flags;
    }

    /// Closes the innermost open node, recording how many descendants it has.
    ///
    /// # Panics
    ///
    /// Panics if no node is open.
    pub fn close(&mut self) {
        let index = self.open.pop();
        assert!(index.is_some(), "closed a node that was never opened");
        let index = index.unwrap_or_default();
        let descendants = self.nodes.len() - index - 1;
        self.nodes[index].descendants = to_u32(descendants);
    }

    /// Returns the finished tree.
    ///
    /// # Panics
    ///
    /// Panics if a node is still open, or if the tree does not have exactly one root.
    #[must_use]
    pub fn finish(self) -> SyntaxTree {
        assert!(self.open.is_empty(), "finished a tree with open nodes");
        assert!(
            self.nodes
                .first()
                .is_some_and(|root| root.descendants as usize + 1 == self.nodes.len()),
            "a tree must have exactly one root"
        );
        SyntaxTree {
            file: self.file,
            nodes: self.nodes,
        }
    }

    /// Appends a node with no operator, flags or descendants yet.
    fn push(&mut self, kind: SyntaxKind, role: Role, start: u32, end: u32) {
        assert!(start <= end, "node start {start} is after its end {end}");
        self.nodes.push(Node {
            kind,
            role,
            operator: Operator::None,
            flags: Flags::NONE,
            start,
            end,
            descendants: 0,
        });
    }

    /// The innermost open node.
    fn innermost(&mut self) -> &mut Node {
        let index = self.open.last().copied();
        assert!(index.is_some(), "no node is open");
        &mut self.nodes[index.unwrap_or_default()]
    }
}

/// Converts a node count to the `u32` stored in a node. Source files are limited to 4 GiB and a
/// node covers at least one byte or sits between two that do, so counts always fit.
fn to_u32(count: usize) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use source::SourceMap;

    fn file() -> FileId {
        SourceMap::new().add("test.byn", "").unwrap()
    }

    #[test]
    #[should_panic(expected = "finished a tree with open nodes")]
    fn finish_rejects_open_nodes() {
        let mut builder = TreeBuilder::new(file());
        builder.open(SyntaxKind::Module, Role::Root, 0, 0);
        let _ = builder.finish();
    }

    #[test]
    #[should_panic(expected = "a tree must have exactly one root")]
    fn finish_rejects_two_roots() {
        let mut builder = TreeBuilder::new(file());
        builder.leaf(SyntaxKind::Module, Role::Root, 0, 0);
        builder.leaf(SyntaxKind::Module, Role::Root, 0, 0);
        let _ = builder.finish();
    }

    #[test]
    #[should_panic(expected = "closed a node that was never opened")]
    fn close_rejects_unopened_nodes() {
        TreeBuilder::new(file()).close();
    }

    #[test]
    fn flags_accumulate() {
        let mut builder = TreeBuilder::new(file());
        builder.open(SyntaxKind::Module, Role::Root, 0, 0);
        builder.set_flags(Flags::ASYNC);
        builder.set_flags(Flags::STAR);
        builder.close();
        let tree = builder.finish();
        assert!(
            tree.node(tree.root())
                .flags()
                .contains(Flags::ASYNC | Flags::STAR)
        );
    }
}
