//! Reading a generic state by position (card 60, an amendment to decision 0065 point 1).
//!
//! Inside a body generic in `S: VectorSpace`, `len(y)` is the number of elements of the state
//! `y` and `y[i]` reads element `i` as an `f64`. An `f64` state has length 1, and `y[0]` is
//! the number itself. An ODE solver needs both: its error norm weighs every element, and its
//! result copies each state into a column of an array. Elements are only read: a new state
//! is built with `+`, `-` and `*`, which keeps every instance a whole-array expression. Any
//! other type parameter cannot be indexed or measured.

use super::Body;
use crate::traits::Trait;
use syntax::{Flags, NodeId, Role, SyntaxKind};
use types::{Float, Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// The type of the subscript `node` of `base` when `base` is a data frame
    /// ([`frame_item`](Self::frame_item)) or a type parameter
    /// ([`state_index`](Self::state_index)); `None` for any other base.
    pub(super) fn frame_or_state_item(
        &mut self,
        node: NodeId,
        base: Type<'db>,
    ) -> Option<Type<'db>> {
        self.frame_item(node, base)
            .or_else(|| self.state_index(node, base))
    }

    /// The type of the subscript `node` of `base` when `base` is a type parameter: `f64` for
    /// `y[i]` under `VectorSpace`, after checking `i` is one integer, and an error for any other
    /// bound or index. `None` when `base` is not a type parameter.
    fn state_index(&mut self, node: NodeId, base: Type<'db>) -> Option<Type<'db>> {
        if !matches!(base.kind(self.db), TypeKind::Parameter { .. }) {
            return None;
        }
        if self.trait_of(base) != Some(Trait::VectorSpace) {
            let message = format!("a value of type `{}` cannot be indexed", self.show(base));
            return Some(self.lowerer.error(
                node,
                &message,
                "not a sequence",
                Some("only a `VectorSpace` type parameter is read by position, as `y[i]`"),
            ));
        }
        let index = self.tree.child(node, Role::Index)?;
        let several = self.tree.kind(index) == SyntaxKind::Tuple
            && !self.tree.node(index).flags().contains(Flags::PARENTHESIZED);
        if several || matches!(self.tree.kind(index), SyntaxKind::Slice | SyntaxKind::None) {
            let message = format!("`{}` takes one index", self.show(base));
            return Some(self.lowerer.error(
                node,
                &message,
                "one integer index",
                Some("a `VectorSpace` state is read one element at a time, as `y[i]`"),
            ));
        }
        self.index(index);
        Some(Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Float(Float::F64)),
        ))
    }

    /// The type of `len(argument)`, where `argument` has type `ty`: an `i64`, after an error
    /// when `ty` is a type parameter whose bound is not `VectorSpace`.
    pub(super) fn length_of(
        &mut self,
        argument: NodeId,
        ty: Type<'db>,
        int: Type<'db>,
    ) -> Type<'db> {
        if matches!(ty.kind(self.db), TypeKind::Parameter { .. })
            && self.trait_of(ty) != Some(Trait::VectorSpace)
        {
            let message = format!("a value of type `{}` has no length", self.show(ty));
            self.lowerer.error(
                argument,
                &message,
                "not a sequence",
                Some("only a `VectorSpace` type parameter has a length, its number of elements"),
            );
        }
        int
    }

    /// Reports an assignment to `target` when it is an element of a `VectorSpace` state,
    /// `y[i] = x`: a state's elements are only read.
    pub(super) fn assigned_state_element(&mut self, target: NodeId) {
        if self.tree.kind(target) != SyntaxKind::Subscript {
            return;
        }
        let Some(value) = self.tree.child(target, Role::Value) else {
            return;
        };
        let state = self
            .typed
            .expressions
            .get(&types::Key(value))
            .is_some_and(|&ty| self.trait_of(ty) == Some(Trait::VectorSpace));
        if state {
            self.lowerer.error(
                target,
                "the elements of a `VectorSpace` state cannot be assigned",
                "assigned here",
                Some("build a new state with `+`, `-` and `*`, as `y + h * k`"),
            );
        }
    }
}
