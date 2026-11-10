//! Whether a struct defines the methods of the traits it names (decision 0031), checked at
//! the class: each method of each trait it names, and of each trait those require, is defined
//! by the struct or an embedded struct, with the trait's parameters, modes and return type,
//! `Self` read as the struct. The receiver's type is not compared, so a method promoted from an
//! embedded struct, whose `self` is that struct, satisfies the trait, unless it takes `self` as
//! `Owned`, which would move the embedded field out of the outer struct.

use crate::body::relations;
use crate::classes::Classes;
use crate::embed::{self, Lookup, Own};
use crate::outline::Entry;
use crate::traits::{self, UserTrait, user_trait};
use diagnostics::Diagnostic;
use syntax::{NodeId, Role};
use types::{Declaration, Item, Mode, Nominal, Signature, Type, TypeKind};

impl<'db> Classes<'db> {
    /// Reports each method of each trait the struct names, or a trait one requires, that the
    /// struct does not define, or defines with another signature. Each trait method is checked
    /// once, at the first base that brings it in.
    pub(crate) fn conformance(
        &self,
        entry: &Entry,
        nominal: &Nominal<'db>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let implementor = Type::new(self.db, TypeKind::Struct(nominal.clone()));
        let written = self.text(entry.name);
        let bases: Vec<NodeId> = self
            .tree
            .child(entry.statement, Role::Arguments)
            .into_iter()
            .flat_map(|arguments| self.tree.children_with(arguments, Role::Item))
            .collect();
        let mut checked: Vec<Item<'db>> = Vec::new();
        for base in bases {
            let Some(named) = user_trait(self.db, self.package, self.file, base) else {
                continue;
            };
            for user in traits::closure(self.db, self.package, [named]) {
                for method in user.own_methods(self.db, self.package) {
                    if checked.contains(&method) {
                        continue;
                    }
                    checked.push(method);
                    let Declaration::Function(required) = &self.db.declaration(method).declaration
                    else {
                        continue;
                    };
                    let required = self.replace_self(required, user, implementor);
                    let name = self.method_name(method);
                    let trait_name = user.name(self.db);
                    let found = match embed::lookup(self.db, self.package, nominal, &name) {
                        Lookup::Found {
                            member:
                                Own::Method {
                                    item,
                                    parameters,
                                    arguments,
                                },
                            owner,
                            path,
                        } => Some((item, parameters, arguments, owner, path)),
                        Lookup::Ambiguous(_) => continue,
                        Lookup::Found { .. } | Lookup::Missing => None,
                    };
                    let Some((item, parameters, arguments, owner, path)) = found else {
                        diagnostics.push(
                            Diagnostic::error(format!(
                                "`{written}` does not define `{name}`, which the trait \
                                 `{trait_name}` lists"
                            ))
                            .with_primary(self.tree.span(base), "the trait is named here")
                            .with_note(format!(
                                "add `{}` to `{written}`",
                                self.show(&name, &required)
                            )),
                        );
                        continue;
                    };
                    let Declaration::Function(defined) = &self.db.declaration(item).declaration
                    else {
                        continue;
                    };
                    let defined = relations::substitute_signature(
                        self.db,
                        defined,
                        owner.file,
                        &parameters,
                        &arguments,
                    );
                    let promoted_owned = !path.is_empty()
                        && defined
                            .parameters
                            .first()
                            .is_some_and(|receiver| receiver.mode == Mode::Owned);
                    if promoted_owned {
                        diagnostics.push(self.owned_promotion(base, written, &name, &owner));
                    } else if !self.matches(&required, &defined) {
                        let owner_name = self.class_name(&owner);
                        let tree = self.db.parse(item.file(self.db)).tree();
                        diagnostics.push(
                            Diagnostic::error(format!(
                                "`{owner_name}.{name}` does not match `{name}` in the trait \
                                 `{trait_name}`"
                            ))
                            .with_primary(self.tree.span(base), "the trait is named here")
                            .with_secondary(tree.span(item.name(self.db)), "defined here")
                            .with_note(format!(
                                "`{trait_name}` lists `{}`",
                                self.show(&name, &required)
                            ))
                            .with_note(format!(
                                "`{owner_name}` defines `{}`",
                                self.show(&name, &defined)
                            )),
                        );
                    }
                }
            }
        }
    }

    /// The error for a trait method that a struct gets from an embedded struct, `owner`, whose
    /// method takes its `self` as `Owned`: called through the trait, it would move the embedded
    /// field out of the outer struct.
    fn owned_promotion(
        &self,
        base: NodeId,
        written: &str,
        name: &str,
        owner: &Nominal<'db>,
    ) -> Diagnostic {
        let owner_name = self.class_name(owner);
        Diagnostic::error(format!(
            "`{written}` gets `{name}` from the embedded `{owner_name}`, which takes it as `Owned`"
        ))
        .with_primary(self.tree.span(base), "the trait is named here")
        .with_note(format!(
            "called through the trait, it would move the `{owner_name}` out of a `{written}`; \
             define `{name}` on `{written}` itself"
        ))
    }

    /// The name of the method `method`.
    fn method_name(&self, method: Item<'db>) -> String {
        let file = method.file(self.db);
        self.db
            .parse(file)
            .tree()
            .text(method.name(self.db), file.source(self.db).text())
            .to_owned()
    }

    /// `signature`, a method of the trait `user`, with `Self` replaced by `implementor`.
    fn replace_self(
        &self,
        signature: &Signature<'db>,
        user: UserTrait,
        implementor: Type<'db>,
    ) -> Signature<'db> {
        relations::substitute_signature(self.db, signature, user.file, &[user.name], &[implementor])
    }

    /// Whether the defined method matches the required one: the same number of parameters,
    /// the same modes, the same types after the receiver, the same return type, and no type
    /// parameters. A type unknown from an earlier error matches anything.
    fn matches(&self, required: &Signature<'db>, defined: &Signature<'db>) -> bool {
        let same = |a: Type<'db>, b: Type<'db>| {
            a == b || relations::is_unknown(self.db, a) || relations::is_unknown(self.db, b)
        };
        defined.type_parameters.is_empty()
            && required.parameters.len() == defined.parameters.len()
            && required
                .parameters
                .iter()
                .zip(&defined.parameters)
                .enumerate()
                .all(|(position, (wanted, found))| {
                    wanted.mode == found.mode && (position == 0 || same(wanted.ty, found.ty))
                })
            && same(required.returns, defined.returns)
    }

    /// A method's signature as a program writes it: `def energy(self) -> f64`.
    fn show(&self, name: &str, signature: &Signature<'db>) -> String {
        let parameters: Vec<String> = signature
            .parameters
            .iter()
            .enumerate()
            .map(|(position, parameter)| {
                let ty = parameter.ty.display(self.db).to_string();
                let ty = match parameter.mode {
                    Mode::Borrow => ty,
                    Mode::InOut => format!("InOut[{ty}]"),
                    Mode::Owned => format!("Owned[{ty}]"),
                };
                match (position, parameter.mode) {
                    (0, Mode::Borrow) => parameter.name.clone(),
                    (0, Mode::InOut) => format!("{}: InOut[Self]", parameter.name),
                    (0, Mode::Owned) => format!("{}: Owned[Self]", parameter.name),
                    _ => format!("{}: {ty}", parameter.name),
                }
            })
            .collect();
        let returns = match signature.returns.kind(self.db) {
            TypeKind::None => String::new(),
            _ => format!(" -> {}", signature.returns.display(self.db)),
        };
        format!("def {name}({}){returns}", parameters.join(", "))
    }
}
