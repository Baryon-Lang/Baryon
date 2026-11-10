//! Operators: arithmetic, comparisons, chained comparisons, `is None`, `and`, `or` and the
//! conditional expression, each computed into a place.

use crate::builder::Builder;
use mir::{
    BinaryOperator, Check, Constant, ConstantValue, MathFunction, Operand, Place, Projection,
    Rvalue, TerminatorKind, UnaryOperator,
};
use syntax::{NodeId, Operator, Role, SyntaxKind};
use types::{Primitive, Type, TypeKind};

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
        if syntax == Operator::Power && self.is_float(ty) {
            let operands = vec![self.operand_as(left, ty), self.operand_as(right, ty)];
            return self.math_call(place, MathFunction::Power, operands, node);
        }
        if syntax == Operator::Power && self.is_integer(ty) {
            let operands = (self.operand_as(left, ty), self.operand_as(right, ty));
            return self.integer_power(place, operands, ty, node);
        }
        let Some(operator) = arithmetic(syntax) else {
            return self.unsupported(node, &format!("`{}`", syntax.symbol()));
        };
        // `/` on integers gives an `f64`, so both sides convert to it; every other operator
        // works at the type of its result. Under a `VectorSpace` bound, `h * state` scales by
        // an `f64`, which stays an `f64`: monomorphization makes it a whole-array expression
        // that applies it to every element.
        let scaling = |builder: &Self, side: NodeId| {
            generic && matches!(builder.ty(side).kind(builder.db), TypeKind::Primitive(_))
        };
        let left = if scaling(self, left) {
            self.operand(left)
        } else {
            self.operand_as(left, ty)
        };
        let right = if scaling(self, right) {
            self.operand(right)
        } else {
            self.operand_as(right, ty)
        };
        if generic {
            // Into a local of the parameter's type, so monomorphization can tell the
            // instance's integer arithmetic by its destination and check it for overflow.
            let result = self.temporary(ty, node);
            self.arithmetic(Place::from(result), operator, (left, right), ty, node);
            let result = Operand::Copy(Place::from(result));
            return self.assign(place, Rvalue::Use(result), node);
        }
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
        let overflows = matches!(
            operator,
            BinaryOperator::Add | BinaryOperator::Subtract | BinaryOperator::Multiply
        );
        if overflows && self.is_integer(ty) {
            return self.checked(place, operator, (left, right), ty, node);
        }
        self.assign(place, Rvalue::Binary(operator, left, right), node);
    }

    /// `place = left op right` for integer `+`, `-` or `*`, with an overflow check: a
    /// [`CheckedBinary`](Rvalue::CheckedBinary) into a `tuple[T, bool]` temporary, an `Assert`
    /// that its flag is `False`, then its result. A debug build panics there; a release build's
    /// backend drops the check and keeps the wrapped result (decision 0030).
    fn checked(
        &mut self,
        place: Place,
        operator: BinaryOperator,
        operands: (Operand<'db>, Operand<'db>),
        ty: Type<'db>,
        node: NodeId,
    ) {
        let check = Check::NoOverflow(operator);
        self.checked_as(place, (operator, check), operands, ty, node);
    }

    /// [`checked`](Self::checked), with the overflow reported as `check`, so the multiplications
    /// of integer `**` can name the `**`.
    pub(crate) fn checked_as(
        &mut self,
        place: Place,
        (operator, check): (BinaryOperator, Check<'db>),
        (left, right): (Operand<'db>, Operand<'db>),
        ty: Type<'db>,
        node: NodeId,
    ) {
        let boolean = self.boolean_type();
        let pair = self.temporary(Type::new(self.db, TypeKind::Tuple(vec![ty, boolean])), node);
        self.assign(
            Place::from(pair),
            Rvalue::CheckedBinary(operator, left, right),
            node,
        );
        let fits = self.temporary(boolean, node);
        self.assign(
            Place::from(fits),
            Rvalue::Unary(
                UnaryOperator::Not,
                Operand::Copy(Place::from(pair).project(Projection::Field(1))),
            ),
            node,
        );
        self.then(node, |target| TerminatorKind::Assert {
            condition: Operand::Copy(Place::from(fits)),
            check,
            target,
        });
        let result = Operand::Copy(Place::from(pair).project(Projection::Field(0)));
        self.assign(place, Rvalue::Use(result), node);
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
        let ty = self.ty(node);
        let operator = self.tree.node(node).operator();
        if operator == Operator::Negate
            && let TypeKind::Primitive(Primitive::Integer(integer)) = ty.kind(self.db)
            && integer.is_signed()
        {
            // `-x` is `0 - x`, which overflows only for the type's minimum, as `-i64::MIN`.
            let zero = Operand::Constant(Constant::integer(ty, 0));
            let operand = self.operand(value);
            return self.checked(place, BinaryOperator::Subtract, (zero, operand), ty, node);
        }
        let operand = self.operand(value);
        if generic && operator == Operator::Negate {
            // As for `+`, `-` and `*` on a parameter: a local of its type, which
            // monomorphization checks when the instance's type is a signed integer.
            let result = self.temporary(ty, node);
            let negated = Rvalue::Unary(UnaryOperator::Negate, operand);
            self.assign(Place::from(result), negated, node);
            let result = Operand::Copy(Place::from(result));
            return self.assign(place, Rvalue::Use(result), node);
        }
        let rvalue = match operator {
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
        let (left_type, right_type) = (self.ty(left), self.ty(right));
        if matches!(operator, Operator::In | Operator::NotIn)
            && matches!(right_type.kind(self.db), TypeKind::List(_))
        {
            self.list_contains(place, operator, (left, right), node);
            return None;
        }
        let is_text = |builder: &Self, ty: Type<'db>| matches!(ty.kind(builder.db), TypeKind::Str);
        if is_text(self, left_type) && is_text(self, right_type) {
            return self.text_comparison(place, operator, (left, right), previous, node);
        }
        let Some(operator) = comparison(operator) else {
            self.unsupported(node, &format!("`{}`", operator.symbol()));
            return None;
        };
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
