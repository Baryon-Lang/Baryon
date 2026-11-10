//! Methods called on a union (decision 0032): `boundary.flux()` compiles when every member of
//! the union defines `flux` with the same parameters, modes, defaults and return type, and it
//! runs the method of the member the union holds.
//!
//! The call is recorded as calling the first member's method; lowering switches on the member
//! the union holds and calls each member's own method of that name. A union has no methods of
//! its own, so a member without the method, or one whose method differs, is an error that names
//! that member:
//!
//! ```text
//! error: `flux` differs between `Dirichlet` and `Neumann`
//! ```

use super::Body;
use super::calls::Arguments;
use super::exclusive::Passed;
use crate::embed::{self, Lookup, Own};
use syntax::{NodeId, Role};
use types::{Callee, Declaration, Item, Mode, Signature, Type, TypeKind};

impl<'db> Body<'db> {
    /// The call `node`, `value.name(arguments)` with `function` the `value.name` part, where
    /// `value` has the union type `base`.
    pub(super) fn union_method(
        &mut self,
        node: NodeId,
        function: NodeId,
        value: NodeId,
        base: Type<'db>,
        arguments: &Arguments,
    ) -> Type<'db> {
        let (TypeKind::Union(members), Some(attribute)) = (
            base.kind(self.db),
            self.tree.child(function, Role::Attribute),
        ) else {
            return self.untyped(arguments);
        };
        let name = self.tree.text(attribute, self.text);
        let receiver = Passed {
            node: value,
            mode: Mode::Borrow,
            ty: base,
        };
        let mut found: Vec<(Type<'db>, Item<'db>, Signature<'db>)> = Vec::new();
        for &member in members {
            let Some((item, signature)) = self.own_method(member, name) else {
                self.untyped(arguments);
                let message = format!(
                    "`{}`, a member of `{}`, has no method `{name}`",
                    self.show(member),
                    self.show(base)
                );
                return self.lowerer.error(
                    function,
                    &message,
                    "not defined on every member",
                    Some("a method called on a union must be defined by every member (decision 0032); match on the union to call it on some members"),
                );
            };
            if let Some((first, _, shared)) = found.first()
                && !same_shape(shared, &signature)
            {
                self.untyped(arguments);
                let message = format!(
                    "`{name}` differs between `{}` and `{}`",
                    self.show(*first),
                    self.show(member)
                );
                return self.lowerer.error(
                    function,
                    &message,
                    "not the same method on every member",
                    Some("a method called on a union must have the same parameters, modes, defaults and return type in every member (decision 0032)"),
                );
            }
            found.push((member, item, signature));
        }
        let Some((_, item, mut signature)) = found.into_iter().next() else {
            return self.untyped(arguments);
        };
        let receiver = match signature.parameters.first_mut() {
            Some(parameter) => {
                parameter.ty = base;
                Some(Passed {
                    mode: parameter.mode,
                    ..receiver
                })
            }
            None => None,
        };
        self.check_call(
            node,
            name,
            &signature,
            receiver,
            Callee::Method(item),
            arguments,
        )
    }

    /// The method `name` that `member` defines itself or through an embedded field, with its
    /// signature, or `None` when it defines none: `None`, an enum, or a struct without it.
    fn own_method(&self, member: Type<'db>, name: &str) -> Option<(Item<'db>, Signature<'db>)> {
        let TypeKind::Struct(nominal) = member.kind(self.db) else {
            return None;
        };
        let Lookup::Found {
            member: Own::Method { item, .. },
            ..
        } = embed::lookup(self.db, self.lowerer.package, nominal, name)
        else {
            return None;
        };
        let Declaration::Function(signature) = &self.db.declaration(item).declaration else {
            return None;
        };
        Some((item, signature.clone()))
    }
}

/// Whether two members' methods agree in everything but the receiver's type: no type
/// parameters, and the same parameters (name, type, mode and whether defaulted), receiver mode
/// and return type, `Raises` included.
fn same_shape(first: &Signature<'_>, second: &Signature<'_>) -> bool {
    let receiver_mode = |signature: &Signature<'_>| signature.parameters.first().map(|p| p.mode);
    first.type_parameters.is_empty()
        && second.type_parameters.is_empty()
        && first.returns == second.returns
        && receiver_mode(first) == receiver_mode(second)
        && first.parameters.get(1..) == second.parameters.get(1..)
}
