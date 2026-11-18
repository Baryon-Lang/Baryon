//! Calls the checker cannot give a type to, reported where they are written (roadmap 1.9.48 and
//! 1.9.49) instead of passing on an unknown type that a later stage stops on with a message
//! about itself, such as "lowering does not handle this call yet":
//!
//! - a type parameter called as a conversion, `T(0)`, which no bound allows;
//! - a generic struct built with its type arguments written, `Box[f64]([])`, where version 0
//!   finds them from the arguments;
//! - a method called on a value whose type is a type parameter with no trait that has the
//!   method, `x.copy()` for `def last[T](xs: list[T])`;
//! - a generic function or constructor whose arguments do not fix one of its type parameters,
//!   `make()` for `def make[T]() -> list[T]`;
//! - a member a standard module does not have, `math.nothing(1.0)`, or `norm(r)` after
//!   `from linalg import norm` (roadmap 1.9.52).
//!
//! ```text
//! error: `T` is a type parameter, which cannot be called
//! ```

use super::Body;
use super::calls::Arguments;
use super::relations;
use crate::outline::{Kind, outline};
use database::Definition;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Key, Signature, Type, TypeKind};

impl<'db> Body<'db> {
    /// The error for `function(arguments)` at `node` when `function` is a type parameter in
    /// scope, `T(0)`, or a generic struct with its type arguments written, `Box[f64]([])`;
    /// `None` for any other call.
    pub(super) fn untyped_call(
        &mut self,
        node: NodeId,
        function: NodeId,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        match self.tree.kind(function) {
            SyntaxKind::Name => {
                let &Definition::Local(binding) = self.lowerer.definition(function)? else {
                    return None;
                };
                if !self.lowerer.is_type_parameter(binding) {
                    return None;
                }
                self.untyped(arguments);
                let name = self.tree.text(function, self.text);
                let message = format!("`{name}` is a type parameter, which cannot be called");
                Some(self.lowerer.error(
                    node,
                    &message,
                    "not a function",
                    Some(
                        "a type parameter has only what its bound allows; a conversion names a \
                         number type, such as `f64(n)`, and a literal takes the type `T` under \
                         `Float`",
                    ),
                ))
            }
            SyntaxKind::Subscript => {
                let class = self.tree.child(function, Role::Value)?;
                let &Definition::Item(file, name) = self.lowerer.definition(class)? else {
                    return None;
                };
                if !matches!(
                    outline(self.db, file).get(name).map(|entry| &entry.kind),
                    Some(Kind::Class(_))
                ) {
                    return None;
                }
                self.untyped(arguments);
                let class = self.tree.text(class, self.text);
                let message = format!("`{class}` is built without its type arguments written");
                let hint = format!(
                    "the arguments say what they are, `{class}(...)`; give an empty list a typed \
                     variable first, `items: list[f64] = []`"
                );
                Some(
                    self.lowerer
                        .error(node, &message, "type arguments written", Some(&hint)),
                )
            }
            _ => None,
        }
    }

    /// The error for the method called at `function`, an `Attribute` node, on `base`, a type
    /// parameter with no trait that has it: `x.copy()` for `def last[T](xs: list[T])`.
    pub(super) fn parameter_method(
        &mut self,
        function: NodeId,
        base: Type<'db>,
        arguments: &Arguments,
    ) -> Type<'db> {
        self.untyped(arguments);
        let attribute = self.tree.child(function, Role::Attribute);
        let name = attribute.map_or("", |attribute| self.tree.text(attribute, self.text));
        let message = format!("`{}` has no method `{name}`", self.show(base));
        self.lowerer.error(
            function,
            &message,
            "not known for a type parameter",
            Some(
                "a type parameter has only what its bound allows, and a trait has methods \
                 only: bound it by a trait that lists the method",
            ),
        )
    }

    /// Reports a type parameter of the generic function or constructor called at `node` that
    /// no argument fixed, and returns the unknown type: `make()` for
    /// `def make[T]() -> list[T]`. Nothing is reported when checking the call reported an
    /// error already, its count of errors grown past `before`: a wrong argument leaves its
    /// type parameter unfixed too.
    pub(super) fn unfixed_type_argument(
        &mut self,
        (node, before): (NodeId, usize),
        callee: &str,
        signature: &Signature<'db>,
    ) -> Option<Type<'db>> {
        if self.lowerer.diagnostics.len() > before {
            return None;
        }
        let call = self.typed.calls.get(&Key(node))?;
        let (file, name) = signature
            .type_parameters
            .iter()
            .zip(&call.type_arguments)
            .find_map(|(parameter, argument)| match parameter.kind(self.db) {
                TypeKind::Parameter { file, name } if relations::is_unknown(self.db, *argument) => {
                    Some((*file, *name))
                }
                _ => None,
            })?;
        let written = self
            .db
            .parse(file)
            .tree()
            .text(name, file.source(self.db).text());
        let message = format!("`{callee}` does not say what `{written}` is from its arguments");
        let hint = format!(
            "a type argument is never written at a call; give the function a parameter whose \
             argument fixes `{written}`"
        );
        Some(
            self.lowerer
                .error(node, &message, "type argument not found", Some(&hint)),
        )
    }

    /// The error for the call `node` of `module.member`, a member the standard module `module`
    /// does not have: neither its library source (decision 0038) nor the compiler defines it,
    /// as for `norm` after `from linalg import norm`.
    pub(super) fn missing_member(
        &mut self,
        node: NodeId,
        (module, member): (&str, &str),
        arguments: &Arguments,
    ) -> Type<'db> {
        self.untyped(arguments);
        let message = format!("`{module}.{member}` is not in Baryon yet");
        let hint = format!(
            "the language reference lists what `{module}` has; the roadmap's Ideas table is the \
             place to ask for more"
        );
        self.lowerer.error(node, &message, "not built", Some(&hint))
    }

    /// A call the checker has no rule for, at `node`, whose callee resolved to `resolved`: the
    /// error for a member a standard module does not have, or else the unknown type, as for
    /// a callee whose own error was reported where it is named.
    pub(super) fn unresolved_call(
        &mut self,
        node: NodeId,
        resolved: Option<&Definition>,
        arguments: &Arguments,
    ) -> Type<'db> {
        match resolved {
            Some(&Definition::Standard {
                module,
                member: Some(ref member),
            }) => {
                let member = member.clone();
                self.missing_member(node, (module, &member), arguments)
            }
            _ => self.untyped(arguments),
        }
    }
}
