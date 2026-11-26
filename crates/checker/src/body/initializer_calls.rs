//! Calls of a class whose struct defines `__init__` (decision 0068, roadmap 1.7.17): the call
//! is checked against `__init__`'s parameters after `self`, and gives the struct.
//!
//! A generic struct's `__init__` is generic in the class's type parameters, then its own. Called
//! as `Grid(xs)`, the class's type arguments are found from the arguments, as a call of the
//! constructor `Pair(x, x)` finds them; written as `Grid[f64](xs)`, they are substituted into
//! `__init__`'s parameters first, so `Grid[f64]([])` makes the `[]` a `list[f64]`. A class type
//! parameter no argument fixes is an error that asks for it to be written:
//!
//! ```text
//! error: `Grid` does not say what `T` is from its arguments
//! ```
//!
//! Either way the call records only `__init__`'s own type arguments, and its type is the struct
//! with the class's, where lowering reads them, as it does for a class function
//! (`class_functions` module).

use super::Body;
use super::calls::Arguments;
use super::relations;
use crate::outline::{Kind, outline};
use database::File;
use syntax::NodeId;
use types::{Callee, Declaration, Item, Key, Parameter, Signature, Type, TypeKind};

impl<'db> Body<'db> {
    /// The call `node` of the struct `name` of `file`, written `callee`, which calls its
    /// `__init__`, `initializer`, with no type arguments written: a generic struct's are found
    /// from the arguments.
    pub(super) fn initialized(
        &mut self,
        node: NodeId,
        callee: &str,
        (file, name): (File, NodeId),
        initializer: Item<'db>,
        arguments: &Arguments,
    ) -> Type<'db> {
        let Declaration::Function(signature) = &self.db.declaration(initializer).declaration else {
            return self.untyped(arguments);
        };
        let parameters = self.class_type_parameters(file, name);
        let class: Vec<Type<'db>> = parameters
            .iter()
            .map(|&parameter| {
                Type::new(
                    self.db,
                    TypeKind::Parameter {
                        file,
                        name: parameter,
                    },
                )
            })
            .collect();
        let signature = Signature {
            type_parameters: class
                .iter()
                .chain(&signature.type_parameters)
                .copied()
                .collect(),
            ..signature.clone()
        };
        let before = self.lowerer.diagnostics.len();
        let method = Callee::Method(initializer);
        let returns = self.check_call(node, callee, &signature, None, method, arguments);
        if parameters.is_empty() {
            return returns;
        }
        let Some(call) = self.typed.calls.get_mut(&Key(node)) else {
            return returns;
        };
        // Lowering finds the class's type arguments in the struct the call gives.
        let own = call
            .type_arguments
            .split_off(parameters.len().min(call.type_arguments.len()));
        let found = std::mem::replace(&mut call.type_arguments, own);
        let unfixed = parameters
            .iter()
            .zip(&found)
            .find(|(_, ty)| relations::is_unknown(self.db, **ty));
        match unfixed {
            Some((&parameter, _)) if self.lowerer.diagnostics.len() == before => {
                let tree = self.db.parse(file).tree();
                let written = tree.text(parameter, file.source(self.db).text());
                let message =
                    format!("`{callee}` does not say what `{written}` is from its arguments");
                let hint = format!("write it on the class, for example `{callee}[f64](...)`");
                self.lowerer
                    .error(node, &message, "type argument not found", Some(&hint))
            }
            _ => returns,
        }
    }

    /// `Grid[f64](xs)`: the call `node` of the generic struct `name` of `file`, written
    /// `callee`, with its type arguments written, so the struct is `built`. It calls the
    /// struct's `__init__`, `initializer`, whose parameters have those type arguments in place.
    pub(super) fn written_initialized(
        &mut self,
        node: NodeId,
        (callee, built): (&str, Type<'db>),
        (file, name): (File, NodeId),
        initializer: Item<'db>,
        arguments: &Arguments,
    ) -> Type<'db> {
        let (Declaration::Function(signature), TypeKind::Struct(nominal)) = (
            &self.db.declaration(initializer).declaration,
            built.kind(self.db),
        ) else {
            return self.untyped(arguments);
        };
        let parameters = self.class_type_parameters(file, name);
        let parameters = signature
            .parameters
            .iter()
            .map(|parameter| Parameter {
                ty: relations::substitute(
                    self.db,
                    parameter.ty,
                    file,
                    &parameters,
                    &nominal.arguments,
                ),
                ..parameter.clone()
            })
            .collect();
        let signature = Signature {
            parameters,
            returns: built,
            ..signature.clone()
        };
        let method = Callee::Method(initializer);
        self.check_call(node, callee, &signature, None, method, arguments)
    }

    /// The type parameters the class `name` of `file` declares, each by its `Name` node.
    fn class_type_parameters(&self, file: File, name: NodeId) -> Vec<NodeId> {
        match outline(self.db, file).get(name).map(|entry| &entry.kind) {
            Some(Kind::Class(class)) => class.type_parameters.clone(),
            _ => Vec::new(),
        }
    }
}
