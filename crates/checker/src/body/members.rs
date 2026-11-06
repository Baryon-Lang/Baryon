//! Names after a dot: a module's member, an enum's member, a struct's field or method, and an
//! array's attributes.

use super::Body;
use super::relations;
use crate::outline::{Base, Kind, outline};
use database::Definition;
use syntax::{NodeId, Role};
use types::{Declaration, Item, Nominal, Type, TypeKind};

/// What a name after a dot refers to on a struct.
pub(super) enum Member<'db> {
    /// A field, with the struct's type arguments substituted into its type.
    Field(Type<'db>),
    /// A method, with the class's type parameters and the struct's type arguments, to
    /// substitute into its signature.
    Method {
        /// The method.
        item: Item<'db>,
        /// The `Name` nodes of the class's type parameters.
        parameters: Vec<NodeId>,
        /// The struct's type arguments.
        arguments: Vec<Type<'db>>,
    },
}

impl<'db> Body<'db> {
    /// `value.name`: a module's member, an enum's member, or a struct's field.
    pub(super) fn attribute(&mut self, node: NodeId) -> Type<'db> {
        let (Some(value), Some(attribute)) = (
            self.tree.child(node, Role::Value),
            self.tree.child(node, Role::Attribute),
        ) else {
            return self.lowerer.unknown();
        };
        if self.lowerer.definition(attribute).is_some() {
            return self.name(attribute);
        }
        if let Some(&Definition::Item(file, class)) = self.lowerer.definition(value) {
            return self.class_attribute(node, file, class, attribute);
        }
        let base = self.expression(value, None);
        let name = self.tree.text(attribute, self.text);
        if let Some(ty) = self.array_attribute(node, base, name) {
            return ty;
        }
        let TypeKind::Struct(nominal) = base.kind(self.db) else {
            return self.lowerer.unknown();
        };
        match self.member(node, nominal, base, attribute) {
            Some(Member::Field(ty)) => ty,
            _ => self.lowerer.unknown(),
        }
    }

    /// `Class.name`: an enum member, checked to exist. Anything else on a class, such as
    /// `Vec3.zeros`, is typed by a later slice.
    fn class_attribute(
        &mut self,
        node: NodeId,
        file: database::File,
        class: NodeId,
        attribute: NodeId,
    ) -> Type<'db> {
        let is_enum = matches!(
            outline(self.db, file).get(class).map(|entry| &entry.kind),
            Some(Kind::Class(class)) if class.base == Base::Enum
        );
        if !is_enum {
            return self.lowerer.unknown();
        }
        let item = Item::new(self.db, self.lowerer.package, file, class);
        let Declaration::Enum { variants } = &self.db.declaration(item).declaration else {
            return self.lowerer.unknown();
        };
        let wanted = self.tree.text(attribute, self.text);
        if !variants.iter().any(|variant| variant == wanted) {
            let class_name = self
                .db
                .parse(file)
                .tree()
                .text(class, file.source(self.db).text());
            let message = format!("`{class_name}` has no member `{wanted}`");
            return self.lowerer.error(node, &message, "not a member", None);
        }
        let nominal = Nominal {
            file,
            name: class,
            arguments: Vec::new(),
        };
        Type::new(self.db, TypeKind::Enum(nominal))
    }

    /// The field or method `attribute` names on a struct of type `base`, or `None` after
    /// reporting that it has neither.
    pub(super) fn member(
        &mut self,
        node: NodeId,
        nominal: &Nominal<'db>,
        base: Type<'db>,
        attribute: NodeId,
    ) -> Option<Member<'db>> {
        let wanted = self.tree.text(attribute, self.text);
        let item = Item::new(self.db, self.lowerer.package, nominal.file, nominal.name);
        let parameters = match outline(self.db, nominal.file).get(nominal.name) {
            Some(entry) => match &entry.kind {
                Kind::Class(class) => class.type_parameters.clone(),
                _ => Vec::new(),
            },
            None => Vec::new(),
        };
        let Declaration::Struct {
            fields, methods, ..
        } = &self.db.declaration(item).declaration
        else {
            return None;
        };
        if let Some(field) = fields.iter().find(|field| field.name == wanted) {
            let ty = relations::substitute(
                self.db,
                field.ty,
                nominal.file,
                &parameters,
                &nominal.arguments,
            );
            return Some(Member::Field(ty));
        }
        let class_tree = self.db.parse(nominal.file).tree();
        let class_text = nominal.file.source(self.db).text();
        if let Some(&method) = methods
            .iter()
            .find(|method| class_tree.text(method.name(self.db), class_text) == wanted)
        {
            return Some(Member::Method {
                item: method,
                parameters,
                arguments: nominal.arguments.clone(),
            });
        }
        let message = format!("`{}` has no field or method `{wanted}`", self.show(base));
        self.lowerer
            .error(node, &message, "not defined on this struct", None);
        None
    }
}
