//! Operators: arithmetic, comparisons, chained comparisons, `is None`, `and`, `or` and the
//! conditional expression, each computed into a place.

use crate::builder::Builder;
use mir::{
    BinaryOperator, Check, Constant, ConstantValue, Operand, Place, Rvalue, TerminatorKind,
    UnaryOperator,
};
use syntax::{NodeId, Operator, Role, SyntaxKind};
use types::{Type, TypeKind};

impl<'db> Builder<'db> {
    /// `left op right` on numbers, into `place`.
    pub(crate) fn binary(&mut self, place: Place, node: NodeId) {
        let (Some(left), Some(right)) = (
            self.tree.child(node, Role::Left),
            self.tree.child(node, Role::Right),
        ) else {
            return;
        };
        let ty = self.ty(node);
        let syntax = self.tree.node(node).operator();
        if matches!(ty.kind(self.db), TypeKind::Str) && syntax == Operator::Add {
            return self.joined(place, &[left, right], node);
        }
        // On a type parameter, `+`, `-` and `*` become the instance's number arithmetic. The
        // others depend on whether the number is an integer: `//` and `%` check the divisor,
        // and `/` converts integers to `f64`.
        let generic = matches!(ty.kind(self.db), TypeKind::Parameter { .. })
            && matches!(
                syntax,
                Operator::Add | Operator::Subtract | Operator::Multiply
            );
        if !generic && matches!(ty.kind(self.db), TypeKind::Parameter { .. }) {
            let what = format!("`{}` on a type parameter", syntax.symbol());
            return self.unsupported(node, &what);
        }
        if !generic && !matches!(ty.kind(self.db), TypeKind::Primitive(_)) {
            return self.unsupported(node, "operators on values other than numbers");
        }
        let Some(operator) = arithmetic(syntax) else {
            return self.unsupported(node, &format!("`{}`", syntax.symbol()));
        };
        // `/` on integers gives an `f64`, so both sides convert to it; every other operator
        // works at the type of its result.
        let left = self.operand_as(left, ty);
        let right = self.operand_as(right, ty);
        self.arithmetic(place, operator, (left, right), ty, node);
    }

    /// `place = left op right` on two operands of type `ty`, after the divisor check integer
    /// `//` and `%` need.
    pub(crate) fn arithmetic(
        &mut self,
        place: Place,
        operator: BinaryOperator,
        (left, right): (Operand<'db>, Operand<'db>),
        ty: Type<'db>,
        node: NodeId,
    ) {
        let divides = matches!(
            operator,
            BinaryOperator::FloorDivide | BinaryOperator::Remainder
        );
        let known_nonzero = matches!(
            &right,
            Operand::Constant(Constant { value: ConstantValue::Integer(value), .. }) if *value != 0
        );
        if divides && self.is_integer(ty) && !known_nonzero {
            let zero = Operand::Constant(Constant::integer(ty, 0));
            let condition = self.temporary(self.boolean_type(), node);
            self.assign(
                Place::from(condition),
                Rvalue::Binary(BinaryOperator::NotEqual, right.clone(), zero),
                node,
            );
            self.then(node, |target| TerminatorKind::Assert {
                condition: Operand::Copy(Place::from(condition)),
                check: Check::NonZeroDivisor,
                target,
            });
        }
        self.assign(place, Rvalue::Binary(operator, left, right), node);
    }

    /// `-x`, `+x`, `~x` and `not x`, into `place`.
    pub(crate) fn unary(&mut self, place: Place, node: NodeId) {
        let Some(value) = self.tree.child(node, Role::Value) else {
            return;
        };
        let generic = matches!(self.ty(node).kind(self.db), TypeKind::Parameter { .. })
            && matches!(
                self.tree.node(node).operator(),
                Operator::Negate | Operator::Plus
            );
        if !generic && !matches!(self.ty(node).kind(self.db), TypeKind::Primitive(_)) {
            return self.unsupported(node, "operators on values other than numbers");
        }
        let operand = self.operand(value);
        let rvalue = match self.tree.node(node).operator() {
            Operator::Negate => Rvalue::Unary(UnaryOperator::Negate, operand),
            Operator::Not => Rvalue::Unary(UnaryOperator::Not, operand),
            Operator::Invert => Rvalue::Unary(UnaryOperator::BitNot, operand),
            _ => Rvalue::Use(operand),
        };
        self.assign(place, rvalue, node);
    }

    /// A comparison chain, `a < b <= c`, into `place`: each link is computed only when the ones
    /// before it held, and each operand is computed once.
    pub(crate) fn compare(&mut self, place: &Place, node: NodeId) {
        let Some(mut left) = self.tree.child(node, Role::Left) else {
            return;
        };
        let comparators: Vec<NodeId> = self.tree.children_with(node, Role::Item).collect();
        let mut join = None;
        let mut previous: Option<Operand<'db>> = None;
        for (index, comparator) in comparators.into_iter().enumerate() {
            let Some(right) = self.tree.child(comparator, Role::Right) else {
                return;
            };
            if index > 0 {
                let next = self.new_block();
                let end = *join.get_or_insert_with(|| self.new_block());
                self.terminate(
                    TerminatorKind::Branch {
                        condition: Operand::Copy(place.clone()),
                        then: next,
                        otherwise: end,
                    },
                    comparator,
                );
                self.start(next);
            }
            let operator = self.tree.node(comparator).operator();
            previous = self.comparison(place.clone(), operator, left, right, previous, comparator);
            left = right;
        }
        if let Some(end) = join {
            self.terminate(TerminatorKind::Goto(end), node);
            self.start(end);
        }
    }

    /// One link of a comparison chain, `left op right`, into `place`. `previous` is the left
    /// operand when the link before computed it. Returns the right operand, for the next link.
    fn comparison(
        &mut self,
        place: Place,
        operator: Operator,
        left: NodeId,
        right: NodeId,
        previous: Option<Operand<'db>>,
        node: NodeId,
    ) -> Option<Operand<'db>> {
        if matches!(operator, Operator::Is | Operator::IsNot) {
            self.is_none(place, operator, left, right, node);
            return None;
        }
        let Some(operator) = comparison(operator) else {
            self.unsupported(node, &format!("`{}`", operator.symbol()));
            return None;
        };
        let (left_type, right_type) = (self.ty(left), self.ty(right));
        if !self.is_copy(left_type) || !self.is_copy(right_type) {
            self.unsupported(node, "comparisons of values that are not copied");
            return None;
        }
        let common = self.common(left_type, right_type);
        let left = match previous {
            Some(operand) => self.convert(operand, left_type, common, node),
            None => self.operand_as(left, common),
        };
        let right_operand = self.operand(right);
        let right = self.convert(right_operand.clone(), right_type, common, node);
        self.assign(place, Rvalue::Binary(operator, left, right), node);
        Some(right_operand)
    }

    /// `x is None` and `x is not None`, into `place`.
    fn is_none(
        &mut self,
        place: Place,
        operator: Operator,
        left: NodeId,
        right: NodeId,
        node: NodeId,
    ) {
        let value = match (self.tree.kind(left), self.tree.kind(right)) {
            (_, SyntaxKind::None) => left,
            (SyntaxKind::None, _) => right,
            _ => return self.unsupported(node, "`is` with neither side `None`"),
        };
        if self.optional_value(self.ty(value)).is_none() {
            let present = operator == Operator::IsNot;
            let value = matches!(self.ty(value).kind(self.db), TypeKind::None) != present;
            return self.assign(
                place,
                Rvalue::Use(Operand::Constant(Constant {
                    ty: self.boolean_type(),
                    value: ConstantValue::Bool(value),
                })),
                node,
            );
        }
        let optional = self.borrow(value);
        if operator == Operator::IsNot {
            return self.assign(place, Rvalue::IsPresent(optional), node);
        }
        let present = self.temporary(self.boolean_type(), node);
        self.assign(Place::from(present), Rvalue::IsPresent(optional), node);
        self.assign(
            place,
            Rvalue::Unary(UnaryOperator::Not, Operand::Copy(Place::from(present))),
            node,
        );
    }

    /// `a and b` and `a or b`, into `place`: each operand is computed only when the ones before
    /// it did not decide the result.
    pub(crate) fn boolean(&mut self, place: &Place, node: NodeId) {
        let is_and = self.tree.node(node).operator() == Operator::And;
        let items: Vec<NodeId> = self.tree.children_with(node, Role::Item).collect();
        let boolean = self.boolean_type();
        let mut join = None;
        for (index, item) in items.into_iter().enumerate() {
            if index > 0 {
                let next = self.new_block();
                let end = *join.get_or_insert_with(|| self.new_block());
                let (then, otherwise) = if is_and { (next, end) } else { (end, next) };
                self.terminate(
                    TerminatorKind::Branch {
                        condition: Operand::Copy(place.clone()),
                        then,
                        otherwise,
                    },
                    item,
                );
                self.start(next);
            }
            self.write(place.clone(), boolean, item);
        }
        if let Some(end) = join {
            self.terminate(TerminatorKind::Goto(end), node);
            self.start(end);
        }
    }

    /// `value if condition else other`, into `place` of type `to`.
    pub(crate) fn conditional(&mut self, place: Place, to: Type<'db>, node: NodeId) {
        let (Some(value), Some(condition), Some(other)) = (
            self.tree.child(node, Role::Value),
            self.tree.child(node, Role::Condition),
            self.tree.child(node, Role::Else),
        ) else {
            return;
        };
        let condition = self.operand(condition);
        let (then, otherwise) = (self.new_block(), self.new_block());
        self.terminate(
            TerminatorKind::Branch {
                condition,
                then,
                otherwise,
            },
            node,
        );
        let mut join = None;
        self.start(then);
        self.write(place.clone(), to, value);
        self.jump(&mut join, node);
        self.start(otherwise);
        self.write(place, to, other);
        self.jump(&mut join, node);
        self.join(join);
    }
}

/// The IR operator for an arithmetic or bitwise operator of the source; `None` for `**` and
/// `@`, which are not single operations.
pub(crate) fn arithmetic(operator: Operator) -> Option<BinaryOperator> {
    Some(match operator {
        Operator::Add => BinaryOperator::Add,
        Operator::Subtract => BinaryOperator::Subtract,
        Operator::Multiply => BinaryOperator::Multiply,
        Operator::Divide => BinaryOperator::Divide,
        Operator::FloorDivide => BinaryOperator::FloorDivide,
        Operator::Modulo => BinaryOperator::Remainder,
        Operator::LeftShift => BinaryOperator::ShiftLeft,
        Operator::RightShift => BinaryOperator::ShiftRight,
        Operator::BitAnd => BinaryOperator::BitAnd,
        Operator::BitOr => BinaryOperator::BitOr,
        Operator::BitXor => BinaryOperator::BitXor,
        _ => return None,
    })
}

/// The IR operator for a comparison operator of the source; `None` for `is` and `in`.
fn comparison(operator: Operator) -> Option<BinaryOperator> {
    Some(match operator {
        Operator::Equal => BinaryOperator::Equal,
        Operator::NotEqual => BinaryOperator::NotEqual,
        Operator::Less => BinaryOperator::Less,
        Operator::LessEqual => BinaryOperator::LessEqual,
        Operator::Greater => BinaryOperator::Greater,
        Operator::GreaterEqual => BinaryOperator::GreaterEqual,
        _ => return None,
    })
}
