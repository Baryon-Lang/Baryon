//! A struct's `__init__`, which builds it from other arguments (decision 0068, roadmap 1.7.17).
//!
//! A struct that defines `__init__(self, ...)` is built by calling the class with `__init__`'s
//! arguments, `CubicSpline(x, y)`, and no longer from its fields. Callers see `__init__` as a
//! function of the class: its signature leaves out `self` and returns the struct, so a call of
//! the class is checked and lowered as a call of that function. Its body is checked in the
//! `body` module, which tracks the fields it sets; lowering keeps each field in a local while
//! the body runs and builds the struct where it ends or returns.
//!
//! This module finds which methods are `__init__` and checks the shape of one where it is
//! declared:
//!
//! ```text
//! error: `__init__` returns nothing
//! ```

use crate::embed::{self, Own};
use crate::lower::Lowerer;
use crate::outline::{Base, Entry, Kind, outline};
use database::{Db, File, Package};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Item, Nominal, Parameter, Signature, TypeDb, TypeKind};

/// Whether `entry`, a definition of `file`, is the `__init__` of a struct. An `__init__` in an
/// error type or a trait is an ordinary method.
pub(crate) fn is_initializer(db: &dyn Db, file: File, entry: &Entry) -> bool {
    let Some(class) = entry.class else {
        return false;
    };
    let struct_class = matches!(
        outline(db, file).get(class).map(|class| &class.kind),
        Some(Kind::Class(class)) if class.base == Base::Struct
    );
    struct_class
        && entry.kind == Kind::Function
        && db
            .parse(file)
            .tree()
            .text(entry.name, file.source(db).text())
            == "__init__"
}

/// The `__init__` the struct `nominal`, a class of `package`, defines itself: the function a
/// call of the class calls. `None` when it has none, so the call builds it from its fields.
pub(crate) fn initializer<'db>(
    db: &'db dyn TypeDb,
    package: Package,
    nominal: &Nominal<'db>,
) -> Option<Item<'db>> {
    match embed::own(db, package, nominal, "__init__")? {
        Own::Method { item, .. } => Some(item),
        Own::Field(_) => None,
    }
}

impl<'db> Lowerer<'db> {
    /// The signature callers see for the `__init__` declared by the statement `function`, from
    /// its written `parameters`, `self` first, and its written return type `returns`: the
    /// parameters after `self`, returning the struct. A return type other than `None` and a
    /// missing `self` are reported. A generic struct's `__init__` keeps only its own type
    /// parameters here; a call of the class adds the class's (`body::initializer_calls`).
    pub(crate) fn initializer_signature(
        &mut self,
        function: NodeId,
        mut signature: Signature<'db>,
    ) -> Signature<'db> {
        let built = self.self_type();
        if let Some(returns) = self.tree.child(function, Role::Returns)
            && !matches!(
                signature.returns.kind(self.db),
                TypeKind::None | TypeKind::Unknown
            )
        {
            self.error(
                returns,
                "`__init__` returns nothing",
                "a return type",
                Some(
                    "calling the class gives the struct `__init__` builds; remove the return \
                     type, and let a bad argument panic (decision 0068)",
                ),
            );
        }
        let has_self = self
            .tree
            .child(function, Role::Parameters)
            .into_iter()
            .flat_map(|parameters| self.tree.children(parameters))
            .find(|&parameter| self.tree.kind(parameter) == SyntaxKind::Parameter)
            .is_some_and(|parameter| self.tree.child(parameter, Role::Annotation).is_none());
        if !has_self {
            let name = self.tree.child(function, Role::Name).unwrap_or(function);
            self.error(
                name,
                "`__init__` takes `self` first, with no type",
                "no `self`",
                Some("write `def __init__(self, x: f64):`"),
            );
        }
        let parameters: Vec<Parameter<'db>> = signature.parameters.drain(..).skip(1).collect();
        Signature {
            parameters,
            returns: built,
            ..signature
        }
    }
}
