//! Class patterns and unions in `match` (decision 0032): `case Dirichlet(value=v):` takes a
//! struct apart, and on a union subject each case picks the member it matches.
//!
//! A class pattern names a struct and matches its fields by position, in declaration order, or
//! by keyword, as Python's dataclasses define `__match_args__`. Fields it leaves out match
//! anything. On a union subject, such as `Periodic | Dirichlet | Neumann`, a class pattern
//! matches the member it names, an enum member pattern the enum's member, and `case None:` the
//! `None` member; exhaustiveness then wants a case for each member, and names the first one
//! missing:
//!
//! ```text
//! error: this `match` does not handle `Robin()`
//! ```

use super::Body;
use super::exhaustive::{Pattern, Space};
use crate::embed;
use database::Definition;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Nominal, Type, TypeKind};

impl<'db> Body<'db> {
    /// Checks `node`, a pattern that is not a capture or an `a | b`, against a union subject of
    /// type `union` with the members `members`, and wraps what it matches of its member in a
    /// [`Pattern::Variant`]. A pattern that matches no member is an error.
    pub(super) fn member_pattern(
        &mut self,
        node: NodeId,
        union: Type<'db>,
        members: &[Type<'db>],
    ) -> Pattern {
        let member = match self.tree.kind(node) {
            SyntaxKind::PatternClass => {
                let Some(class) = self.pattern_class(node) else {
                    self.assume_captured(node);
                    return Pattern::Wild;
                };
                let position = members.iter().position(|&member| {
                    matches!(member.kind(self.db), TypeKind::Struct(nominal)
                        if (nominal.file, nominal.name) == class)
                });
                let Some(position) = position else {
                    self.not_a_member(node, union);
                    return Pattern::Wild;
                };
                (position, self.class_pattern(node, members[position]))
            }
            SyntaxKind::PatternSingleton if self.tree.text(node, self.text) == "None" => {
                let position = members
                    .iter()
                    .position(|member| matches!(member.kind(self.db), TypeKind::None));
                let Some(position) = position else {
                    self.not_a_member(node, union);
                    return Pattern::Wild;
                };
                (position, Pattern::Wild)
            }
            SyntaxKind::PatternValue => {
                let Some(value) = self.tree.child(node, Role::Value) else {
                    return Pattern::Wild;
                };
                let (pattern, found) = self.value_pattern(value, union);
                if pattern == Pattern::Wild {
                    return pattern;
                }
                let Some(position) = members.iter().position(|&member| member == found) else {
                    return Pattern::Wild;
                };
                self.record_member(node, &pattern);
                (position, pattern)
            }
            _ => return self.plain_pattern(node, union),
        };
        let (position, inner) = member;
        Pattern::Variant(position, Box::new(inner))
    }

    /// Checks the class pattern `node` against a subject of type `ty`, binds its captures,
    /// and reduces it to one pattern per field for exhaustiveness.
    pub(super) fn class_pattern(&mut self, node: NodeId, ty: Type<'db>) -> Pattern {
        let Some(class) = self.pattern_class(node) else {
            self.assume_captured(node);
            return Pattern::Wild;
        };
        let nominal = match ty.kind(self.db) {
            TypeKind::Struct(nominal) if (nominal.file, nominal.name) == class => nominal.clone(),
            TypeKind::Unknown => {
                self.assume_captured(node);
                return Pattern::Wild;
            }
            _ => {
                let written = self.class_written(node);
                let message = format!(
                    "a `{written}` pattern cannot match a value of type `{}`",
                    self.show(ty)
                );
                self.lowerer
                    .error(node, &message, "does not match the subject", None);
                self.assume_captured(node);
                return Pattern::Wild;
            }
        };
        let fields = embed::fields(self.db, self.lowerer.package, &nominal);
        let mut matched: Vec<Option<Pattern>> = vec![None; fields.len()];
        let items: Vec<NodeId> = self.tree.children_with(node, Role::Item).collect();
        let mut position = 0;
        for item in items {
            let (index, pattern) = if self.tree.kind(item) == SyntaxKind::PatternKeyword {
                let (Some(name), Some(pattern)) = (
                    self.tree.child(item, Role::Name),
                    self.tree.child(item, Role::Pattern),
                ) else {
                    continue;
                };
                let wanted = self.tree.text(name, self.text);
                let Some(index) = fields.iter().position(|(field, _)| field == wanted) else {
                    let message = format!("`{}` has no field `{wanted}`", self.show(ty));
                    self.lowerer
                        .error(name, &message, "not a field of this struct", None);
                    self.assume_captured(item);
                    continue;
                };
                (index, pattern)
            } else {
                let index = position;
                position += 1;
                if index >= fields.len() {
                    let message = format!(
                        "`{}` has {} field(s), so a class pattern takes at most {} by position",
                        self.show(ty),
                        fields.len(),
                        fields.len()
                    );
                    self.lowerer
                        .error(item, &message, "no field at this position", None);
                    self.assume_captured(item);
                    continue;
                }
                (index, item)
            };
            if matched[index].is_some() {
                let message = format!("the field `{}` is matched twice", fields[index].0);
                self.lowerer.error(
                    item,
                    &message,
                    "matched again here",
                    Some("each field is matched once, by position or by keyword"),
                );
                self.assume_captured(item);
                continue;
            }
            matched[index] = Some(self.pattern(pattern, fields[index].1));
        }
        Pattern::Class(
            matched
                .into_iter()
                .map(|pattern| pattern.unwrap_or(Pattern::Wild))
                .collect(),
        )
    }

    /// The class a class pattern names, as the file and `Name` node that declare it, or `None`
    /// after reporting that it names no struct.
    fn pattern_class(&mut self, node: NodeId) -> Option<(database::File, NodeId)> {
        let class = self.tree.child(node, Role::Class)?;
        match self.lowerer.definition(class) {
            Some(&Definition::Item(file, name)) if self.is_struct(file, name) => Some((file, name)),
            None => None,
            Some(_) => {
                let written = self.tree.text(class, self.text);
                let message = format!("`{written}` is not a struct");
                self.lowerer.error(
                    class,
                    &message,
                    "expected a struct",
                    Some("a class pattern names a struct or an error type, such as `case Dirichlet(value=v):`"),
                );
                None
            }
        }
    }

    /// Whether the definition at `name` in `file` is a struct or an error type.
    fn is_struct(&self, file: database::File, name: NodeId) -> bool {
        let nominal = Nominal {
            file,
            name,
            arguments: Vec::new(),
        };
        let item = types::Item::new(self.db, self.lowerer.package, nominal.file, nominal.name);
        matches!(
            self.db.declaration(item).declaration,
            types::Declaration::Struct { .. }
        )
    }

    /// The class name a class pattern writes, such as `Dirichlet`.
    fn class_written(&self, node: NodeId) -> &'db str {
        self.tree
            .child(node, Role::Class)
            .map_or("", |class| self.tree.text(class, self.text))
    }

    /// Reports a pattern that matches no member of the union `union`.
    fn not_a_member(&mut self, node: NodeId, union: Type<'db>) {
        let message = format!("this pattern matches no member of `{}`", self.show(union));
        self.lowerer.error(
            node,
            &message,
            "not a member of the union",
            Some("a case on a union matches one of its members, or `_` for the rest"),
        );
        self.assume_captured(node);
    }

    /// What the values of a union with the members `members` can look like: one space per
    /// member, `None` as a nameless struct with no fields. `open` holds the structs being
    /// taken apart.
    pub(super) fn union_space(&self, members: &[Type<'db>], open: &mut Vec<Type<'db>>) -> Space {
        Space::Union(
            members
                .iter()
                .map(|&member| match member.kind(self.db) {
                    TypeKind::None => Space::Struct {
                        name: String::new(),
                        fields: Vec::new(),
                    },
                    _ => self.space_within(member, open),
                })
                .collect(),
        )
    }

    /// What the values of the struct `nominal`, of type `ty`, can look like, field by field.
    /// `open` holds the structs being taken apart, `ty` among them.
    pub(super) fn struct_space(
        &self,
        ty: Type<'db>,
        nominal: &Nominal<'db>,
        open: &mut Vec<Type<'db>>,
    ) -> Space {
        Space::Struct {
            name: self.show(ty),
            fields: embed::fields(self.db, self.lowerer.package, nominal)
                .into_iter()
                .map(|(_, field)| self.space_within(field, open))
                .collect(),
        }
    }
}
