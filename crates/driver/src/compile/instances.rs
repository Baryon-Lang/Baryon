//! The instances a program needs (roadmap 1.9.5, decision 0018): each function compiled at the
//! type arguments it is called at, collected by following calls from the roots, and each body
//! made ready for a backend.

use diagnostics::Diagnostic;
use mir::{Body, Callee, Phase, TerminatorKind};
use syntax::{Role, SyntaxKind};
use types::{Declaration, Item, Nominal, TypeDb, TypeKind};

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
/// instance that a collected body calls, once: an instance of a generic function, or the
/// function that computes a default (decision 0027), which is never a root.
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

    /// Adds each instance that `body` or one of its `prange` bodies calls and is not yet found.
    /// The body is an instance's, so its type arguments are concrete. A call of a function
    /// that is not generic finds its root, already found, unless the function is one the roots
    /// leave out, such as a default's.
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

impl<'db> Instances<'db> {
    /// Adds the `__del__` of each struct a drop in `body` may reach and is not yet found
    /// (decision 0063): the struct of a local, or one a local holds in a field, an element or a
    /// member, however deep. The backend calls the method from inside a drop, which no call in
    /// the body names, so a library class's method, never a root, is found here.
    pub(crate) fn follow_drops(
        &mut self,
        db: &'db dyn TypeDb,
        body: &Body<'db>,
        declared: &dyn Fn(&Nominal<'db>) -> types::Declared<'db>,
        destructor: &dyn Fn(&Nominal<'db>) -> Option<Item<'db>>,
    ) {
        let mut visited = std::collections::HashSet::new();
        let mut pending: Vec<types::Type<'db>> = body.locals.iter().map(|local| local.ty).collect();
        pending.extend(
            body.outlined
                .iter()
                .flat_map(|outlined| outlined.locals.iter().map(|local| local.ty)),
        );
        while let Some(ty) = pending.pop() {
            if !visited.insert(ty) {
                continue;
            }
            match ty.kind(db) {
                TypeKind::Tuple(members) | TypeKind::Union(members) => pending.extend(members),
                TypeKind::List(element) | TypeKind::Optional(element) => pending.push(*element),
                TypeKind::Dict { key, value } => pending.extend([*key, *value]),
                TypeKind::Struct(nominal) => {
                    if let Some(item) = destructor(nominal) {
                        let instance = Instance {
                            item,
                            arguments: Vec::new(),
                        };
                        if self.seen.insert(instance.clone()) {
                            self.found.push(instance);
                        }
                    }
                    if let types::Declared::Struct(fields) = declared(nominal) {
                        pending.extend(fields);
                    }
                }
                _ => {}
            }
        }
    }
}

/// The body of `instance`, ready for a backend: lowered, its type parameters replaced by the
/// instance's arguments, each call of a trait's method sent to the method that implements it
/// (`implementation`, decision 0031), its drops elaborated and its whole-array expressions
/// expanded. Errors for what lowering cannot handle yet.
///
/// A method of a generic class, such as `Pair[T]`'s `total`, is generic in its class's type
/// parameters: they come first, before the method's own and its `Callable` ones, as lowering
/// passes them at each call (roadmap 1.9.18).
pub(crate) fn ready<'db>(
    db: &'db dyn TypeDb,
    instance: &Instance<'db>,
    declared: &dyn Fn(&Nominal<'db>) -> types::Declared<'db>,
    destructor: &dyn Fn(&Nominal<'db>) -> bool,
    implementation: &monomorphize::Implementation<'_, 'db>,
) -> Result<Body<'db>, Vec<Diagnostic>> {
    let mut body = lowering::lower(db, instance.item)?;
    if !instance.arguments.is_empty() {
        let Declaration::Function(signature) = &db.declaration(instance.item).declaration else {
            return Err(Vec::new());
        };
        // Each `Callable` parameter is an implicit type parameter after the written ones
        // (decision 0026).
        let parameters: Vec<types::Type<'db>> = class_parameters(db, instance.item)
            .into_iter()
            .chain(signature.type_parameters.iter().copied())
            .chain(
                signature
                    .parameters
                    .iter()
                    .map(|parameter| parameter.ty)
                    .filter(|ty| matches!(ty.kind(db), TypeKind::Callable { .. })),
            )
            .collect();
        if parameters.len() != instance.arguments.len() {
            return Err(vec![
                Diagnostic::error("this call's type arguments do not match the function's")
                    .with_primary(body.span, "compiled at the wrong number of types")
                    .with_note("this is a compiler bug; please report it"),
            ]);
        }
        body = monomorphize::instantiate(db, &body, &parameters, &instance.arguments);
        monomorphize::forward_callables(db, &mut body, &|item| callable_positions(db, item));
        monomorphize::trait_calls(db, &mut body, implementation);
    }
    array_fusion::fuse(db, &mut body);
    drop_elaboration::elaborate(db, &mut body, declared, destructor);
    array_expansion::expand(db, &mut body, declared);
    bounds_hoisting::hoist(db, &mut body, declared);
    body.phase = Phase::Ready;
    Ok(body)
}

/// The type parameters of the class that declares the method `item`, as types, in order: `T`
/// for a method of `class Pair[T: Number](Struct)`. Empty for a top-level function and for a
/// method of a class that has none, such as a trait (decision 0031), whose `Self` is not a
/// written type parameter.
pub(crate) fn class_parameters<'db>(db: &'db dyn TypeDb, item: Item<'db>) -> Vec<types::Type<'db>> {
    let file = item.file(db);
    let tree = db.parse(file).tree();
    let name = item.name(db);
    let Some(class) = tree
        .children_with(tree.root(), Role::Body)
        .filter(|&statement| tree.kind(statement) == SyntaxKind::ClassDefinition)
        .find(|&class| {
            tree.children_with(class, Role::Body)
                .any(|method| tree.child(method, Role::Name) == Some(name))
        })
    else {
        return Vec::new();
    };
    tree.child(class, Role::TypeParameters)
        .into_iter()
        .flat_map(|parameters| tree.children_with(parameters, Role::Item))
        .filter_map(|parameter| tree.child(parameter, Role::Name))
        .map(|name| types::Type::new(db, TypeKind::Parameter { file, name }))
        .collect()
}

/// The positions of the `Callable` parameters of the function `item` among its parameters, the
/// receiver first for a method: the parameters whose type arguments are the functions passed
/// (decision 0026).
fn callable_positions<'db>(db: &'db dyn TypeDb, item: Item<'db>) -> Vec<usize> {
    let Declaration::Function(signature) = &db.declaration(item).declaration else {
        return Vec::new();
    };
    signature
        .parameters
        .iter()
        .enumerate()
        .filter(|(_, parameter)| matches!(parameter.ty.kind(db), TypeKind::Callable { .. }))
        .map(|(position, _)| position)
        .collect()
}
