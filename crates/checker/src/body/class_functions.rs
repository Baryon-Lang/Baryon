//! A generic struct's methods called through its class with no type arguments written,
//! `Pair.both(1.5)` or `Pair.total(p)`: the class's type arguments are found from the
//! arguments, as a call of its constructor `Pair(x, x)` finds them (the `generics` module).

use super::Body;
use super::calls::Arguments;
use super::members::Member;
use database::File;
use syntax::NodeId;
use types::{Callee, Declaration, Key, Nominal, Signature, Type, TypeKind};

impl<'db> Body<'db> {
    /// `Pair.both(1.5)`: a method of the generic struct `name`, declared in `file` with the
    /// type parameters `parameters`, called through its class `value` with no type arguments
    /// written. The method is checked as a generic function whose type parameters are the
    /// class's, then its own, so each takes the type of the first argument that fixes it,
    /// exactly as the constructor's do. A class type parameter no argument fixes is an error
    /// that asks for it to be written, `Pair[f64].empty()`.
    ///
    /// The checker records the class with the type arguments found as the type of `value`,
    /// where lowering reads them (`types::Call`), and the method's own type arguments in the
    /// call.
    pub(super) fn generic_struct_function(
        &mut self,
        node: NodeId,
        (value, file, name, parameters): (NodeId, File, NodeId, &[NodeId]),
        attribute: NodeId,
        arguments: &Arguments,
    ) -> Type<'db> {
        let class_parameters: Vec<Type<'db>> = parameters
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
        let nominal = Nominal {
            file,
            name,
            arguments: class_parameters.clone(),
        };
        let ty = Type::new(self.db, TypeKind::Struct(nominal.clone()));
        let callee = self.tree.text(attribute, self.text).to_owned();
        let item = match self.member(attribute, &nominal, ty, attribute) {
            Some(Member::Method { item, .. }) if self.typed.promotion(attribute).is_empty() => item,
            Some(Member::Method { .. } | Member::Field(_) | Member::Copy) => {
                self.untyped(arguments);
                let message = format!("`{callee}` is not a function of `{}`", self.show(ty));
                return self.lowerer.error(
                    attribute,
                    &message,
                    "not defined by this class",
                    Some("a class's own methods can be called through it, as `Point.origin()`"),
                );
            }
            None => return self.untyped(arguments),
        };
        let Declaration::Function(signature) = &self.db.declaration(item).declaration else {
            return self.untyped(arguments);
        };
        let signature = Signature {
            type_parameters: class_parameters
                .iter()
                .chain(&signature.type_parameters)
                .copied()
                .collect(),
            ..signature.clone()
        };
        let returns = self.check_call(
            node,
            &callee,
            &signature,
            None,
            Callee::Method(item),
            arguments,
        );
        let Some(call) = self.typed.calls.get_mut(&Key(node)) else {
            return returns;
        };
        let own = call
            .type_arguments
            .split_off(parameters.len().min(call.type_arguments.len()));
        let found = std::mem::replace(&mut call.type_arguments, own);
        let unfixed = parameters
            .iter()
            .zip(&found)
            .find(|(_, ty)| matches!(ty.kind(self.db), TypeKind::Unknown));
        if let Some((&parameter, _)) = unfixed {
            let tree = self.db.parse(file).tree();
            let written = tree.text(parameter, file.source(self.db).text());
            let class = self.tree.text(value, self.text);
            let message =
                format!("`{class}.{callee}` does not say what `{written}` is from its arguments");
            let hint = format!("write it on the class, for example `{class}[f64].{callee}(...)`");
            return self
                .lowerer
                .error(node, &message, "type argument not found", Some(&hint));
        }
        let class = Nominal {
            file,
            name,
            arguments: found,
        };
        let class = Type::new(self.db, TypeKind::Struct(class));
        self.typed.expressions.insert(Key(value), class);
        returns
    }
}
