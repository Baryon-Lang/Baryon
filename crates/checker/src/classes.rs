//! Checks on whole classes that need other classes' declarations (decision 0031): a trait's
//! methods and what it requires, and a struct's traits and embedded fields.
//!
//! These run after every declaration, in [`check_file`](crate::check_file), rather than while
//! declaring a class. A declaration then reads only its own annotations and names, so a struct
//! that embeds another, or a trait that requires another, never waits on that class's
//! declaration, and classes that refer to each other in error give one message, here, rather
//! than a cycle.
//!
//! For a trait, `class Body(Protocol)`:
//!
//! - each method's body is `...`, after an optional documentation string: default bodies are
//!   not in version 0 (Ideas I.31), and a method takes no type parameters;
//! - it does not require itself, directly or through another trait;
//! - no two of the traits it is made of list a method of the same name.
//!
//! For a struct or error type:
//!
//! - it does not embed itself through another struct (embedding itself directly is reported
//!   with its field);
//! - no name is provided by two embedded structs at the same depth, unless a shallower name
//!   hides it: the error is at the class, never a silent pick where the name is used;
//! - it defines the methods of every trait it names (the `conformance` module).

use crate::embed;
use crate::outline::{Base, Entry, Kind, outline};
use crate::traits::{self, UserTrait};
use database::{File, Package};
use diagnostics::Diagnostic;
use std::collections::BTreeMap;
use syntax::{NodeId, Role, SyntaxKind, SyntaxTree};
use types::{Declaration, Item, Nominal, Type, TypeDb, TypeKind};

/// The errors of every class in `file`, a file of `package`, in source order.
///
/// Cost: per trait, one pass over its methods and the traits it requires; per struct, one walk
/// over the structs it embeds and one lookup per method of each trait it names. Every
/// declaration read is the cached [`TypeDb::declaration`].
pub(crate) fn check(db: &dyn TypeDb, package: Package, file: File) -> Vec<Diagnostic> {
    let checker = Classes {
        db,
        package,
        file,
        tree: db.parse(file).tree(),
        text: file.source(db).text(),
    };
    let mut diagnostics = Vec::new();
    for entry in outline(db, file).entries() {
        let Kind::Class(class) = &entry.kind else {
            continue;
        };
        match class.base {
            Base::Protocol => checker.trait_class(entry, &mut diagnostics),
            Base::Struct | Base::Error => checker.struct_class(entry, &mut diagnostics),
            Base::Enum => {}
        }
    }
    diagnostics
}

/// What checking the classes of one file reads.
pub(crate) struct Classes<'db> {
    /// The database.
    pub(crate) db: &'db dyn TypeDb,
    /// The package the file belongs to.
    pub(crate) package: Package,
    /// The file.
    pub(crate) file: File,
    /// Its tree.
    pub(crate) tree: &'db SyntaxTree,
    /// Its text.
    pub(crate) text: &'db str,
}

impl<'db> Classes<'db> {
    /// The text of `node`.
    pub(crate) fn text(&self, node: NodeId) -> &'db str {
        self.tree.text(node, self.text)
    }

    /// Checks the trait `entry`: its methods' bodies, what it requires, and its method names.
    fn trait_class(&self, entry: &Entry, diagnostics: &mut Vec<Diagnostic>) {
        let methods = self
            .tree
            .children_with(entry.statement, Role::Body)
            .filter(|&statement| self.tree.kind(statement) == SyntaxKind::FunctionDefinition);
        for method in methods {
            let name = self.tree.child(method, Role::Name).unwrap_or(method);
            if let Some(parameters) = self.tree.child(method, Role::TypeParameters) {
                diagnostics.push(
                    Diagnostic::error("a trait's method takes no type parameters in version 0")
                        .with_primary(self.tree.span(parameters), "not in version 0")
                        .with_note("decision 0031 leaves generic trait methods for later"),
                );
            }
            if !self.is_elided(method) {
                let written = self.text(name);
                diagnostics.push(
                    Diagnostic::error("a trait's method has the body `...`")
                        .with_primary(self.tree.span(name), "has a body")
                        .with_note(format!(
                            "a trait lists its methods; each type that implements it defines \
                             them: `def {written}(self) -> f64: ...`"
                        ))
                        .with_note(
                            "default method bodies are not in version 0 (decision 0031, Ideas \
                             I.31)",
                        ),
                );
            }
        }
        let own = UserTrait {
            file: self.file,
            name: entry.name,
        };
        let item = own.item(self.db, self.package);
        let Declaration::Trait { requires, .. } = &self.db.declaration(item).declaration else {
            return;
        };
        let required = traits::closure(
            self.db,
            self.package,
            requires
                .iter()
                .map(|&item| UserTrait::of_item(self.db, item)),
        );
        let written = self.text(entry.name);
        if required.contains(&own) {
            diagnostics.push(
                Diagnostic::error(format!("`{written}` requires itself"))
                    .with_primary(self.tree.span(entry.name), "through the traits it requires")
                    .with_note("a trait cannot be part of what it requires"),
            );
            return;
        }
        let mut listed: BTreeMap<String, UserTrait> = BTreeMap::new();
        for user in traits::closure(self.db, self.package, [own]) {
            let tree = self.db.parse(user.file).tree();
            let text = user.file.source(self.db).text();
            for method in user.own_methods(self.db, self.package) {
                let name = tree.text(method.name(self.db), text).to_owned();
                match listed.get(&name) {
                    Some(&first) if first != user => {
                        diagnostics.push(
                            Diagnostic::error(format!(
                                "`{written}` gets two methods named `{name}`"
                            ))
                            .with_primary(self.tree.span(entry.name), "")
                            .with_note(format!(
                                "both `{}` and `{}` list `{name}`; a call on a type parameter \
                                 bounded by `{written}` could not tell which is meant",
                                first.name(self.db),
                                user.name(self.db)
                            )),
                        );
                    }
                    Some(_) => {}
                    None => {
                        listed.insert(name, user);
                    }
                }
            }
        }
    }

    /// Whether the method `method`'s body is `...`, after an optional documentation string.
    fn is_elided(&self, method: NodeId) -> bool {
        let statements: Vec<NodeId> = self.tree.children_with(method, Role::Body).collect();
        let value = |statement: NodeId| {
            (self.tree.kind(statement) == SyntaxKind::ExpressionStatement)
                .then(|| self.tree.child(statement, Role::Value))
                .flatten()
                .map(|value| self.tree.kind(value))
        };
        let rest = match statements.as_slice() {
            [first, rest @ ..] if value(*first) == Some(SyntaxKind::StringLiteral) => rest,
            all => all,
        };
        matches!(rest, [only] if value(*only) == Some(SyntaxKind::Ellipsis))
    }

    /// Checks the struct or error type `entry`: its embedded fields and the traits it names.
    fn struct_class(&self, entry: &Entry, diagnostics: &mut Vec<Diagnostic>) {
        let Kind::Class(class) = &entry.kind else {
            return;
        };
        let arguments = class
            .type_parameters
            .iter()
            .map(|&name| {
                Type::new(
                    self.db,
                    TypeKind::Parameter {
                        file: self.file,
                        name,
                    },
                )
            })
            .collect();
        let nominal = Nominal {
            file: self.file,
            name: entry.name,
            arguments,
        };
        if self.embeds_itself(entry, &nominal, diagnostics) {
            return;
        }
        self.ambiguous_names(entry, &nominal, diagnostics);
        self.conformance(entry, &nominal, diagnostics);
    }

    /// Reports a struct that embeds itself through other structs, and returns whether it does,
    /// since its names then have no end to search.
    fn embeds_itself(
        &self,
        entry: &Entry,
        nominal: &Nominal<'db>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> bool {
        let mut pending: Vec<(Nominal<'db>, Vec<String>)> =
            vec![(nominal.clone(), vec![self.text(entry.name).to_owned()])];
        let mut visited: Vec<(File, NodeId)> = Vec::new();
        while let Some((current, chain)) = pending.pop() {
            for (_, inner) in embed::embedded(self.db, self.package, &current) {
                let name = self.class_name(&inner);
                if (inner.file, inner.name) == (nominal.file, nominal.name) {
                    let written = self.text(entry.name);
                    diagnostics.push(
                        Diagnostic::error(format!("`{written}` embeds itself"))
                            .with_primary(self.tree.span(entry.name), "")
                            .with_note(format!(
                                "through {} and back: a struct that held itself would have no end",
                                chain.join(" embeds ")
                            )),
                    );
                    return true;
                }
                if !visited.contains(&(inner.file, inner.name)) {
                    visited.push((inner.file, inner.name));
                    let mut longer = chain.clone();
                    longer.push(name);
                    pending.push((inner, longer));
                }
            }
        }
        false
    }

    /// Reports each name that two embedded structs provide at the same depth and no shallower
    /// struct hides.
    fn ambiguous_names(
        &self,
        entry: &Entry,
        nominal: &Nominal<'db>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let mut hidden: Vec<String> = embed::own_names(self.db, self.package, nominal);
        let mut level: Vec<(String, Nominal<'db>)> =
            self.embedded_fields(nominal).into_iter().collect();
        while !level.is_empty() {
            let mut providers: BTreeMap<String, Vec<String>> = BTreeMap::new();
            let mut next = Vec::new();
            for (path, current) in &level {
                for name in embed::own_names(self.db, self.package, current) {
                    providers.entry(name).or_default().push(path.clone());
                }
                for (field, inner) in self.embedded_fields(current) {
                    next.push((format!("{path}.{field}"), inner));
                }
            }
            for (name, paths) in &providers {
                if paths.len() > 1 && !hidden.contains(name) {
                    let written = self.text(entry.name);
                    let reached: Vec<String> = paths
                        .iter()
                        .map(|path| format!("`{path}.{name}`"))
                        .collect();
                    diagnostics.push(
                        Diagnostic::error(format!(
                            "`{written}.{name}` could mean more than one embedded field"
                        ))
                        .with_primary(self.tree.span(entry.name), "")
                        .with_note(format!("it is {}", reached.join(" or ")))
                        .with_note(format!(
                            "define `{name}` on `{written}` itself, which hides both, or rename \
                             one of them"
                        )),
                    );
                }
            }
            hidden.extend(providers.into_keys());
            level = next;
        }
    }

    /// The embedded fields of the struct `nominal`, each with its name and the embedded type.
    fn embedded_fields(&self, nominal: &Nominal<'db>) -> Vec<(String, Nominal<'db>)> {
        let item = Item::new(self.db, self.package, nominal.file, nominal.name);
        let Declaration::Struct { fields, .. } = &self.db.declaration(item).declaration else {
            return Vec::new();
        };
        embed::embedded(self.db, self.package, nominal)
            .into_iter()
            .filter_map(|(index, inner)| {
                let field = fields.get(usize::try_from(index).ok()?)?;
                Some((field.name.clone(), inner))
            })
            .collect()
    }

    /// The name of the class `nominal` names.
    pub(crate) fn class_name(&self, nominal: &Nominal<'db>) -> String {
        self.db
            .parse(nominal.file)
            .tree()
            .text(nominal.name, nominal.file.source(self.db).text())
            .to_owned()
    }
}
