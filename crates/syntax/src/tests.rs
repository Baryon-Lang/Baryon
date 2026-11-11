//! Unit tests of the tree's queries on small hand-built trees.

use crate::*;
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
