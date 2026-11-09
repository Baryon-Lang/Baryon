//! Declaring what a class's bases and fields say beyond its kind (decision 0031): the traits a
//! struct names, the traits a trait requires and its methods, and `Embed[T]` fields.
//!
//! Every check here reads only the class's own syntax and what its names resolve to, never
//! another class's declaration, so classes that name each other never wait on each other.
//! Checks that compare classes run later, in the `classes` module.

use crate::lower::Lowerer;
use crate::traits::{UserTrait, user_trait};
use database::Definition;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Declaration, Item, Type, TypeKind};

impl<'db> Lowerer<'db> {
    /// The type of a struct field and whether it is embedded: `Embed[Particle]` is stored as a
    /// `Particle`, whose fields and methods the outer struct reaches directly (decision 0031).
    /// Only a struct can be embedded, and not the struct being declared.
    pub(crate) fn field_type(&mut self, annotation: NodeId) -> (Type<'db>, bool) {
        let embed = self.tree.kind(annotation) == SyntaxKind::Subscript
            && self
                .tree
                .child(annotation, Role::Value)
                .is_some_and(|value| self.definition(value) == Some(&Definition::Builtin("Embed")));
        if !embed {
            return (self.lower(annotation), false);
        }
        let Some(inner) = self.tree.child(annotation, Role::Index) else {
            return (self.unknown(), true);
        };
        let ty = self.lower(inner);
        let shown = ty.display(self.db).to_string();
        match ty.kind(self.db) {
            TypeKind::Struct(_) if ty == self.self_type() => {
                let message = format!("`{shown}` cannot embed itself");
                let ty = self.error(
                    inner,
                    &message,
                    "the struct being declared",
                    Some("a struct that held itself would have no end"),
                );
                (ty, true)
            }
            TypeKind::Struct(_) | TypeKind::Unknown => (ty, true),
            _ => {
                let message = format!("only a struct can be embedded, not `{shown}`");
                let ty = self.error(
                    inner,
                    &message,
                    "not a struct",
                    Some("hold it as a plain field instead: `name: T`"),
                );
                (ty, true)
            }
        }
    }

    /// The traits a struct, error type or enum names among its bases, in order, reporting each
    /// base that is neither a trait nor one of the bases that say what the class is: a class
    /// cannot inherit from another (decisions 0015 and 0031).
    pub(crate) fn named_traits(&mut self, class: NodeId) -> Vec<(NodeId, UserTrait)> {
        let mut traits = Vec::new();
        for base in self.extra_bases(class) {
            if let Some(found) = user_trait(self.db, self.package, self.file, base) {
                traits.push((base, found));
                continue;
            }
            let written = self.tree.text(base, self.text).to_owned();
            let message = format!("a class cannot inherit from `{written}`");
            self.error(
                base,
                &message,
                "not a trait",
                Some(&format!(
                    "there is no class inheritance (decision 0015); to share behavior, declare \
                     `class {written}(Protocol)` with the methods, and to share fields, hold a \
                     `{written}` as a field or embed it: `base: Embed[{written}]` (decision 0031)"
                )),
            );
        }
        traits
    }

    /// The bases of `class` other than `Struct`, `Copy`, `Exception`, `Enum` and `Protocol`
    /// that name something: traits, or names the class may not inherit from. An undefined name
    /// was reported by name resolution, and a base that is not a name by the syntax check.
    fn extra_bases(&self, class: NodeId) -> Vec<NodeId> {
        self.tree
            .child(class, Role::Arguments)
            .into_iter()
            .flat_map(|arguments| self.tree.children_with(arguments, Role::Item))
            .filter(|&base| {
                matches!(
                    self.tree.kind(base),
                    SyntaxKind::Name | SyntaxKind::Attribute
                )
            })
            .filter(|&base| match self.definition(base) {
                None => false,
                Some(Definition::Builtin(name)) => {
                    !["Struct", "Copy", "Exception", "Enum", "Protocol"].contains(name)
                }
                Some(_) => true,
            })
            .collect()
    }

    /// A trait, `class Body(Protocol)` (decision 0031): the traits it requires and its methods.
    /// A trait has methods only, and no type parameters in version 0. Each method's body is
    /// checked to be `...` with the class (the `classes` module).
    pub(crate) fn trait_declaration(&mut self, class: NodeId) -> Declaration<'db> {
        if let Some(parameters) = self.tree.child(class, Role::TypeParameters) {
            self.error(
                parameters,
                "a trait takes no type parameters in version 0",
                "not in version 0",
                Some("decision 0031 leaves traits with type parameters for later"),
            );
        }
        let mut requires = Vec::new();
        for base in self.extra_bases(class) {
            if let Some(found) = user_trait(self.db, self.package, self.file, base) {
                requires.push(found.item(self.db, self.package));
                continue;
            }
            let written = self.tree.text(base, self.text);
            let message = format!("a trait can require only traits, not `{written}`");
            self.error(
                base,
                &message,
                "not a trait",
                Some(
                    "the bases before `Protocol` are the traits a type that implements this \
                     one must also implement",
                ),
            );
        }
        let mut methods = Vec::new();
        let statements: Vec<NodeId> = self.tree.children_with(class, Role::Body).collect();
        for statement in statements {
            match self.tree.kind(statement) {
                SyntaxKind::AnnotatedAssign | SyntaxKind::Assign => {
                    self.error(
                        statement,
                        "a trait has methods only, no fields",
                        "not a method",
                        Some("a type that implements the trait declares its own fields; a method such as `def mass(self) -> f64: ...` can give access to one"),
                    );
                }
                SyntaxKind::FunctionDefinition => {
                    if let Some(name) = self.tree.child(statement, Role::Name) {
                        methods.push(Item::new(self.db, self.package, self.file, name));
                    }
                }
                _ => {}
            }
        }
        Declaration::Trait { requires, methods }
    }
}
