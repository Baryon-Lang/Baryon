//! `match` (language reference, "Syntax"): the cases are tried in order, and the first whose
//! pattern matches, and whose guard holds, runs its body.
//!
//! Each case tests its pattern against the subject with branches that go to the next case on
//! the first part that does not match: a [`Switch`](mir::TerminatorKind::Switch) on an enum
//! member, an `Equal` comparison with a literal, an
//! [`IsPresent`](mir::Rvalue::IsPresent) test for `None`, a switch on the member a union holds
//! (the `unions` module), and the same tests on each item of a tuple or field of a struct. Only when the whole pattern matched are its captures assigned, then the guard is
//! tested. The checker proved that the cases without a guard cover every value, so falling past
//! the last case cannot happen.
//!
//! The first version handles the patterns the checker accepts (literals, enum members, `True`,
//! `False`, `None`, tuples, class patterns, captures, `_` and `a | b`), with two limits: a capture
//! that is not of a copy type must borrow its part of a read-only parameter, which it names in
//! place (decision 0032, roadmap 1.9.50), since moving part of an owned subject out is not built
//! yet; and an `a | b` pattern may not capture.

use crate::builder::Builder;
use database::Definition;
use mir::{BinaryOperator, BlockId, LocalKind, Operand, Place, Projection, Rvalue, TerminatorKind};
use syntax::{NodeId, Role, SyntaxKind};
use types::Mode;
use types::{Type, TypeKind};

/// A capture a matched pattern assigns: the name's node, and the place and type of the part of
/// the subject it names.
pub(crate) type Capture<'db> = (NodeId, Place, Type<'db>);

impl<'db> Builder<'db> {
    /// `match value:` and its cases.
    pub(crate) fn match_statement(&mut self, statement: NodeId) {
        let Some(value) = self.tree.child(statement, Role::Value) else {
            return;
        };
        let ty = self.ty(value);
        let subject = self.borrow(value);
        let cases: Vec<NodeId> = self.tree.children_with(statement, Role::Case).collect();
        let mut end = None;
        for case in cases {
            let next = self.new_block();
            let mut captures = Vec::new();
            if let Some(pattern) = self.tree.child(case, Role::Pattern)
                && !self.test(pattern, subject.clone(), ty, next, &mut captures)
            {
                return;
            }
            // A capture that borrows part of the subject names it only inside its case.
            let aliases = self.aliases.clone();
            for (name, place, ty) in captures {
                if !self.capture(name, place, ty) {
                    return;
                }
            }
            if let Some(guard) = self.tree.child(case, Role::Condition) {
                let condition = self.condition(guard);
                self.then(guard, |then| TerminatorKind::Branch {
                    condition,
                    then,
                    otherwise: next,
                });
            }
            self.block(case, Role::Body);
            self.aliases = aliases;
            self.jump(&mut end, case);
            self.start(next);
        }
        // Past the last case: no case matched, which the checker proved cannot happen.
        self.terminate(TerminatorKind::Unreachable, statement);
        self.join(end);
    }

    /// Tests whether the value at `place`, of type `ty`, matches `pattern`, going to `fail`
    /// at the first part that does not; on a match, control continues in the current block.
    /// Collects the captures to assign onto `captures`. Returns `false` after reporting a
    /// pattern lowering does not handle.
    pub(crate) fn test(
        &mut self,
        pattern: NodeId,
        place: Place,
        ty: Type<'db>,
        fail: BlockId,
        captures: &mut Vec<Capture<'db>>,
    ) -> bool {
        if let Some(members) = self.union_members(ty)
            && !matches!(
                self.tree.kind(pattern),
                SyntaxKind::PatternAs | SyntaxKind::PatternOr
            )
        {
            return self.member_test(pattern, place, members, fail, captures);
        }
        match self.tree.kind(pattern) {
            SyntaxKind::PatternAs => {
                if let Some(inner) = self.tree.child(pattern, Role::Pattern)
                    && !self.test(inner, place.clone(), ty, fail, captures)
                {
                    return false;
                }
                if let Some(name) = self.tree.child(pattern, Role::Name) {
                    captures.push((name, place, ty));
                }
                true
            }
            SyntaxKind::PatternOr => self.alternatives(pattern, &place, ty, fail),
            SyntaxKind::PatternSingleton if self.text_of(pattern) == "None" => {
                if self.optional_value(ty).is_some() {
                    let present = self.is_present(&place, pattern);
                    self.then(pattern, |then| TerminatorKind::Branch {
                        condition: present,
                        then: fail,
                        otherwise: then,
                    });
                }
                true
            }
            SyntaxKind::PatternSingleton => {
                let (place, _) = self.present(place, ty, fail, pattern);
                let condition = Operand::Copy(place);
                let wanted = self.text_of(pattern) == "True";
                self.then(pattern, |then| TerminatorKind::Branch {
                    condition,
                    then: if wanted { then } else { fail },
                    otherwise: if wanted { fail } else { then },
                });
                true
            }
            SyntaxKind::PatternValue => {
                let Some(value) = self.tree.child(pattern, Role::Value) else {
                    return true;
                };
                let (place, ty) = self.present(place, ty, fail, pattern);
                if let Some(member) = self.typed.pattern(pattern) {
                    self.then(pattern, |then| TerminatorKind::Switch {
                        value: Operand::Copy(place),
                        cases: vec![(i128::from(member), then)],
                        otherwise: fail,
                    });
                    return true;
                }
                let literal = self.operand_as(value, ty);
                let equal = self.temporary(self.boolean_type(), pattern);
                self.assign(
                    Place::from(equal),
                    Rvalue::Binary(BinaryOperator::Equal, Operand::Copy(place), literal),
                    pattern,
                );
                self.then(pattern, |then| TerminatorKind::Branch {
                    condition: Operand::Copy(Place::from(equal)),
                    then,
                    otherwise: fail,
                });
                true
            }
            SyntaxKind::PatternSequence => {
                let (place, ty) = self.present(place, ty, fail, pattern);
                let TypeKind::Tuple(types) = ty.kind(self.db).clone() else {
                    self.unsupported(pattern, "sequence patterns on values other than tuples");
                    return false;
                };
                let items: Vec<NodeId> = self.tree.children_with(pattern, Role::Item).collect();
                for ((field, item), ty) in (0..).zip(items).zip(types) {
                    let part = place.clone().project(Projection::Field(field));
                    if !self.test(item, part, ty, fail, captures) {
                        return false;
                    }
                }
                true
            }
            SyntaxKind::PatternClass => {
                let (place, ty) = self.present(place, ty, fail, pattern);
                self.class_test(pattern, &place, ty, fail, captures)
            }
            _ => {
                self.unsupported(pattern, "this pattern");
                false
            }
        }
    }

    /// `a | b`: each alternative is tried in turn, and the first that matches continues. None
    /// may capture, since each would bind its names from a different path.
    fn alternatives(
        &mut self,
        pattern: NodeId,
        place: &Place,
        ty: Type<'db>,
        fail: BlockId,
    ) -> bool {
        let items: Vec<NodeId> = self.tree.children_with(pattern, Role::Item).collect();
        let matched = self.new_block();
        for (index, item) in items.iter().enumerate() {
            let next = if index + 1 == items.len() {
                fail
            } else {
                self.new_block()
            };
            let mut captures = Vec::new();
            if !self.test(*item, place.clone(), ty, next, &mut captures) {
                return false;
            }
            if !captures.is_empty() {
                self.unsupported(pattern, "`|` patterns that capture names");
                return false;
            }
            self.terminate(TerminatorKind::Goto(matched), *item);
            if next != fail {
                self.start(next);
            }
        }
        self.start(matched);
        true
    }

    /// The value inside `place` when `ty` is `T | None`: a test that goes to `fail` when it
    /// is `None`, then the `T`. Any other type is returned as it is.
    fn present(
        &mut self,
        place: Place,
        ty: Type<'db>,
        fail: BlockId,
        node: NodeId,
    ) -> (Place, Type<'db>) {
        let Some(value) = self.optional_value(ty) else {
            return (place, ty);
        };
        let present = self.is_present(&place, node);
        self.then(node, |then| TerminatorKind::Branch {
            condition: present,
            then,
            otherwise: fail,
        });
        (place.project(Projection::Value), value)
    }

    /// Whether the `T | None` at `place` holds a `T`, as a `bool` operand.
    fn is_present(&mut self, place: &Place, node: NodeId) -> Operand<'db> {
        let present = self.temporary(self.boolean_type(), node);
        self.assign(Place::from(present), Rvalue::IsPresent(place.clone()), node);
        Operand::Copy(Place::from(present))
    }

    /// Assigns the capture `name` the value at `place`, of type `ty`. After `case None:`, a
    /// capture of a `T | None` subject has the type `T`, and reads the value inside. Returns
    /// `false` after reporting a capture lowering does not handle.
    fn capture(&mut self, name: NodeId, place: Place, ty: Type<'db>) -> bool {
        let Some(variable) = self.target_variable(name) else {
            return true;
        };
        let wanted = self.local_type(variable);
        let place = if wanted != ty && self.optional_value(ty) == Some(wanted) {
            place.project(Projection::Value)
        } else {
            place
        };
        if !self.is_copy(wanted) {
            // A part of a read-only parameter is borrowed in place, as a loop variable borrows
            // its element (decision 0032, roadmap 1.9.50): the parameter's owner frees it.
            let borrowed = matches!(
                self.body.locals[place.local.index()].kind,
                LocalKind::Parameter {
                    mode: Mode::Borrow,
                    ..
                }
            );
            if !borrowed {
                self.unsupported(name, "moving a captured value out of its subject");
                return false;
            }
            if let Some(&Definition::Local(binding)) = self.definition(name) {
                self.aliases.insert(binding, (place, wanted));
            }
            return true;
        }
        self.assign(
            Place::from(variable),
            Rvalue::Use(Operand::Copy(place)),
            name,
        );
        true
    }
}
