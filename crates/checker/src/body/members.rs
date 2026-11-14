//! Names after a dot: a module's member, an enum's member, a struct's field or method, its own
//! or an embedded struct's (the `embed` module), and an array's attributes.

use super::Body;
use crate::embed::{self, Lookup, Own};
use crate::outline::{Base, Kind, outline};
use database::Definition;
use syntax::{NodeId, Role};
use types::{Declaration, Item, Key, Nominal, Type, TypeKind};

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
    /// `copy`, which every struct has unless it defines its own: a new struct holding a copy
    /// of each field. It is how to keep a struct and also move it (language reference,
    /// "Values and references", "Moves").
    Copy,
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
        if self.maybe_none(value, base) {
            return self.lowerer.unknown();
        }
        let name = self.tree.text(attribute, self.text);
        if self.is_text_array(base) {
            let what = format!("`{name}`");
            return self.text_array_unbuilt(node, &what);
        }
        if self.is_optional_array(base) {
            let what = format!("`{name}`");
            return self.optional_array_unbuilt(node, &what);
        }
        if let Some(ty) = self.array_attribute(node, base, name) {
            return ty;
        }
        if let Some(ty) = self.complex_attribute(node, base, name) {
            return ty;
        }
        let name = name.to_owned();
        if let Some(ty) = self.frame_attribute(node, base, &name) {
            return ty;
        }
        let name = name.as_str();
        if self.is_path(base) {
            return self.path_attribute(node, name);
        }
        if let Some(ty) = self.netcdf_attribute(node, base, name) {
            return ty;
        }
        let TypeKind::Struct(nominal) = base.kind(self.db) else {
            if self.has_no_members(base) {
                return self.no_member(node, base, name, false);
            }
            if matches!(base.kind(self.db), TypeKind::Parameter { .. }) {
                let message = format!("`{}` has no attribute `{name}`", self.show(base));
                return self.lowerer.error(
                    node,
                    &message,
                    "not known for a type parameter",
                    Some("a type parameter has only what its bound allows, and a trait has methods only: call a method such as `body.mass()`"),
                );
            }
            return self.lowerer.unknown();
        };
        match self.member(node, nominal, base, attribute) {
            Some(Member::Field(ty)) => ty,
            _ => self.lowerer.unknown(),
        }
    }

    /// Whether a value of type `ty` has no fields or methods at all in version 0, so any name
    /// after a dot is an error: a `str` (other than `copy`, which the caller handles), a real
    /// number, `bool`, an enum value, a tuple or `None`. A complex number's `real`, `imag` and
    /// `conjugate()` are checked before this (the `complex` module). `Array`, `Fixed`, `list`, structs and the
    /// standard library's types have their own.
    pub(super) fn has_no_members(&self, ty: Type<'db>) -> bool {
        matches!(
            ty.kind(self.db),
            TypeKind::Str
                | TypeKind::Primitive(_)
                | TypeKind::Enum(_)
                | TypeKind::Tuple(_)
                | TypeKind::None
        )
    }

    /// Reports `base.name`, or the call `base.name(...)` when `call`, on a value whose type
    /// has no fields or methods in version 0, such as `"abc".upper()` or `x.real`. Before,
    /// these were left unchecked and failed later as a call lowering does not handle.
    pub(super) fn no_member(
        &mut self,
        node: NodeId,
        base: Type<'db>,
        name: &str,
        call: bool,
    ) -> Type<'db> {
        let what = if call { "method" } else { "attribute" };
        let message = format!("`{}` has no {what} `{name}`", self.show(base));
        let advice = match base.kind(self.db) {
            TypeKind::Str => "a `str` has no attributes, only methods such as `split` and `strip`",
            _ => "version 0 defines no methods or attributes on numbers, `bool`, enums or tuples",
        };
        self.lowerer
            .error(node, &message, "not defined on this type", Some(advice))
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

    /// The field or method `attribute` names on a struct of type `base`, its own or one
    /// reached through embedded fields (decision 0031), or `None` after reporting that it has
    /// neither. A name found through embedded fields records their path for lowering, keyed by
    /// `node`. A name two embedded structs provide at one depth was reported at the class, so
    /// it gives no second error here.
    pub(super) fn member(
        &mut self,
        node: NodeId,
        nominal: &Nominal<'db>,
        base: Type<'db>,
        attribute: NodeId,
    ) -> Option<Member<'db>> {
        let wanted = self.tree.text(attribute, self.text);
        match embed::lookup(self.db, self.lowerer.package, nominal, wanted) {
            Lookup::Found { path, member, .. } => {
                if !path.is_empty() {
                    self.typed.promotions.insert(Key(node), path);
                }
                Some(match member {
                    Own::Field(ty) => Member::Field(ty),
                    Own::Method {
                        item,
                        parameters,
                        arguments,
                    } => Member::Method {
                        item,
                        parameters,
                        arguments,
                    },
                })
            }
            Lookup::Ambiguous(_) => None,
            Lookup::Missing if wanted == "copy" => Some(Member::Copy),
            Lookup::Missing => {
                let message = format!("`{}` has no field or method `{wanted}`", self.show(base));
                self.lowerer
                    .error(node, &message, "not defined on this struct", None);
                None
            }
        }
    }
}
