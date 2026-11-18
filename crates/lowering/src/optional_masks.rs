//! Masks that may hold `None`, combined: `(bids > 1.0) & (asks < 3.0)`, `|`, `^` and `~`,
//! where a side is an `Array[bool | None, 1]` (decision 0045, part 3).
//!
//! The rules are the ones pandas, Arrow and SQL share: the result is known wherever one
//! side settles it, so `False & None` is `False` and `True | None` is `True`, while
//! `True & None`, `False | None`, `None ^ x` and `~None` are `None`.
//!
//! The values are one loop over the two sides' stored values, combined as plain `bool`s,
//! which is right wherever the result is known. The validity bitmap comes from the standard
//! library's `and_bits` and `or_bits` (`<std>/frame_masks.byn`, decision 0038), which read
//! the values too. `^` is known only where both sides are, the bitmap arithmetic has, and
//! `~` keeps its operand's bitmap. A side that is a plain `Array[bool, 1]` gets a bitmap of
//! ones from `full_bits`.

use crate::builder::Builder;
use crate::optional_arrays::{VALIDITY, VALUES};
use mir::UnaryOperator;
use mir::{BinaryOperator, Elementwise, Local, Operand, Place, Projection, Rvalue};
use syntax::{NodeId, Operator, Role, SyntaxKind};

impl Builder<'_> {
    /// Whether `node` is `&`, `|`, `^` or `~` whose result is a mask that may hold `None`.
    pub(crate) fn is_optional_mask_operation(&self, node: NodeId) -> bool {
        if self.optional_array_element(node) != Some(self.boolean_type()) {
            return false;
        }
        let operator = || self.tree.node(node).operator();
        match self.tree.kind(node) {
            SyntaxKind::BinaryOperation => matches!(
                operator(),
                Operator::BitAnd | Operator::BitOr | Operator::BitXor
            ),
            SyntaxKind::UnaryOperation => operator() == Operator::Invert,
            _ => false,
        }
    }

    /// The operation `node` on masks, whose result is a mask that may hold `None`, into
    /// `place`. Each side is read once, an expression into a temporary first. Two sides
    /// must have the same length, which is checked when the program runs.
    pub(crate) fn optional_mask_operation(&mut self, place: Place, node: NodeId) {
        let boolean = self.boolean_type();
        let operator = self.tree.node(node).operator();
        let (tree, bits) = if operator == Operator::Invert {
            let Some(value) = self.tree.child(node, Role::Value) else {
                return;
            };
            let Some((values, bits)) = self.mask_side(value) else {
                return;
            };
            let turned = Elementwise::Unary(UnaryOperator::Not, Box::new(leaf(&values)));
            (turned, self.joined_bitmaps(&[bits], node))
        } else {
            let (Some(left), Some(right)) = (
                self.tree.child(node, Role::Left),
                self.tree.child(node, Role::Right),
            ) else {
                return;
            };
            let (Some((left, left_bits)), Some((right, right_bits))) =
                (self.mask_side(left), self.mask_side(right))
            else {
                return;
            };
            let count = self.values_length(&left, node);
            let found = self.values_length(&right, node);
            self.same_length(0, count, found, node);
            let (combined, kernel) = match operator {
                Operator::BitAnd => (BinaryOperator::BitAnd, Some("and_bits")),
                Operator::BitOr => (BinaryOperator::BitOr, Some("or_bits")),
                _ => (BinaryOperator::BitXor, None),
            };
            let bits = match kernel {
                Some(kernel) => {
                    let arguments = vec![left.clone(), left_bits, right.clone(), right_bits];
                    let ty = self.bitmap_type();
                    self.frame_call(kernel, Vec::new(), arguments, ty, node)
                }
                None => self.joined_bitmaps(&[left_bits, right_bits], node),
            };
            let both = Elementwise::Binary(combined, Box::new(leaf(&left)), Box::new(leaf(&right)));
            (both, bits)
        };
        let Some(bits) = bits else {
            return;
        };
        let values = self.temporary(self.array_of(boolean), node);
        self.assign(
            Place::from(values),
            Rvalue::Elementwise(Box::new(tree)),
            node,
        );
        let whole = self.with_bits(boolean, values, bits, node);
        self.assign(place, Rvalue::Use(whole), node);
    }

    /// The stored values and the validity bitmap of `node`, one side of an operation on
    /// masks: those of a mask that may hold `None`, or a plain mask with a bitmap of ones,
    /// which is dropped when the statement ends. `None` after a report.
    fn mask_side(&mut self, node: NodeId) -> Option<(Place, Place)> {
        let base = self.borrow(node);
        if self.optional_array_element(node).is_some() {
            let values = base.clone().project(Projection::Field(VALUES));
            return Some((values, base.project(Projection::Field(VALIDITY))));
        }
        let count = self.values_length(&base, node);
        let count = Place::from(self.held(self.int_type(), count, node));
        let ty = self.bitmap_type();
        let bits: Local = self.frame_call("full_bits", Vec::new(), vec![count], ty, node)?;
        self.drop_later(bits);
        Some((base, Place::from(bits)))
    }
}

/// The leaf of an [`Elementwise`] tree that reads the array at `values`.
fn leaf<'db>(values: &Place) -> Elementwise<'db> {
    Elementwise::Leaf(Operand::Copy(values.clone()))
}
