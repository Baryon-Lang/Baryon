//! Unions of structs (decision 0032): a member's value made into its union, and `match` cases
//! that pick a member and take a struct apart.
//!
//! A union value is a tag and its member (`types::layout`). A member becomes the union through
//! an [`Aggregate`](mir::Rvalue::Aggregate) of the union type around the member's value. A case
//! on a union subject reads the tag with [`MemberIndex`](mir::Rvalue::MemberIndex), switches to
//! the next case unless it is the member the pattern names, and tests the rest of the pattern
//! on the member, read with the [`Member`](mir::Projection::Member) step. A class pattern tests
//! each field it names, by position or by keyword, with the same tests as any other part.
//!
//! The first version leaves out what [`patterns`](crate::patterns) leaves out (captures of
//! values that are not copy types, captures inside `a | b`), and class patterns on generic
//! structs, whose field types would need the struct's type arguments.

use crate::builder::Builder;
use crate::patterns::Capture;
use database::Definition;
use mir::{BlockId, Operand, Place, Projection, Rvalue, TerminatorKind};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Type, TypeKind};

impl<'db> Builder<'db> {
    /// The members of `ty` when it is a union.
    pub(crate) fn union_members(&self, ty: Type<'db>) -> Option<&'db [Type<'db>]> {
        match ty.kind(self.db) {
            TypeKind::Union(members) => Some(members),
            _ => None,
        }
    }

    /// Tests whether the union at `place`, of type `ty` with the members `members`, holds the
    /// member `pattern` names, going to `fail` when not, then tests the rest of `pattern` on
    /// that member. Returns `false` after reporting what lowering does not handle.
    pub(crate) fn member_test(
        &mut self,
        pattern: NodeId,
        place: Place,
        members: &[Type<'db>],
        fail: BlockId,
        captures: &mut Vec<Capture<'db>>,
    ) -> bool {
        let Some(position) = self.member_of(pattern, members) else {
            self.unsupported(pattern, "this pattern on a union");
            return false;
        };
        let tag = self.temporary(self.int_type(), pattern);
        self.assign(
            Place::from(tag),
            Rvalue::MemberIndex(place.clone()),
            pattern,
        );
        let Ok(index) = u32::try_from(position) else {
            return false;
        };
        self.then(pattern, |then| TerminatorKind::Switch {
            value: Operand::Copy(Place::from(tag)),
            cases: vec![(i128::from(index), then)],
            otherwise: fail,
        });
        let member = members[position];
        if matches!(member.kind(self.db), TypeKind::None) {
            return true;
        }
        let inner = place.project(Projection::Member(index));
        self.test(pattern, inner, member, fail, captures)
    }

    /// The position in `members` of the member `pattern` matches: the struct a class pattern
    /// names, `None` for `case None:`, or the enum of an enum member.
    fn member_of(&self, pattern: NodeId, members: &[Type<'db>]) -> Option<usize> {
        match self.tree.kind(pattern) {
            SyntaxKind::PatternClass => {
                let class = self.pattern_class(pattern)?;
                members.iter().position(|&member| {
                    matches!(member.kind(self.db), TypeKind::Struct(nominal)
                        if (nominal.file, nominal.name) == class)
                })
            }
            SyntaxKind::PatternSingleton if self.text_of(pattern) == "None" => members
                .iter()
                .position(|member| matches!(member.kind(self.db), TypeKind::None)),
            SyntaxKind::PatternValue => {
                let value = self.tree.child(pattern, Role::Value)?;
                let ty = self.ty(value);
                members.iter().position(|&member| member == ty)
            }
            _ => None,
        }
    }

    /// The class a class pattern names, as the file and `Name` node that declare it.
    fn pattern_class(&self, pattern: NodeId) -> Option<(database::File, NodeId)> {
        let class = self.tree.child(pattern, Role::Class)?;
        let name = match self.tree.kind(class) {
            SyntaxKind::Attribute => self.tree.child(class, Role::Attribute)?,
            _ => class,
        };
        match self.definition(name) {
            Some(&Definition::Item(file, name)) => Some((file, name)),
            _ => None,
        }
    }

    /// Tests the fields a class pattern names on the struct at `place`, of type `ty`, each with
    /// the test of its own pattern. The checker proved the pattern names `ty`'s class.
    pub(crate) fn class_test(
        &mut self,
        pattern: NodeId,
        place: &Place,
        ty: Type<'db>,
        fail: BlockId,
        captures: &mut Vec<Capture<'db>>,
    ) -> bool {
        let TypeKind::Struct(nominal) = ty.kind(self.db) else {
            self.unsupported(pattern, "class patterns on values other than structs");
            return false;
        };
        if !nominal.arguments.is_empty() {
            self.unsupported(pattern, "class patterns on generic structs");
            return false;
        }
        let Some(fields) = self.fields((nominal.file, nominal.name)) else {
            return false;
        };
        let items: Vec<NodeId> = self.tree.children_with(pattern, Role::Item).collect();
        let mut position = 0;
        for item in items {
            let (index, sub) = if self.tree.kind(item) == SyntaxKind::PatternKeyword {
                let (Some(name), Some(sub)) = (
                    self.tree.child(item, Role::Name),
                    self.tree.child(item, Role::Pattern),
                ) else {
                    return false;
                };
                let wanted = self.text_of(name);
                let Some(index) = fields.iter().position(|field| field.name == wanted) else {
                    return false;
                };
                (index, sub)
            } else {
                position += 1;
                (position - 1, item)
            };
            let (Some(field), Ok(step)) = (fields.get(index), u32::try_from(index)) else {
                return false;
            };
            let part = place.clone().project(Projection::Field(step));
            if !self.test(sub, part, field.ty, fail, captures) {
                return false;
            }
        }
        true
    }
}
