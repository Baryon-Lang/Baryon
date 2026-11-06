//! Scopes: the names visible at one point of a file, innermost first, and how a name is looked
//! up through them.
//!
//! The rules are Python's. A name bound anywhere in a function, by an assignment, a `for`
//! target, an `except ... as`, a pattern or an import, is local to the whole function, even
//! before the binding. A class body is a scope of its own that its methods do not see. A
//! comprehension's targets are local to the comprehension. Type parameters, `[T]` after a
//! function or class name, are visible in its signature and body. What is not local comes from
//! the module's top level, then from the names the language provides.

use database::Definition;
use std::collections::HashMap;

/// What kind of code a scope belongs to. Only the kind decides who can see in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScopeKind {
    /// The module's top level.
    Module,
    /// The type parameters of a function or class, `[T]`.
    TypeParameters,
    /// A function's parameters and locals, or a lambda's parameters.
    Function,
    /// A class body: its fields and methods. Visible only to code directly in the body.
    Class,
    /// A comprehension's targets.
    Comprehension,
}

/// The names bound in one scope.
#[derive(Debug)]
pub(crate) struct Scope {
    /// What kind of code the scope belongs to.
    kind: ScopeKind,
    /// Each name bound here, with its definition, or `None` for a name whose import failed, so
    /// that uses of it are not reported again as undefined.
    names: HashMap<String, Option<Definition>>,
}

impl Scope {
    /// An empty scope of the given kind.
    pub(crate) fn new(kind: ScopeKind) -> Self {
        Self {
            kind,
            names: HashMap::new(),
        }
    }

    /// Binds `name` here, unless it is already bound: the first binding of a name in a scope
    /// is the one every use refers to.
    pub(crate) fn bind(&mut self, name: &str, definition: Option<Definition>) {
        self.names.entry(name.to_owned()).or_insert(definition);
    }

    /// What `name` is bound to in this scope: `None` if it is not bound here.
    pub(crate) fn get(&self, name: &str) -> Option<&Option<Definition>> {
        self.names.get(name)
    }
}

/// The scopes in effect at one point, outermost first.
#[derive(Debug, Default)]
pub(crate) struct Scopes {
    /// The stack of scopes; the last is the innermost.
    stack: Vec<Scope>,
}

impl Scopes {
    /// Enters `scope`, making it the innermost.
    pub(crate) fn push(&mut self, scope: Scope) {
        self.stack.push(scope);
    }

    /// Leaves the innermost scope.
    pub(crate) fn pop(&mut self) {
        self.stack.pop();
    }

    /// The innermost scope, where new bindings go. `None` only before the module scope is
    /// entered.
    pub(crate) fn innermost(&mut self) -> Option<&mut Scope> {
        self.stack.last_mut()
    }

    /// Looks `name` up from the innermost scope outward. A class scope counts only when it is
    /// the innermost, because a method does not see its class's names.
    ///
    /// Returns `None` when no scope binds the name, and `Some(None)` when it is bound by an
    /// import that failed.
    pub(crate) fn lookup(&self, name: &str) -> Option<&Option<Definition>> {
        let innermost = self.stack.len().checked_sub(1)?;
        self.stack
            .iter()
            .enumerate()
            .rev()
            .filter(|&(index, scope)| scope.kind != ScopeKind::Class || index == innermost)
            .find_map(|(_, scope)| scope.get(name))
    }
}
