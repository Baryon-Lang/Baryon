//! A generic struct built with its type arguments written, `Box[f64]([])` (decision 0066,
//! roadmap 1.9.61).
//!
//! Written there, the type arguments are the class's, as they are on a class function,
//! `Bag[f64].empty()`: each field's type has them substituted, and the arguments are checked
//! against those types, so `[]` is an empty `list[f64]` and `Pair[f64](1, 2)` makes its
//! literals `f64`. The call is recorded as the struct's constructor, and its type is the struct
//! with those arguments, which lowering builds as it builds `Box(xs)`. A written type argument
//! is checked against its parameter's bound as it is lowered (the `written_bounds` module).
//!
//! ```text
//! error: `Pair` needs `T` to be a `Number`, found `str`
//! ```
//!
//! A built-in constructor takes no written type arguments: `Array[i64 | None, 1]([10, None])`
//! and `DataFrame[Trade]({...})` are errors that say to name the type on the variable
//! (roadmap 1.9.66). Taking them is idea I.70. The arguments are still checked against the
//! written type, as against an annotation, so the one error is the only one.
//!
//! ```text
//! error: `Array` takes no type arguments at a call
//! ```

use super::Body;
use super::calls::Arguments;
use super::relations;
use crate::outline::{Base, Kind, outline};
use database::File;
use syntax::NodeId;
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

    /// The type of `function(arguments)` at `node`, where `function` is a subscript that names
    /// the built-in `builtin` with type arguments written, which is an error. An `Array` or
    /// `DataFrame` still checks its arguments against the written type, as it does against
    /// an annotation; any other built-in types them alone.
    pub(super) fn written_builtin(
        &mut self,
        node: NodeId,
        function: NodeId,
        builtin: &str,
        arguments: &Arguments,
    ) -> Type<'db> {
        let written = self.tree.text(function, self.text).to_owned();
        let message = format!("`{builtin}` takes no type arguments at a call");
        let hint = format!("name the type on the variable: `x: {written} = {builtin}(...)`");
        let error = self
            .lowerer
            .error(function, &message, "type arguments written", Some(&hint));
        match builtin {
            "Array" | "DataFrame" => {
                let expected = self.lowerer.lower(function);
                if builtin == "Array" {
                    self.array_from_list(node, arguments, Some(expected));
                } else {
                    self.frame_new(node, arguments, Some(expected));
                }
            }
            _ => {
                self.untyped(arguments);
            }
        }
        error
    }
}
