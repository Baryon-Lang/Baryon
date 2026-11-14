//! Version 1 B-trees, grown the way the HDF5 library 2.0 grows them (`H5B.c`): a group's
//! B-tree, whose keys are names and whose leaves point to symbol table nodes, and a chunked
//! dataset's, whose keys are chunk positions and whose leaves point to chunks.
//!
//! A node holds up to `2K` children and one more key than children; the keys bound the
//! children's contents. An insert walks down to a leaf; there the tree's [`Class`] adds the
//! value to an existing child, or a new child goes left or right of one. A full node splits:
//! a node with no right neighbour keeps 90% of its children, one with no left neighbour 10%,
//! and others half. When the root splits, it moves to a new address and a new root takes its
//! place, so the tree's address never changes.

use super::header::to_usize;
use super::space::{Space, UNDEFINED, Use};
use std::collections::BTreeMap;

/// What an insert did to a node, which tells its parent what to do (`H5B_ins_t`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Insert {
    /// Nothing for the parent to do.
    Nothing,
    /// A new child goes left of the one the insert went into.
    Left,
    /// A new child goes right of the one the insert went into.
    Right,
    /// The first child of an empty tree.
    First,
}

/// Whether an insert changed the left or right key bounding the child it went into.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Changed {
    /// The left key changed.
    pub(super) left: bool,
    /// The right key changed.
    pub(super) right: bool,
}

/// What a kind of B-tree stores and how (`H5B_class_t`).
pub(super) trait Class {
    /// A key.
    type Key: Clone;
    /// What an insert adds.
    type Value;
    /// Whether an insert left of the first key or right of the last goes into the first or
    /// last child rather than starting a new one.
    fn follow_edges(&self) -> bool;
    /// Where `value` goes relative to the child between `left` and `right`: negative left
    /// of it, positive right of it, zero in it.
    fn compare(&self, left: &Self::Key, value: &Self::Value, right: &Self::Key) -> i32;
    /// Makes a new child for `value` and returns its address, setting its keys: `left`
    /// always, `right` unless `insert` is [`Insert::Left`].
    fn new_child(
        &mut self,
        space: &mut Space,
        insert: Insert,
        left: &mut Self::Key,
        value: &Self::Value,
        right: &mut Self::Key,
    ) -> u64;
    /// Adds `value` to the child at `address`, bounded by `left` and `right`. Returns what
    /// the parent must do and the new child's address; `middle` is the key between the old
    /// child and a new one.
    #[expect(
        clippy::too_many_arguments,
        reason = "the library's callback, field for field"
    )]
    fn insert(
        &mut self,
        space: &mut Space,
        address: u64,
        left: &mut Self::Key,
        changed: &mut Changed,
        middle: &mut Self::Key,
        value: &Self::Value,
        right: &mut Self::Key,
    ) -> (Insert, u64);
}

/// One node.
#[derive(Clone, Debug)]
struct Node<K> {
    /// Its level: 0 for a leaf.
    level: u8,
    /// Its left sibling's address.
    left: u64,
    /// Its right sibling's address.
    right: u64,
    /// Its keys: `2K + 1` slots, of which one more than the children are used.
    keys: Vec<K>,
    /// Its children's addresses.
    children: Vec<u64>,
}

/// A B-tree: its nodes by address.
#[derive(Clone, Debug)]
pub(super) struct Btree<K> {
    /// The root's address, which never changes.
    pub(super) root: u64,
    /// The size of a node in the file.
    node_size: u64,
    /// The most children a node holds, `2K`.
    two_k: usize,
    /// The key of a new, empty node.
    empty: K,
    /// The nodes.
    nodes: BTreeMap<u64, Node<K>>,
}

impl<K: Clone> Btree<K> {
    /// A new tree of one empty node of `node_size` bytes, which holds up to `two_k`
    /// children and starts with `empty` keys, its space taken from `space` (`H5B_create`).
    pub(super) fn new(space: &mut Space, node_size: u64, two_k: usize, empty: K) -> Self {
        let mut tree = Self {
            root: 0,
            node_size,
            two_k,
            empty,
            nodes: BTreeMap::new(),
        };
        tree.root = tree.create(space, 0);
        tree
    }

    /// Makes an empty node at `level` and returns its address.
    fn create(&mut self, space: &mut Space, level: u8) -> u64 {
        let address = space.allocate(Use::Metadata, self.node_size);
        let node = Node {
            level,
            left: UNDEFINED,
            right: UNDEFINED,
            keys: vec![self.empty.clone(); self.two_k + 1],
            children: Vec::new(),
        };
        self.nodes.insert(address, node);
        address
    }

    /// Inserts `value` (`H5B_insert`).
    pub(super) fn insert<C: Class<Key = K>>(
        &mut self,
        class: &mut C,
        space: &mut Space,
        value: &C::Value,
    ) {
        let mut left = self.empty.clone();
        let mut middle = self.empty.clone();
        let mut right = self.empty.clone();
        let mut changed = Changed::default();
        let root = self.root;
        let (insert, split) = self.insert_into(
            class,
            space,
            root,
            (&mut left, &mut changed),
            &mut middle,
            value,
            &mut right,
        );
        if insert == Insert::Nothing {
            return;
        }
        let Some(split) = split else { return };
        let old = &self.nodes[&root];
        if !changed.left {
            left = old.keys[0].clone();
        }
        let split_node = &self.nodes[&split];
        if !changed.right {
            right = split_node.keys[split_node.children.len()].clone();
        }
        let level = old.level;
        // The old root moves to a new address; a new root takes its place.
        let moved = space.allocate(Use::Metadata, self.node_size);
        let old = self
            .nodes
            .remove(&root)
            .unwrap_or_else(|| unreachable!("the root exists"));
        self.nodes.insert(moved, old);
        if let Some(split_node) = self.nodes.get_mut(&split) {
            split_node.left = moved;
        }
        let mut keys = vec![self.empty.clone(); self.two_k + 1];
        keys[0] = left;
        keys[1] = middle;
        keys[2] = right;
        self.nodes.insert(
            root,
            Node {
                level: level + 1,
                left: UNDEFINED,
                right: UNDEFINED,
                keys,
                children: vec![moved, split],
            },
        );
    }

    /// Inserts `value` below the node at `address`, bounded by `left` and `right`
    /// (`H5B__insert_helper`). Returns what the parent must do and the node split off, if any.
    #[expect(
        clippy::too_many_arguments,
        reason = "the library's helper, field for field"
    )]
    fn insert_into<C: Class<Key = K>>(
        &mut self,
        class: &mut C,
        space: &mut Space,
        address: u64,
        (left, changed): (&mut K, &mut Changed),
        middle: &mut K,
        value: &C::Value,
        right: &mut K,
    ) -> (Insert, Option<u64>) {
        let node = &self.nodes[&address];
        let (level, count) = (node.level, node.children.len());
        let (mut low, mut high) = (0, count);
        let (mut index, mut order) = (0, -1);
        while low < high && order != 0 {
            index = usize::midpoint(low, high);
            order = class.compare(&node.keys[index], value, &node.keys[index + 1]);
            if order < 0 {
                high = index;
            } else {
                low = index + 1;
            }
        }
        let mut insert = Insert::Nothing;
        let mut new_child = 0;
        if count == 0 {
            let node = self.node(address);
            let (first, rest) = node.keys.split_at_mut(1);
            let child = class.new_child(space, Insert::First, &mut first[0], value, &mut rest[0]);
            node.children.push(child);
            index = 0;
            if class.follow_edges() {
                (insert, new_child) =
                    self.insert_leaf(class, space, address, 0, changed, middle, value);
            }
        } else if order < 0 && index == 0 {
            if level > 0 {
                (insert, new_child) =
                    self.insert_below(class, space, address, 0, changed, middle, value);
            } else if class.follow_edges() {
                (insert, new_child) =
                    self.insert_leaf(class, space, address, 0, changed, middle, value);
            } else {
                insert = Insert::Left;
                let node = self.node(address);
                *middle = node.keys[0].clone();
                new_child = class.new_child(space, Insert::Left, &mut node.keys[0], value, middle);
                changed.left = true;
            }
        } else if order > 0 && index + 1 >= count {
            index = count - 1;
            if level > 0 {
                (insert, new_child) =
                    self.insert_below(class, space, address, index, changed, middle, value);
            } else if class.follow_edges() {
                (insert, new_child) =
                    self.insert_leaf(class, space, address, index, changed, middle, value);
            } else {
                insert = Insert::Right;
                let node = self.node(address);
                *middle = node.keys[index + 1].clone();
                new_child = class.new_child(
                    space,
                    Insert::Right,
                    middle,
                    value,
                    &mut node.keys[index + 1],
                );
                changed.right = true;
            }
        } else if level > 0 {
            (insert, new_child) =
                self.insert_below(class, space, address, index, changed, middle, value);
        } else {
            (insert, new_child) =
                self.insert_leaf(class, space, address, index, changed, middle, value);
        }
        self.settle(
            space,
            address,
            (index, insert, new_child),
            (left, changed),
            middle,
            right,
        )
    }

    /// The end of [`Btree::insert_into`]: passes changed bounding keys up to the parent, adds
    /// a new child to the node at `address`, splitting it when full, and says what the parent
    /// must do. `index` is the child the insert went into.
    fn settle(
        &mut self,
        space: &mut Space,
        address: u64,
        (mut index, insert, new_child): (usize, Insert, u64),
        (left, changed): (&mut K, &mut Changed),
        middle: &mut K,
        right: &mut K,
    ) -> (Insert, Option<u64>) {
        let node = self.node(address);
        if changed.left {
            if index > 0 {
                changed.left = false;
            } else {
                *left = node.keys[index].clone();
            }
        }
        if changed.right {
            if index + 1 < node.children.len() {
                changed.right = false;
            } else {
                *right = node.keys[index + 1].clone();
            }
        }
        if !matches!(insert, Insert::Left | Insert::Right) {
            return (Insert::Nothing, None);
        }
        let mut target = address;
        let mut split = None;
        if node.children.len() == self.two_k {
            let off = self.split(space, address, index);
            split = Some(off);
            let kept = self.nodes[&address].children.len();
            if index >= kept {
                index -= kept;
                target = off;
            }
        }
        insert_child(self.node(target), index, new_child, insert, middle.clone());
        match split {
            Some(off) => {
                *middle = self.nodes[&off].keys[0].clone();
                (Insert::Right, Some(off))
            }
            None => (Insert::Nothing, None),
        }
    }

    /// The node at `address`, to change.
    fn node(&mut self, address: u64) -> &mut Node<K> {
        self.nodes
            .get_mut(&address)
            .unwrap_or_else(|| unreachable!("every child address is a node"))
    }

    /// Inserts into the child `index` of the internal node at `address`, which is itself a
    /// node; the child's bounding keys are the node's own, copied and written back.
    #[expect(
        clippy::too_many_arguments,
        reason = "the library's helper, field for field"
    )]
    fn insert_below<C: Class<Key = K>>(
        &mut self,
        class: &mut C,
        space: &mut Space,
        address: u64,
        index: usize,
        changed: &mut Changed,
        middle: &mut K,
        value: &C::Value,
    ) -> (Insert, u64) {
        let node = &self.nodes[&address];
        let child = node.children[index];
        let mut left = node.keys[index].clone();
        let mut right = node.keys[index + 1].clone();
        let (insert, split) = self.insert_into(
            class,
            space,
            child,
            (&mut left, changed),
            middle,
            value,
            &mut right,
        );
        let node = self.node(address);
        node.keys[index] = left;
        node.keys[index + 1] = right;
        (insert, split.unwrap_or(0))
    }

    /// Inserts into the child `index` of the leaf at `address` through the tree's class.
    #[expect(
        clippy::too_many_arguments,
        reason = "the library's helper, field for field"
    )]
    fn insert_leaf<C: Class<Key = K>>(
        &mut self,
        class: &mut C,
        space: &mut Space,
        address: u64,
        index: usize,
        changed: &mut Changed,
        middle: &mut K,
        value: &C::Value,
    ) -> (Insert, u64) {
        let node = self.node(address);
        let child = node.children[index];
        let (low, high) = node.keys.split_at_mut(index + 1);
        class.insert(
            space,
            child,
            &mut low[index],
            changed,
            middle,
            value,
            &mut high[0],
        )
    }

    /// Splits the full node at `address`, where an insert goes at `index`, and returns the new
    /// node's address (`H5B__split`).
    fn split(&mut self, space: &mut Space, address: u64, index: usize) -> u64 {
        let two_k = self.two_k;
        let node = &self.nodes[&address];
        let mut kept = if node.right == UNDEFINED {
            two_k * 9 / 10
        } else if node.left == UNDEFINED {
            two_k / 10
        } else {
            two_k / 2
        };
        if index < kept && kept == two_k {
            kept -= 1;
        } else if index >= kept && kept == 0 {
            kept += 1;
        }
        let moved = two_k - kept;
        let level = node.level;
        let off = self.create(space, level);
        let node = self.node(address);
        let keys: Vec<K> = node.keys[kept..=kept + moved].to_vec();
        let children = node.children.split_off(kept);
        let old_right = node.right;
        node.right = off;
        let new = self.node(off);
        new.keys[..=moved].clone_from_slice(&keys);
        new.children = children;
        new.left = address;
        new.right = old_right;
        if old_right != UNDEFINED {
            self.node(old_right).left = off;
        }
        off
    }

    /// The tree's nodes as `(address, bytes)`, each node marked as of type `kind`, its keys
    /// encoded by `encode_key`.
    pub(super) fn encode(
        &self,
        kind: u8,
        encode_key: impl Fn(&K) -> Vec<u8>,
    ) -> Vec<(u64, Vec<u8>)> {
        self.nodes
            .iter()
            .map(|(&address, node)| {
                let mut bytes = Vec::with_capacity(to_usize(self.node_size));
                bytes.extend_from_slice(b"TREE");
                bytes.push(kind);
                bytes.push(node.level);
                let count = u16::try_from(node.children.len()).unwrap_or(u16::MAX);
                bytes.extend_from_slice(&count.to_le_bytes());
                bytes.extend_from_slice(&node.left.to_le_bytes());
                bytes.extend_from_slice(&node.right.to_le_bytes());
                for (key, child) in node.keys.iter().zip(&node.children) {
                    bytes.extend_from_slice(&encode_key(key));
                    bytes.extend_from_slice(&child.to_le_bytes());
                }
                if !node.children.is_empty() {
                    bytes.extend_from_slice(&encode_key(&node.keys[node.children.len()]));
                }
                bytes.resize(to_usize(self.node_size), 0);
                (address, bytes)
            })
            .collect()
    }
}

/// Adds `child` to `node` next to the child at `index`, on the side `insert` says, with
/// `middle` as the key between them (`H5B__insert_child`).
fn insert_child<K: Clone>(node: &mut Node<K>, index: usize, child: u64, insert: Insert, middle: K) {
    let count = node.children.len();
    if index + 1 == count {
        node.keys[index + 2] = node.keys[index + 1].clone();
        node.keys[index + 1] = middle;
        if insert == Insert::Right {
            node.children.insert(index + 1, child);
        } else {
            let old = node.children[index];
            node.children.insert(index + 1, old);
            node.children[index] = child;
        }
    } else {
        for at in (index + 1..=count).rev() {
            node.keys[at + 1] = node.keys[at].clone();
        }
        node.keys[index + 1] = middle;
        let at = if insert == Insert::Right {
            index + 1
        } else {
            index
        };
        node.children.insert(at, child);
    }
}
