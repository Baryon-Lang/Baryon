//! Defaults that are not a literal or a `Final` constant, such as `Array.zeros(3)` or a call:
//! each is evaluated at every call that leaves its argument out (decision 0027, roadmap 1.9.13).
//!
//! Each such default is lowered as a function of its own, with no parameters, that returns the
//! default's value. It is named by the parameter's name, so its item stands next to
//! the callee's, in the callee's module: the default's names resolve there, never in the
//! caller's module. A call that leaves the argument out calls it after the arguments that were
//! passed, in the order the parameters are declared, and passes the result as the argument. It
//! raises what the callee declares, so an error the default raises reaches the caller as one
//! the call raised.

use crate::builder::Builder;
use crate::constants::DefaultArgument;
use diagnostics::Diagnostic;
use mir::{Argument, Body, Callee, Local, Operand, Place, TerminatorKind};
use syntax::{NodeId, Role, SyntaxKind, SyntaxTree};
use types::{Declaration, Item, Mode, Type, TypeDb, TypeKind};

/// The body of the function that computes the default of the parameter whose name `item` is:
/// it evaluates the default expression in the callee's module and returns it.
///
/// # Errors
///
/// What lowering cannot handle yet in the expression, and no errors when `item` names no
/// defaulted parameter of a function, which the checker already reports.
pub(crate) fn lower<'db>(
    db: &'db dyn TypeDb,
    item: Item<'db>,
) -> Result<Body<'db>, Vec<Diagnostic>> {
    let tree = db.parse(item.file(db)).tree();
    let (function, class, parameter, position) = owner(tree, item.name(db)).ok_or_else(Vec::new)?;
    let name = tree.child(function, Role::Name).ok_or_else(Vec::new)?;
    let callee = Item::new(db, item.package(db), item.file(db), name);
    let Declaration::Function(signature) = &db.declaration(callee).declaration else {
        return Err(Vec::new());
    };
    let typed = db.body(callee);
    if !db.declaration(callee).diagnostics.is_empty() || !typed.diagnostics.is_empty() {
        return Err(Vec::new());
    }
    let (Some(declared), Some(default)) = (
        signature.parameters.get(position),
        tree.child(parameter, Role::Default),
    ) else {
        return Err(Vec::new());
    };
    let mut builder = Builder::for_default(db, callee, declared.ty, typed, function, class);
    builder.default_body(default);
    let body = builder.finish()?;
    debug_assert_eq!(body.validate(db), [], "lowering built an invalid default");
    Ok(body)
}

/// The named parameters of the function `function` declares, in order: the `Parameter` nodes
/// that have a name, which leaves out the `*` and `/` markers.
pub(crate) fn parameters(tree: &SyntaxTree, function: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    tree.child(function, Role::Parameters)
        .into_iter()
        .flat_map(|parameters| tree.children(parameters))
        .filter(|&parameter| tree.kind(parameter) == SyntaxKind::Parameter)
        .filter(|&parameter| tree.child(parameter, Role::Name).is_some())
}

/// The function that declares the parameter whose `Name` node is `name`, among the top-level
/// statements of `tree` and the bodies of its classes: its `FunctionDefinition`, its class for a
/// method, the `Parameter` node, and its position among the named parameters.
fn owner(tree: &SyntaxTree, name: NodeId) -> Option<(NodeId, Option<NodeId>, NodeId, usize)> {
    let position = |function: NodeId| {
        (tree.kind(function) == SyntaxKind::FunctionDefinition)
            .then(|| {
                parameters(tree, function)
                    .enumerate()
                    .find(|&(_, parameter)| tree.child(parameter, Role::Name) == Some(name))
                    .map(|(position, parameter)| (parameter, position))
            })
            .flatten()
    };
    tree.children_with(tree.root(), Role::Body)
        .find_map(|statement| {
            if let Some((parameter, found)) = position(statement) {
                return Some((statement, None, parameter, found));
            }
            (tree.kind(statement) == SyntaxKind::ClassDefinition).then(|| {
                tree.children_with(statement, Role::Body)
                    .find_map(|method| {
                        let (parameter, found) = position(method)?;
                        Some((method, Some(statement), parameter, found))
                    })
            })?
        })
}

impl<'db> Builder<'db> {
    /// Evaluates `default` into the return value and returns, dropping the temporaries it
    /// made.
    pub(crate) fn default_body(&mut self, default: NodeId) {
        self.enter();
        let returns = self.local_type(Local::RETURN);
        self.write(Place::from(Local::RETURN), returns, default);
        self.leave(default);
        self.terminate(TerminatorKind::Return, default);
    }

    /// The argument the call `node` passes for parameter `position` of `item`, of type `ty` and
    /// mode `mode`, which the call leaves out: the default's constant, or the value its own
    /// function computes, in a temporary dropped when the statement ends. `None` after
    /// reporting what lowering cannot build yet.
    pub(crate) fn default_argument(
        &mut self,
        item: Item<'db>,
        position: usize,
        (mode, ty): (Mode, Type<'db>),
        node: NodeId,
    ) -> Option<Argument<'db>> {
        if mode == Mode::InOut {
            // The callee's changes to a default would go nowhere the caller can see.
            self.unsupported(node, "leaving out an `InOut` argument");
            return None;
        }
        let value = match self.default_value(item, position, node)? {
            DefaultArgument::Constant(constant) => {
                if mode == Mode::Owned {
                    return Some(Argument::Owned(Operand::Constant(constant)));
                }
                let temporary = self.temporary(ty, node);
                let value = mir::Rvalue::Use(Operand::Constant(constant));
                self.assign(Place::from(temporary), value, node);
                return Some(Argument::Borrow(Place::from(temporary)));
            }
            DefaultArgument::Computed(function) => function,
        };
        // A default's function raises what its callee declares, in the same order, so an
        // error it raises is handled as one the call raised.
        let errors: &[Type<'db>] = match &self.db.declaration(item).declaration {
            Declaration::Function(signature) => match signature.returns.kind(self.db) {
                TypeKind::Fallible { errors, .. } => errors,
                _ => &[],
            },
            _ => &[],
        };
        let temporary = self.temporary(ty, node);
        let edge = self.error_edge(errors, node);
        let next = self.new_block();
        self.terminate(
            TerminatorKind::Call {
                callee: Callee::Function {
                    item: value,
                    type_arguments: Vec::new(),
                },
                arguments: Vec::new(),
                destination: Place::from(temporary),
                target: next,
                error: edge.clone(),
            },
            node,
        );
        if let Some(edge) = edge {
            self.dispatch(&edge, errors, node);
        }
        self.start(next);
        self.drop_later(temporary);
        Some(if mode == Mode::Owned {
            Argument::Owned(Operand::Move(Place::from(temporary)))
        } else {
            Argument::Borrow(Place::from(temporary))
        })
    }
}
