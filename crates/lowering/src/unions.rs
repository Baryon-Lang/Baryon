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
use mir::{Argument, BlockId, Operand, Place, Projection, Rvalue, TerminatorKind};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Item, Type, TypeKind};

impl<'db> Builder<'db> {
    /// Ends the block with the call of `item` at `type_arguments`, with `call`'s arguments,
    /// destination, target and error edge. A method called on a union calls instead the method
    /// of the member it holds (decision 0032), one call per member behind a switch. `false`
    /// when the call cannot be built, which has been reported.
    pub(crate) fn call_terminator(
        &mut self,
        item: Item<'db>,
        type_arguments: Vec<Type<'db>>,
        arguments: &[Option<NodeId>],
        call: (Vec<Argument<'db>>, Place, BlockId, Option<mir::ErrorEdge>),
        node: NodeId,
    ) -> bool {
        // A method's receiver comes first; a function's first argument is not one.
        let is_method = matches!(
            crate::definition(self.db.parse(item.file(self.db)).tree(), item.name(self.db)),
            Some((_, Some(_)))
        );
        let receiver = arguments.first().copied().flatten().filter(|_| is_method);
        if let Some(members) = receiver.and_then(|receiver| self.union_members(self.ty(receiver))) {
            return self.member_calls(item, members, call, node);
        }
        let (arguments, destination, target, error) = call;
        self.terminate(
            TerminatorKind::Call {
                callee: mir::Callee::Function {
                    item,
                    type_arguments,
                },
                arguments,
                destination,
                target,
                error,
            },
            node,
        );
        true
    }

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

    /// Ends the current block with a call of the method `item`, the first member's method of
    /// its name, on a union receiver with the members `members`: a switch on the member the
    /// union holds, then in each branch a call of that member's own method of the same name,
    /// the receiver read as the member. `call` is the arguments, receiver first, the result's
    /// place, the block every call returns to, and the error edge they share. Returns `false`
    /// after reporting what lowering does not handle.
    pub(crate) fn member_calls(
        &mut self,
        item: Item<'db>,
        members: &[Type<'db>],
        call: (Vec<Argument<'db>>, Place, BlockId, Option<mir::ErrorEdge>),
        node: NodeId,
    ) -> bool {
        let (arguments, destination, target, error) = call;
        let Some((receiver, rest)) = arguments.split_first() else {
            return false;
        };
        let Some(base) = argument_place(receiver) else {
            self.unsupported(node, "a method call on a union that is not in a variable");
            return false;
        };
        let tree = self.db.parse(item.file(self.db)).tree();
        let text = item.file(self.db).source(self.db).text();
        let name = tree.text(item.name(self.db), text);
        let mut methods = Vec::with_capacity(members.len());
        for &member in members {
            let Some(method) = self.own_method(member, name) else {
                self.unsupported(node, "a method a union member gets from an embedded field");
                return false;
            };
            methods.push(method);
        }
        let tag = self.temporary(self.int_type(), node);
        self.assign(Place::from(tag), Rvalue::MemberIndex(base.clone()), node);
        let blocks: Vec<BlockId> = methods.iter().map(|_| self.new_block()).collect();
        let cases = (0..).zip(blocks.iter().copied()).collect();
        let Some(&first) = blocks.first() else {
            return false;
        };
        self.terminate(
            TerminatorKind::Switch {
                value: Operand::Copy(Place::from(tag)),
                cases,
                otherwise: first,
            },
            node,
        );
        for ((position, method), block) in (0_u32..).zip(methods).zip(blocks) {
            self.start(block);
            let member = base.clone().project(Projection::Member(position));
            let mut passed = vec![with_place(receiver, member)];
            passed.extend_from_slice(rest);
            self.terminate(
                TerminatorKind::Call {
                    callee: mir::Callee::Function {
                        item: method,
                        type_arguments: Vec::new(),
                    },
                    arguments: passed,
                    destination: destination.clone(),
                    target,
                    error: error.clone(),
                },
                node,
            );
        }
        true
    }

    /// The method `name` the struct `member` declares itself.
    fn own_method(&self, member: Type<'db>, name: &str) -> Option<Item<'db>> {
        let TypeKind::Struct(nominal) = member.kind(self.db) else {
            return None;
        };
        let item = Item::new(self.db, self.package, nominal.file, nominal.name);
        let types::Declaration::Struct { methods, .. } = &self.db.declaration(item).declaration
        else {
            return None;
        };
        let tree = self.db.parse(nominal.file).tree();
        let text = nominal.file.source(self.db).text();
        methods
            .iter()
            .copied()
            .find(|method| tree.text(method.name(self.db), text) == name)
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

/// The place an argument names, or `None` for a constant.
fn argument_place(argument: &Argument<'_>) -> Option<Place> {
    match argument {
        Argument::Borrow(place)
        | Argument::InOut(place)
        | Argument::Owned(Operand::Copy(place) | Operand::Move(place)) => Some(place.clone()),
        Argument::Owned(Operand::Constant(_)) => None,
    }
}

/// `argument` passed in the same way, but naming `place`.
fn with_place<'db>(argument: &Argument<'db>, place: Place) -> Argument<'db> {
    match argument {
        Argument::Borrow(_) => Argument::Borrow(place),
        Argument::InOut(_) => Argument::InOut(place),
        Argument::Owned(Operand::Move(_)) => Argument::Owned(Operand::Move(place)),
        Argument::Owned(_) => Argument::Owned(Operand::Copy(place)),
    }
}
