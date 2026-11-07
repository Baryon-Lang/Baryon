//! Arrays (language reference, "Arrays"): indexing with bounds checks, whole-array arithmetic as
//! one [`Elementwise`] tree, `a.shape`, `a.size` and `len(a)`, and creating arrays with `zeros`,
//! `ones` and `fill`.

use crate::builder::Builder;
use crate::expressions::arithmetic;
use mir::{
    BinaryOperator, Check, Constant, ConstantValue, Elementwise, Local, Operand, Place, Projection,
    Rvalue, TerminatorKind, UnaryOperator,
};
use syntax::{Flags, NodeId, Operator, Role, SyntaxKind};
use types::{Integer, Primitive, Shape, Type, TypeKind};

/// How the lengths of an array's dimensions are known.
enum Lengths<'a> {
    /// An `Array`: read at run time from the array.
    Runtime,
    /// A `Fixed` array: from its type.
    Fixed(&'a Shape),
}

impl<'db> Builder<'db> {
    /// The element `a[i, j]` names, after a bounds check of each index unless the program wrote
    /// `a.unchecked[i, j]` (decision 0010). A negative literal counts from the end. Indexing a
    /// tuple with a constant names the item.
    pub(crate) fn subscript(&mut self, node: NodeId) -> Option<Place> {
        let (value, index) = (
            self.tree.child(node, Role::Value)?,
            self.tree.child(node, Role::Index)?,
        );
        let indices: Vec<NodeId> = if self.tree.kind(index) == SyntaxKind::Tuple
            && !self.tree.node(index).flags().contains(Flags::PARENTHESIZED)
        {
            self.tree.children_with(index, Role::Item).collect()
        } else {
            vec![index]
        };
        if indices
            .iter()
            .any(|&index| self.tree.kind(index) == SyntaxKind::Slice)
        {
            self.unsupported(node, "slices");
            return None;
        }
        let (array, unchecked) = match self.unchecked_base(value) {
            Some(array) => (array, true),
            None => (value, false),
        };
        let base_type = self.ty(array);
        match base_type.kind(self.db) {
            TypeKind::Tuple(_) => {
                let Some(position) = self.constant_integer(index) else {
                    self.unsupported(node, "tuple indices that are not constants");
                    return None;
                };
                let base = self.borrow(value);
                Some(base.project(Projection::Field(u32::try_from(position).ok()?)))
            }
            TypeKind::Array { .. } | TypeKind::Fixed { .. } => {
                let base = self.borrow(array);
                let mut locals = Vec::with_capacity(indices.len());
                for (dimension, index) in indices.into_iter().enumerate() {
                    let dimension = u32::try_from(dimension).ok()?;
                    locals.push(self.index(&base, base_type, dimension, index, unchecked));
                }
                Some(base.project(Projection::Index(locals)))
            }
            _ => {
                self.unsupported(node, "indexing lists and strings");
                None
            }
        }
    }

    /// The array `a` in `a.unchecked`, when `value` is that attribute.
    fn unchecked_base(&self, value: NodeId) -> Option<NodeId> {
        let attribute = self.tree.child(value, Role::Attribute)?;
        (self.tree.kind(value) == SyntaxKind::Attribute && self.text_of(attribute) == "unchecked")
            .then(|| self.tree.child(value, Role::Value))
            .flatten()
    }

    /// The `i64` local holding `index`, the index into dimension `dimension` of the array at
    /// `array` of type `ty`, checked to be in range unless `unchecked`.
    fn index(
        &mut self,
        array: &Place,
        ty: Type<'db>,
        dimension: u32,
        index: NodeId,
        unchecked: bool,
    ) -> Local {
        let int = self.int_type();
        let from_end = self.negative_literal(index);
        // An unchecked index that counts from the start needs no length.
        let length =
            (!unchecked || from_end.is_some()).then(|| self.length(array, ty, dimension, index));
        let local = self.temporary(int, index);
        let value = match (from_end, &length) {
            (Some(from_end), Some(Operand::Constant(constant))) => {
                let ConstantValue::Integer(length) = constant.value else {
                    return local;
                };
                Rvalue::Use(Operand::Constant(Constant::integer(int, length - from_end)))
            }
            (Some(from_end), Some(length)) => Rvalue::Binary(
                BinaryOperator::Subtract,
                length.clone(),
                Operand::Constant(Constant::integer(int, from_end)),
            ),
            _ => Rvalue::Use(self.operand_as(index, int)),
        };
        self.assign(Place::from(local), value, index);
        let Some(length) = length.filter(|_| !unchecked) else {
            return local;
        };
        // An index known when compiling, into a `Fixed` array, was checked by the checker.
        if matches!(length, Operand::Constant(_))
            && (from_end.is_some() || self.constant_integer(index).is_some())
        {
            return local;
        }
        let boolean = self.boolean_type();
        let (above, below, inside) = (
            self.temporary(boolean, index),
            self.temporary(boolean, index),
            self.temporary(boolean, index),
        );
        let zero = Operand::Constant(Constant::integer(int, 0));
        let copy = Operand::Copy(Place::from(local));
        self.assign(
            Place::from(above),
            Rvalue::Binary(BinaryOperator::GreaterEqual, copy.clone(), zero),
            index,
        );
        self.assign(
            Place::from(below),
            Rvalue::Binary(BinaryOperator::Less, copy.clone(), length.clone()),
            index,
        );
        self.assign(
            Place::from(inside),
            Rvalue::Binary(
                BinaryOperator::BitAnd,
                Operand::Copy(Place::from(above)),
                Operand::Copy(Place::from(below)),
            ),
            index,
        );
        self.then(index, |target| TerminatorKind::Assert {
            condition: Operand::Copy(Place::from(inside)),
            check: Check::IndexInBounds {
                dimension,
                index: copy,
                length,
            },
            target,
        });
        local
    }

    /// The length of dimension `dimension` of the array at `array` of type `ty`, as an `i64`
    /// operand: a constant for a `Fixed` array, read into a temporary for an `Array`.
    fn length(
        &mut self,
        array: &Place,
        ty: Type<'db>,
        dimension: u32,
        node: NodeId,
    ) -> Operand<'db> {
        if let Some(Lengths::Fixed(shape)) = self.lengths(ty) {
            let length = shape.lengths()[dimension as usize];
            return Operand::Constant(Constant::integer(self.int_type(), i128::from(length)));
        }
        let local = self.temporary(self.int_type(), node);
        self.assign(
            Place::from(local),
            Rvalue::Length(array.clone(), dimension),
            node,
        );
        Operand::Copy(Place::from(local))
    }

    /// How the lengths of an array of type `ty` are known; `None` when it is not an array.
    fn lengths(&self, ty: Type<'db>) -> Option<Lengths<'db>> {
        match ty.kind(self.db) {
            TypeKind::Array { .. } => Some(Lengths::Runtime),
            TypeKind::Fixed { shape, .. } => Some(Lengths::Fixed(shape)),
            _ => None,
        }
    }

    /// The `k` of a negative integer literal `-k`.
    fn negative_literal(&self, node: NodeId) -> Option<i128> {
        let negated = self.tree.kind(node) == SyntaxKind::UnaryOperation
            && self.tree.node(node).operator() == Operator::Negate;
        let value = self.tree.child(node, Role::Value)?;
        (negated && self.tree.kind(value) == SyntaxKind::Integer)
            .then(|| self.constant_integer(value))
            .flatten()
    }

    /// The number of dimensions of an array type, or 0 for anything else.
    fn rank(&self, ty: Type<'db>) -> u32 {
        match ty.kind(self.db) {
            TypeKind::Array { rank, .. } => *rank,
            TypeKind::Fixed { shape, .. } => u32::try_from(shape.rank()).unwrap_or(0),
            _ => 0,
        }
    }

    /// The element type of an array type, or the type itself for a scalar.
    pub(crate) fn element(&self, ty: Type<'db>) -> Type<'db> {
        match ty.kind(self.db) {
            TypeKind::Array { element, .. } | TypeKind::Fixed { element, .. } => *element,
            _ => ty,
        }
    }

    /// Whether `ty` is an `Array` or a `Fixed` array.
    pub(crate) fn is_array(&self, ty: Type<'db>) -> bool {
        self.lengths(ty).is_some()
    }

    /// A whole-array expression, `a + b * k`, into `place` as one [`Elementwise`] tree, which
    /// array expansion turns into a loop nest after fusion has seen it (decision 0018).
    pub(crate) fn elementwise(&mut self, place: Place, node: NodeId) {
        let element = self.element(self.ty(node));
        if let Some(tree) = self.tree_of(node, element) {
            self.assign(place, Rvalue::Elementwise(Box::new(tree)), node);
        }
    }

    /// The [`Elementwise`] tree of `node`: operators whose result is an array become nodes,
    /// arrays and scalars leaves. A scalar converts to the arrays' element type, `element`.
    fn tree_of(&mut self, node: NodeId, element: Type<'db>) -> Option<Elementwise<'db>> {
        let ty = self.ty(node);
        let child = |role| self.tree.child(node, role);
        match self.tree.kind(node) {
            SyntaxKind::BinaryOperation if self.is_array(ty) => {
                let syntax = self.tree.node(node).operator();
                let Some(operator) = arithmetic(syntax) else {
                    self.unsupported(node, &format!("`{}` on arrays", syntax.symbol()));
                    return None;
                };
                let (left, right) = (child(Role::Left)?, child(Role::Right)?);
                let left = self.tree_of(left, element)?;
                let right = self.tree_of(right, element)?;
                Some(Elementwise::Binary(
                    operator,
                    Box::new(left),
                    Box::new(right),
                ))
            }
            SyntaxKind::UnaryOperation
                if self.is_array(ty) && self.tree.node(node).operator() == Operator::Negate =>
            {
                let value = self.tree_of(child(Role::Value)?, element)?;
                Some(Elementwise::Unary(UnaryOperator::Negate, Box::new(value)))
            }
            _ if self.is_array(ty) => {
                if matches!(
                    self.tree.kind(node),
                    SyntaxKind::Name | SyntaxKind::Attribute | SyntaxKind::Subscript
                ) && let Some(place) = self.place(node)
                {
                    return Some(Elementwise::Leaf(Operand::Copy(place)));
                }
                Some(Elementwise::Leaf(self.operand(node)))
            }
            _ => Some(Elementwise::Leaf(self.operand_as(node, element))),
        }
    }

    /// `target op= value` on an array, as `target = target op value` over every element.
    pub(crate) fn augmented_array(
        &mut self,
        place: Place,
        ty: Type<'db>,
        operator: BinaryOperator,
        value: NodeId,
        node: NodeId,
    ) {
        let Some(right) = self.tree_of(value, self.element(ty)) else {
            return;
        };
        let tree = Elementwise::Binary(
            operator,
            Box::new(Elementwise::Leaf(Operand::Copy(place.clone()))),
            Box::new(right),
        );
        let temporary = self.temporary(ty, node);
        self.assign(
            Place::from(temporary),
            Rvalue::Elementwise(Box::new(tree)),
            node,
        );
        if self.needs_drop(ty) {
            self.push(mir::StatementKind::Drop(place.clone()), node);
        }
        let moved = self.take(Place::from(temporary), ty);
        self.assign(place, Rvalue::Use(moved), node);
    }

    /// Whether `node` reads `a.shape`, one of its entries `a.shape[k]`, or `a.size` of an
    /// array `a`. None of them is a place: each is computed from the lengths.
    pub(crate) fn is_array_attribute(&self, node: NodeId) -> bool {
        if self.tree.kind(node) == SyntaxKind::Subscript {
            return self.tree.child(node, Role::Value).is_some_and(|value| {
                self.is_array_attribute(value)
                    && self
                        .tree
                        .child(value, Role::Attribute)
                        .is_some_and(|name| self.text_of(name) == "shape")
            });
        }
        self.tree.kind(node) == SyntaxKind::Attribute
            && self
                .tree
                .child(node, Role::Value)
                .is_some_and(|value| self.is_array(self.ty(value)))
            && self
                .tree
                .child(node, Role::Attribute)
                .is_some_and(|name| matches!(self.text_of(name), "shape" | "size"))
    }

    /// `a.shape`, the tuple of the lengths, `a.shape[k]`, one length, or `a.size`, their
    /// product, into `place`.
    pub(crate) fn array_attribute(&mut self, place: Place, node: NodeId) {
        if self.tree.kind(node) == SyntaxKind::Subscript {
            return self.shape_entry(place, node);
        }
        let (Some(value), Some(name)) = (
            self.tree.child(node, Role::Value),
            self.tree.child(node, Role::Attribute),
        ) else {
            return;
        };
        let ty = self.ty(value);
        let array = self.borrow(value);
        let lengths: Vec<Operand<'db>> = (0..self.rank(ty))
            .map(|dimension| self.length(&array, ty, dimension, node))
            .collect();
        if self.text_of(name) == "shape" {
            return self.assign(place, Rvalue::Aggregate(self.ty(node), lengths), node);
        }
        let mut lengths = lengths.into_iter();
        let Some(first) = lengths.next() else {
            return;
        };
        self.assign(place.clone(), Rvalue::Use(first), node);
        for length in lengths {
            self.assign(
                place.clone(),
                Rvalue::Binary(
                    BinaryOperator::Multiply,
                    Operand::Copy(place.clone()),
                    length,
                ),
                node,
            );
        }
    }

    /// `a.shape[k]`, the length of dimension `k`, into `place`. The checker requires `k` to be
    /// a constant in range; a negative one counts from the end.
    fn shape_entry(&mut self, place: Place, node: NodeId) {
        let (Some(shape), Some(index)) = (
            self.tree.child(node, Role::Value),
            self.tree.child(node, Role::Index),
        ) else {
            return;
        };
        let Some(value) = self.tree.child(shape, Role::Value) else {
            return;
        };
        let ty = self.ty(value);
        let rank = i128::from(self.rank(ty));
        let dimension = match (self.negative_literal(index), self.constant_integer(index)) {
            (Some(from_end), _) => rank - from_end,
            (None, Some(dimension)) => dimension,
            (None, None) => {
                return self.unsupported(node, "`shape` indices that are not constants");
            }
        };
        let Ok(dimension) = u32::try_from(dimension) else {
            return;
        };
        let array = self.borrow(value);
        let length = self.length(&array, ty, dimension, node);
        self.assign(place, Rvalue::Use(length), node);
    }

    /// `len(a)`: the length of an array's first dimension, into `place`.
    pub(crate) fn len(&mut self, place: Place, argument: NodeId, node: NodeId) {
        let ty = self.ty(argument);
        if !self.is_array(ty) {
            return self.unsupported(node, "`len` of lists and strings");
        }
        let array = self.borrow(argument);
        let length = self.length(&array, ty, 0, node);
        self.assign(place, Rvalue::Use(length), node);
    }

    /// The `i64` type, of indices and lengths.
    pub(crate) fn int_type(&self) -> Type<'db> {
        Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::I64)),
        )
    }
}
