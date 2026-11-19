//! A generic struct built with its type arguments written, `Box[f64]([])` (decision 0066,
//! roadmap 1.9.61).
//!
//! Written there, the type arguments are the class's, as they are on a class function,
//! `Bag[f64].empty()`: each field's type has them substituted, and the arguments are checked
//! against those types, so `[]` is an empty `list[f64]` and `Pair[f64](1, 2)` makes its
//! literals `f64`. The call is recorded as the struct's constructor, and its type is the struct
//! with those arguments, which lowering builds as it builds `Box(xs)`. A written type argument
//! is checked against its parameter's bound, as an inferred one is.
//!
//! ```text
//! error: `Pair` needs `T` to be a `Number`, found `str`
//! ```

use super::Body;
use super::calls::Arguments;
use super::relations;
use crate::outline::{Base, Kind, outline};
use crate::traits::bound;
use database::File;
use syntax::{NodeId, Role};
use types::{Callee, Declaration, Item, Mode, Parameter, Signature, Type, TypeKind};

impl<'db> Body<'db> {
    /// The type of `function(arguments)` at `node`, where `function` is a subscript that names
    /// the struct `name` of `file` with its type arguments written: the struct with those
    /// arguments, after the arguments are checked against its fields. `None` when the class is
    /// not a generic struct, which the caller reports.
    pub(super) fn written_constructor(
        &mut self,
        node: NodeId,
        function: NodeId,
        (file, name): (File, NodeId),
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        let Some(Kind::Class(class)) = outline(self.db, file).get(name).map(|entry| &entry.kind)
        else {
            return None;
        };
        if class.base != Base::Struct || class.type_parameters.is_empty() {
            return None;
        }
        let parameters = class.type_parameters.clone();
        let built = self.lowerer.lower(function);
        let TypeKind::Struct(nominal) = built.kind(self.db) else {
            // The lowerer has reported type arguments that do not fit the class.
            return Some(self.untyped(arguments));
        };
        let type_arguments = nominal.arguments.clone();
        let callee = self.tree.text(function, self.text).to_owned();
        self.written_bounds(function, (file, &parameters), &type_arguments);
        let item = Item::new(self.db, self.lowerer.package, file, name);
        let Declaration::Struct { fields, .. } = self.db.declaration(item).declaration.clone()
        else {
            return Some(self.untyped(arguments));
        };
        let constructor = Signature {
            type_parameters: Vec::new(),
            parameters: fields
                .into_iter()
                .map(|field| Parameter {
                    name: field.name,
                    ty: relations::substitute(
                        self.db,
                        field.ty,
                        file,
                        &parameters,
                        &type_arguments,
                    ),
                    mode: Mode::Owned,
                    default: false,
                })
                .collect(),
            returns: built,
        };
        Some(self.check_call(
            node,
            &callee,
            &constructor,
            None,
            Callee::Constructor(item),
            arguments,
        ))
    }

    /// Reports each written type argument of `function` that does not satisfy the bound of
    /// its parameter in `parameters`, declared in `file`: `Pair[str](...)` for
    /// `class Pair[T: Number]`, as an inferred `Pair("a", "b")` is reported.
    fn written_bounds(
        &mut self,
        function: NodeId,
        (file, parameters): (File, &[NodeId]),
        type_arguments: &[Type<'db>],
    ) {
        for (&parameter, &ty) in parameters.iter().zip(type_arguments) {
            let Some(bound) = bound(self.db, self.lowerer.package, file, parameter) else {
                continue;
            };
            if self.satisfies(bound, ty) {
                continue;
            }
            let class = self
                .tree
                .child(function, Role::Value)
                .map_or("", |class| self.tree.text(class, self.text));
            let name = self
                .db
                .parse(file)
                .tree()
                .text(parameter, file.source(self.db).text());
            let message = format!(
                "`{class}` needs `{name}` to be a `{}`, found `{}`",
                bound.name(self.db),
                self.show(ty)
            );
            self.lowerer
                .error(function, &message, "bound not satisfied", None);
        }
    }
}
