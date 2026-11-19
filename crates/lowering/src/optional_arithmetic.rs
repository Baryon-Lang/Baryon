//! Arithmetic on arrays with missing values: `bids + asks`, `bids * 2.0`, `bids ** 2.0` and
//! `-bids`, where
//! a side is an `Array[T | None, 1]` (decision 0045, part 3).
//!
//! The result has `None` wherever a side has `None`, so its validity bitmap is the sides'
//! bitmaps with each bit set only where every side's is. The values are computed in one of
//! two ways:
//!
//! - **Floats.** No float operation panics, so the whole expression is one
//!   [`Elementwise`] tree over the stored values, fused into one loop like any array
//!   expression. Whatever it computes under a `None` is never read.
//! - **Integers.** An integer operation panics on overflow and on division by zero, and the
//!   filler under a `None` must not cause a panic the program did not earn. So each array
//!   is first gathered down to the elements the result keeps (the library's `present`), the
//!   tree runs over those, and the library's `spread` puts the results back in their
//!   places. Cost: one more pass and one more array for each array in the expression.
//!
//! A comparison, `bids > 1.0`, gives an `Array[bool | None, 1]` the same way. No comparison
//! panics, so it is always one loop over the stored values of its two sides.
//! A comparison of text is not a loop the backend fuses and is in [`crate::optional_text`].

use crate::builder::Builder;
use crate::operators::{arithmetic, comparison};
use crate::optional_arrays::{VALIDITY, VALUES};
use mir::{BinaryOperator, Elementwise, Local, Operand, Place, Projection, Rvalue, UnaryOperator};
use syntax::{NodeId, Operator, Role, SyntaxKind};
use types::Type;

/// One array an expression reads: where its values are stored, and its element type.
type Values<'db> = (Place, Type<'db>);

/// What an expression on arrays with missing values reads, in the order it is written.
#[derive(Default)]
struct Leaves<'db> {
    /// Every array, with or without missing values.
    arrays: Vec<Values<'db>>,
    /// The validity bitmap of each array with missing values.
    bitmaps: Vec<Place>,
    /// Every operand, an array's values or a single number, for the tree's leaves.
    operands: Vec<Operand<'db>>,
    /// For each operand, its position in `arrays` when it is an array.
    positions: Vec<Option<usize>>,
}

/// The IR operator for an arithmetic operator of the source between arrays with missing
/// values: those of [`arithmetic`] and `**`, which is one operation on each pair of
/// elements (roadmap 1.9.58).
fn gap_operator(operator: Operator) -> Option<BinaryOperator> {
    match operator {
        Operator::Power => Some(BinaryOperator::Power),
        _ => arithmetic(operator),
    }
}

impl<'db> Builder<'db> {
    /// Whether `node` is arithmetic whose result is an array with missing values.
    pub(crate) fn is_optional_arithmetic(&self, node: NodeId) -> bool {
        self.optional_array_element(node).is_some() && self.is_gap_operation(node)
    }

    /// Whether `node` is an operator this module computes element by element: `+`, `-`, `*`,
    /// `/`, `//`, `%` or `**` of two operands, or `-` or `+` of one.
    fn is_gap_operation(&self, node: NodeId) -> bool {
        let operator = || self.tree.node(node).operator();
        match self.tree.kind(node) {
            SyntaxKind::BinaryOperation => matches!(
                operator(),
                Operator::Add
                    | Operator::Subtract
                    | Operator::Multiply
                    | Operator::Divide
                    | Operator::FloorDivide
                    | Operator::Modulo
                    | Operator::Power
            ),
            SyntaxKind::UnaryOperation => matches!(operator(), Operator::Negate | Operator::Plus),
            _ => false,
        }
    }

    /// The arithmetic `node`, whose result is an array with missing values, into `place`.
    /// Every array in it must have the same length, which is checked when the program runs:
    /// arrays with missing values do not broadcast.
    pub(crate) fn optional_arithmetic(&mut self, place: Place, node: NodeId) {
        let Some(element) = self.optional_array_element(node) else {
            return;
        };
        if let Some(whole) = self.optional_arithmetic_value(node, element) {
            self.assign(place, Rvalue::Use(whole), node);
        }
    }

    /// Whether the statement `statement` is an in-place operator on a variable that holds
    /// an array with missing values, `bids += 1.0`.
    pub(crate) fn is_optional_augmented(&self, statement: NodeId) -> bool {
        self.tree
            .child(statement, Role::Target)
            .is_some_and(|target| self.optional_array_element(target).is_some())
    }

    /// `target += value` and the other in-place arithmetic operators, the statement
    /// `statement`, on a variable that holds an array with missing values: the variable is
    /// given the array `target + value` would be, and the one it held is dropped. The new
    /// array is computed first, so the value may read the target. Cost: that of the
    /// arithmetic, a new array and a new bitmap.
    pub(crate) fn optional_augmented(&mut self, statement: NodeId) {
        let Some(target) = self.tree.child(statement, Role::Target) else {
            return;
        };
        let (Some(element), Some((place, ty))) =
            (self.optional_array_element(target), self.target(target))
        else {
            return;
        };
        if gap_operator(self.tree.node(statement).operator()).is_none() {
            return self.unsupported(statement, "this in-place operator on missing values");
        }
        let Some(whole) = self.optional_arithmetic_value(statement, element) else {
            return;
        };
        let result = self.temporary(ty, statement);
        self.assign(Place::from(result), Rvalue::Use(whole), statement);
        self.replace(place, Operand::Move(Place::from(result)), target);
    }

    /// The array with missing values of `element`s that the arithmetic `node` gives, an
    /// expression or an in-place statement: its values and its validity bitmap as one
    /// value. `None` after a report.
    fn optional_arithmetic_value(
        &mut self,
        node: NodeId,
        element: Type<'db>,
    ) -> Option<Operand<'db>> {
        let mut leaves = Leaves::default();
        self.gather_leaves(node, element, &mut leaves);
        let ((first, _), rest) = leaves.arrays.split_first()?;
        let count = self.values_length(first, node);
        for (other, _) in rest {
            let found = self.values_length(other, node);
            self.same_length(0, count.clone(), found, node);
        }
        let bits = self.joined_bitmaps(&leaves.bitmaps, node)?;
        let fused = leaves.arrays.iter().all(|(_, ty)| self.is_float(*ty));
        let values = if fused {
            self.gap_values(node, element, &leaves.operands)
        } else {
            self.gathered_values(node, element, &leaves, (bits, count))
        }?;
        Some(self.with_bits(element, values, bits, node))
    }

    /// Whether `node` is a comparison whose result is an array with missing values.
    pub(crate) fn is_optional_comparison(&self, node: NodeId) -> bool {
        self.tree.kind(node) == SyntaxKind::Compare && self.optional_array_element(node).is_some()
    }

    /// The comparison `node`, `left > right`, whose result is an array of `bool`s with
    /// missing values, into `place`: `None` wherever a side is `None`. Each side is read
    /// once, an expression into a temporary first, and a side that is a single number
    /// converts to the type the two sides' elements meet in. Two arrays must have the same
    /// length, which is checked when the program runs.
    pub(crate) fn optional_comparison(&mut self, place: Place, node: NodeId) {
        let Some(comparator) = self.tree.children_with(node, Role::Item).next() else {
            return;
        };
        let syntax = self.tree.node(comparator).operator();
        let (Some(left), Some(right), Some(operator)) = (
            self.tree.child(node, Role::Left),
            self.tree.child(comparator, Role::Right),
            comparison(syntax),
        ) else {
            return self.unsupported(node, "this comparison of an array with missing values");
        };
        let own = |builder: &Self, side: NodeId| {
            let ty = builder.ty(side);
            match builder.optional_array_element(side) {
                Some(element) => element,
                None if builder.is_array(ty) => builder.element(ty),
                None => ty,
            }
        };
        let common = self.common(own(self, left), own(self, right));
        let mut leaves = Leaves::default();
        self.leaf(left, common, &mut leaves);
        self.leaf(right, common, &mut leaves);
        let Some(((first, _), rest)) = leaves.arrays.split_first() else {
            return;
        };
        let count = self.values_length(first, node);
        for (other, _) in rest {
            let found = self.values_length(other, node);
            self.same_length(0, count.clone(), found, node);
        }
        let (Some(bits), [first, second]) = (
            self.joined_bitmaps(&leaves.bitmaps, node),
            leaves.operands.as_slice(),
        ) else {
            return;
        };
        let tree = Elementwise::Binary(
            operator,
            Box::new(Elementwise::Leaf(first.clone())),
            Box::new(Elementwise::Leaf(second.clone())),
        );
        let boolean = self.boolean_type();
        let values = self.temporary(self.array_of(boolean), node);
        self.assign(
            Place::from(values),
            Rvalue::Elementwise(Box::new(tree)),
            node,
        );
        let whole = self.with_bits(boolean, values, bits, node);
        self.assign(place, Rvalue::Use(whole), node);
    }

    /// Reads every operand of the arithmetic at `node` once, in the order written, into
    /// `leaves`. A single number converts to `element`, the result's element type.
    fn gather_leaves(&mut self, node: NodeId, element: Type<'db>, leaves: &mut Leaves<'db>) {
        if self.tree.kind(node) == SyntaxKind::AugmentedAssign {
            // `target += value` reads its target, then the leaves of its value.
            for (role, whole) in [(Role::Target, true), (Role::Value, false)] {
                match self.tree.child(node, role) {
                    Some(child) if whole => self.leaf(child, element, leaves),
                    Some(child) => self.gather_leaves(child, element, leaves),
                    None => {}
                }
            }
            return;
        }
        if self.is_optional_arithmetic(node) {
            for role in [Role::Left, Role::Right, Role::Value] {
                if let Some(child) = self.tree.child(node, role) {
                    self.gather_leaves(child, element, leaves);
                }
            }
            return;
        }
        self.leaf(node, element, leaves);
    }

    /// Reads `node`, one operand, into `leaves`: an array with or without missing values,
    /// an expression into a temporary first, or a single number, converted to `element`.
    fn leaf(&mut self, node: NodeId, element: Type<'db>, leaves: &mut Leaves<'db>) {
        let array = if let Some(own) = self.optional_array_element(node) {
            let base = self.borrow(node);
            leaves
                .bitmaps
                .push(base.clone().project(Projection::Field(VALIDITY)));
            Some((base.project(Projection::Field(VALUES)), own))
        } else if self.is_array(self.ty(node)) {
            let own = self.element(self.ty(node));
            Some((self.borrow(node), own))
        } else {
            None
        };
        if let Some((values, own)) = array {
            leaves.positions.push(Some(leaves.arrays.len()));
            leaves.operands.push(Operand::Copy(values.clone()));
            leaves.arrays.push((values, own));
        } else {
            leaves.positions.push(None);
            leaves.operands.push(self.operand_as(node, element));
        }
    }

    /// The length of the one-dimensional array of values at `values`, an `i64`.
    pub(crate) fn values_length(&mut self, values: &Place, node: NodeId) -> Operand<'db> {
        let length = self.temporary(self.int_type(), node);
        self.assign(Place::from(length), Rvalue::Length(values.clone(), 0), node);
        Operand::Copy(Place::from(length))
    }

    /// A new validity bitmap whose bit is 1 only where the bit of every one of `bitmaps` is:
    /// a copy of the one, or the library's `both_bits` of each with the rest. The caller owns
    /// it. `None` after a report.
    pub(crate) fn joined_bitmaps(&mut self, bitmaps: &[Place], node: NodeId) -> Option<Local> {
        let (first, rest) = bitmaps.split_first()?;
        let ty = self.bitmap_type();
        let mut joined = self.temporary(ty, node);
        let copy = Elementwise::Leaf(Operand::Copy(first.clone()));
        self.assign(
            Place::from(joined),
            Rvalue::Elementwise(Box::new(copy)),
            node,
        );
        for other in rest {
            let arguments = vec![Place::from(joined), other.clone()];
            let next = self.frame_call("both_bits", Vec::new(), arguments, ty, node)?;
            self.drop_later(joined);
            joined = next;
        }
        Some(joined)
    }

    /// The values of the arithmetic at `node` as one loop over `operands`, its leaves in the
    /// order written: a new array of `element`s, which the caller owns. `None` after a
    /// report.
    fn gap_values(
        &mut self,
        node: NodeId,
        element: Type<'db>,
        operands: &[Operand<'db>],
    ) -> Option<Local> {
        let mut operands = operands.iter().cloned();
        let tree = self.gap_tree(node, &mut operands)?;
        let values = self.temporary(self.array_of(element), node);
        self.assign(
            Place::from(values),
            Rvalue::Elementwise(Box::new(tree)),
            node,
        );
        Some(values)
    }

    /// The values of the arithmetic at `node` computed only where the result has a value, by
    /// the validity bitmap `bits` of its `count` elements: each array is gathered down to
    /// those elements, the loop runs over them, and the library's `spread` puts the results
    /// back with the filler under each `None`. `None` after a report.
    fn gathered_values(
        &mut self,
        node: NodeId,
        element: Type<'db>,
        leaves: &Leaves<'db>,
        (bits, count): (Local, Operand<'db>),
    ) -> Option<Local> {
        let mut gathered = Vec::with_capacity(leaves.arrays.len());
        for (values, own) in &leaves.arrays {
            let arguments = vec![values.clone(), Place::from(bits)];
            let ty = self.array_of(*own);
            let dense = self.frame_call("present", vec![*own], arguments, ty, node)?;
            self.drop_later(dense);
            gathered.push(dense);
        }
        let operands: Vec<Operand<'db>> = leaves
            .operands
            .iter()
            .zip(&leaves.positions)
            .map(
                |(operand, position)| match position.and_then(|at| gathered.get(at)) {
                    Some(dense) => Operand::Copy(Place::from(*dense)),
                    None => operand.clone(),
                },
            )
            .collect();
        let dense = self.gap_values(node, element, &operands)?;
        self.drop_later(dense);
        let filler = Operand::Constant(self.filler(element)?);
        let filler = Place::from(self.held(element, filler, node));
        let count = Place::from(self.held(self.int_type(), count, node));
        let arguments = vec![Place::from(dense), Place::from(bits), count, filler];
        let ty = self.array_of(element);
        self.frame_call("spread", vec![element], arguments, ty, node)
    }

    /// The [`Elementwise`] tree of the arithmetic at `node`, taking each leaf, in the order
    /// written, from `operands`. `None` after a report.
    fn gap_tree(
        &mut self,
        node: NodeId,
        operands: &mut impl Iterator<Item = Operand<'db>>,
    ) -> Option<Elementwise<'db>> {
        let operator = self.tree.node(node).operator();
        if self.tree.kind(node) == SyntaxKind::AugmentedAssign {
            let left = operands.next().map(Elementwise::Leaf)?;
            let right = self.gap_tree(self.tree.child(node, Role::Value)?, operands)?;
            let operator = gap_operator(operator)?;
            return Some(Elementwise::Binary(
                operator,
                Box::new(left),
                Box::new(right),
            ));
        }
        if !self.is_optional_arithmetic(node) {
            return operands.next().map(Elementwise::Leaf);
        }
        if self.tree.kind(node) == SyntaxKind::UnaryOperation {
            let value = self.gap_tree(self.tree.child(node, Role::Value)?, operands)?;
            return Some(match operator {
                Operator::Negate => Elementwise::Unary(UnaryOperator::Negate, Box::new(value)),
                _ => value,
            });
        }
        let left = self.gap_tree(self.tree.child(node, Role::Left)?, operands)?;
        let right = self.gap_tree(self.tree.child(node, Role::Right)?, operands)?;
        Some(Elementwise::Binary(
            gap_operator(operator)?,
            Box::new(left),
            Box::new(right),
        ))
    }
}
