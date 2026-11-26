//! A struct's `__init__` (decision 0068, roadmap 1.7.17), lowered as a function that returns
//! the struct it builds.
//!
//! Callers see `__init__` without `self`: a call of the class is a call of this function, with
//! `__init__`'s other arguments. Inside it, each field is a local of its own, which the checker
//! listed among the body's variables, keyed by the node that declares the field. `self.field`
//! names that local, so `self.c[i] = v` and `self.n += 1` work as they do on a variable, and
//! setting a field twice drops its old value as a variable's is dropped. Where the body ends
//! or returns, the struct is built from the locals in one aggregate, so it is never half built
//! in the IR and drop elaboration and the backends see nothing new.

use crate::builder::Builder;
use database::Definition;
use mir::{Local, Place, Rvalue};
use syntax::{NodeId, Role, SyntaxKind, SyntaxTree};
use types::{Signature, TypeDb, TypeKind};

/// The struct an `__init__` builds, as lowering its body needs it.
pub(crate) struct Initializer {
    /// The `Name` node of `self`, the binding every use of `self` resolves to.
    pub(crate) receiver: NodeId,
    /// The local holding each field, by the name and in the order the class declares them.
    pub(crate) fields: Vec<(String, Local)>,
}

/// Whether the method `function` of the class statement `class`, with `signature`, is an
/// `__init__`: the checker gives one a signature that returns the struct and leaves out `self`.
pub(crate) fn is_initializer(
    db: &dyn TypeDb,
    tree: &SyntaxTree,
    text: &str,
    (function, class): (NodeId, Option<NodeId>),
    signature: &Signature<'_>,
) -> bool {
    class.is_some()
        && tree
            .child(function, Role::Name)
            .is_some_and(|name| tree.text(name, text) == "__init__")
        && matches!(signature.returns.kind(db), TypeKind::Struct(_))
}

/// The `Name` node of the first parameter of `function`, `self` in a method.
pub(crate) fn receiver(tree: &SyntaxTree, function: NodeId) -> Option<NodeId> {
    tree.child(function, Role::Parameters)
        .into_iter()
        .flat_map(|parameters| tree.children(parameters))
        .find(|&parameter| tree.kind(parameter) == SyntaxKind::Parameter)
        .and_then(|parameter| tree.child(parameter, Role::Name))
}

impl Builder<'_> {
    /// Finds the local of each field of the struct `__init__` builds, now that every variable
    /// has one, for the class statement `class` and the `self` named `receiver`.
    pub(crate) fn start_initializer(&mut self, class: NodeId, receiver: NodeId) {
        let fields = self
            .tree
            .children_with(class, Role::Body)
            .filter(|&statement| self.tree.kind(statement) == SyntaxKind::AnnotatedAssign)
            .filter_map(|statement| self.tree.child(statement, Role::Target))
            .filter_map(|target| {
                let local = self.variables.get(&target).copied()?;
                Some((self.text_of(target).to_owned(), local))
            })
            .collect();
        self.initializer = Some(Initializer { receiver, fields });
    }

    /// The local of the field `node` names inside `__init__`, `self.offset`; `None` for
    /// anything else.
    pub(crate) fn initializer_field(&self, node: NodeId) -> Option<Local> {
        let initializer = self.initializer.as_ref()?;
        if self.tree.kind(node) != SyntaxKind::Attribute {
            return None;
        }
        let value = self.tree.child(node, Role::Value)?;
        if self.definition(value) != Some(&Definition::Local(initializer.receiver)) {
            return None;
        }
        let wanted = self.text_of(self.tree.child(node, Role::Attribute)?);
        initializer
            .fields
            .iter()
            .find(|(name, _)| name == wanted)
            .map(|&(_, local)| local)
    }

    /// Builds the struct into the return value from its fields' locals, at the end of
    /// `__init__` or a `return` in it, `node`. `false` outside `__init__`.
    pub(crate) fn build_initialized(&mut self, node: NodeId) -> bool {
        let Some(initializer) = &self.initializer else {
            return false;
        };
        let operands = initializer
            .fields
            .iter()
            .map(|&(_, local)| self.take(Place::from(local), self.local_type(local)))
            .collect();
        self.assign(
            Place::from(Local::RETURN),
            Rvalue::Aggregate(self.returns, operands),
            node,
        );
        true
    }
}
