//! The instances a program needs (roadmap 1.9.5, decision 0018): each function compiled at the
//! type arguments it is called at, collected by following calls from the roots, and each body
//! made ready for a backend.

use diagnostics::Diagnostic;
use mir::{Body, Callee, Phase, TerminatorKind};
use types::{Declaration, Item, Nominal, TypeDb};

/// A function and the type arguments it is compiled at: none for a function that is not
/// generic (decision 0018).
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct Instance<'db> {
    /// The function or method.
    pub(crate) item: Item<'db>,
    /// One concrete type per type parameter, in the order the signature lists them.
    pub(crate) arguments: Vec<types::Type<'db>>,
}

/// The instances a program needs, collected as they are found: the roots first, then each
/// instance of a generic function that a collected body calls, once.
pub(crate) struct Instances<'db> {
    /// Every instance found so far, in the order found.
    found: Vec<Instance<'db>>,
    /// The same instances, to find a repeat in constant time.
    seen: std::collections::HashSet<Instance<'db>>,
    /// How many of `found` [`next`](Self::next) has handed out.
    handed: usize,
}

impl<'db> Instances<'db> {
    /// Starts from `roots`, each compiled whether or not anything calls it.
    pub(crate) fn new(roots: Vec<Instance<'db>>) -> Self {
        Self {
            seen: roots.iter().cloned().collect(),
            found: roots,
            handed: 0,
        }
    }

    /// The next instance to compile, or `None` when every instance found so far has been.
    pub(crate) fn next(&mut self) -> Option<Instance<'db>> {
        let instance = self.found.get(self.handed)?.clone();
        self.handed += 1;
        Some(instance)
    }

    /// Adds each instance of a generic function that `body` or one of its `prange` bodies calls
    /// and is not yet found. The body is an instance's, so its type arguments are concrete.
    pub(crate) fn follow(&mut self, body: &Body<'db>) {
        for outlined in &body.outlined {
            self.follow(outlined);
        }
        for block in &body.blocks {
            if let TerminatorKind::Call {
                callee:
                    Callee::Function {
                        item,
                        type_arguments,
                    },
                ..
            } = &block.terminator.kind
                && !type_arguments.is_empty()
            {
                let instance = Instance {
                    item: *item,
                    arguments: type_arguments.clone(),
                };
                if self.seen.insert(instance.clone()) {
                    self.found.push(instance);
                }
            }
        }
    }
}

/// The body of `instance`, ready for a backend: lowered, its type parameters replaced by the
/// instance's arguments, its drops elaborated and its whole-array expressions expanded. Errors
/// for what lowering cannot handle yet, and for a generic method of a generic class, whose
/// class's type parameters are not collected yet.
pub(crate) fn ready<'db>(
    db: &'db dyn TypeDb,
    instance: &Instance<'db>,
    declared: &dyn Fn(&Nominal<'db>) -> types::Declared<'db>,
) -> Result<Body<'db>, Vec<Diagnostic>> {
    let mut body = lowering::lower(db, instance.item)?;
    if !instance.arguments.is_empty() {
        let Declaration::Function(signature) = &db.declaration(instance.item).declaration else {
            return Err(Vec::new());
        };
        if signature.type_parameters.len() != instance.arguments.len() {
            return Err(vec![
                Diagnostic::error("methods of generic classes are not compiled yet")
                    .with_primary(body.span, "a method of a generic class")
                    .with_note("collecting a class's type arguments comes after roadmap 1.9.5"),
            ]);
        }
        body =
            monomorphize::instantiate(db, &body, &signature.type_parameters, &instance.arguments);
    }
    drop_elaboration::elaborate(db, &mut body, declared);
    array_expansion::expand(db, &mut body, declared);
    body.phase = Phase::Ready;
    Ok(body)
}
