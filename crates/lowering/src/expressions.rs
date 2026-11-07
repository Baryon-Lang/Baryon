//! Expressions: each one either names a place, is a constant, or is computed into a place by
//! statements and, for `and`, `or`, conditionals and calls, blocks.

use crate::builder::Builder;
use mir::{
    BinaryOperator, Check, Constant, ConstantValue, Operand, Place, Projection, Rvalue,
    TerminatorKind, UnaryOperator,
};
use syntax::{NodeId, Operator, Role, SyntaxKind};
use types::{Declaration, Item, Primitive, Type, TypeKind};

impl<'db> Builder<'db> {
    /// Writes the value of the expression `node` into `place`, which has type `to`, converting
    /// it where the types differ: a `T` into a `T | None`, or one number type into another.
    pub(crate) fn write(&mut self, place: Place, to: Type<'db>, node: NodeId) {
        let ty = self.ty(node);
        let kind = self.tree.kind(node);
        if kind == SyntaxKind::Conditional {
            return self.conditional(place, to, node);
        }
        if ty != to {
            let operand = self.operand(node);
            let rvalue = self.coerce(operand, ty, to);
            return self.assign(place, rvalue, node);
        }
        match kind {
            _ if self.is_literal(node) => {
                let operand = self.operand(node);
                self.assign(place, Rvalue::Use(operand), node);
            }
            SyntaxKind::Name | SyntaxKind::Attribute => {
                let operand = self.operand(node);
                self.assign(place, Rvalue::Use(operand), node);
            }
            SyntaxKind::BinaryOperation => self.binary(place, node),
            SyntaxKind::UnaryOperation => self.unary(place, node),
            SyntaxKind::Compare => self.compare(&place, node),
            SyntaxKind::BooleanOperation => self.boolean(&place, node),
            SyntaxKind::Call => self.call(place, node),
            SyntaxKind::Tuple => {
                let items: Vec<NodeId> = self.tree.children_with(node, Role::Item).collect();
                let operands = items.into_iter().map(|item| self.operand(item)).collect();
                self.assign(place, Rvalue::Aggregate(ty, operands), node);
            }
            SyntaxKind::List => self.unsupported(node, "list displays"),
            SyntaxKind::Subscript => self.unsupported(node, "indexing and slicing"),
            SyntaxKind::FormattedString => self.unsupported(node, "f-strings"),
            SyntaxKind::ListComprehension => self.unsupported(node, "comprehensions"),
            _ => self.unsupported(node, "this expression"),
        }
    }

    /// The value of `node` as an operand: a constant, a copy or move of the place it names, or
    /// a temporary it is computed into. A value that does not copy moves.
    pub(crate) fn operand(&mut self, node: NodeId) -> Operand<'db> {
        if self.is_literal(node) {
            return Operand::Constant(self.literal(node));
        }
        if let Some(constant) = self.named_constant(node) {
            return Operand::Constant(constant);
        }
        let ty = self.ty(node);
        if let Some(place) = self.place(node) {
            return self.take(place, ty);
        }
        if matches!(
            self.tree.kind(node),
            SyntaxKind::Name | SyntaxKind::Attribute
        ) {
            // A name of something lowering does not handle, already reported by `place`.
            return Operand::Constant(Constant {
                ty,
                value: ConstantValue::None,
            });
        }
        let temporary = self.temporary(ty, node);
        self.write(Place::from(temporary), ty, node);
        self.take(Place::from(temporary), ty)
    }

    /// The value of `node` converted to the number type `to`.
    pub(crate) fn operand_as(&mut self, node: NodeId, to: Type<'db>) -> Operand<'db> {
        let ty = self.ty(node);
        let operand = self.operand(node);
        self.convert(operand, ty, to, node)
    }

    /// `operand`, of type `from`, converted to type `to`, through a temporary when it must be.
    pub(crate) fn convert(
        &mut self,
        operand: Operand<'db>,
        from: Type<'db>,
        to: Type<'db>,
        node: NodeId,
    ) -> Operand<'db> {
        if from == to {
            return operand;
        }
        let rvalue = self.coerce(operand, from, to);
        let temporary = self.temporary(to, node);
        self.assign(Place::from(temporary), rvalue, node);
        self.take(Place::from(temporary), to)
    }

    /// The rvalue that turns `operand`, of type `from`, into a value of type `to`: `None` or a
    /// `T` into a `T | None`, or a number into another number type.
    fn coerce(&self, operand: Operand<'db>, from: Type<'db>, to: Type<'db>) -> Rvalue<'db> {
        if from == to {
            return Rvalue::Use(operand);
        }
        if let Some(value) = self.optional_value(to) {
            if matches!(from.kind(self.db), TypeKind::None) {
                return Rvalue::Use(Operand::Constant(Constant {
                    ty: to,
                    value: ConstantValue::None,
                }));
            }
            if value == from {
                return Rvalue::Aggregate(to, vec![operand]);
            }
        }
        match (from.kind(self.db), to.kind(self.db)) {
            (TypeKind::Primitive(_), TypeKind::Primitive(_)) => Rvalue::Convert(operand, to),
            _ => Rvalue::Use(operand),
        }
    }

    /// A copy of the value at `place` of type `ty`, or a move when the type does not copy.
    pub(crate) fn take(&self, place: Place, ty: Type<'db>) -> Operand<'db> {
        if self.is_copy(ty) {
            Operand::Copy(place)
        } else {
            Operand::Move(place)
        }
    }

    /// The place `node` names: a variable or parameter, or a field of a place. A read of a
    /// `T | None` variable the checker narrowed to `T` reads the value inside.
    pub(crate) fn place(&mut self, node: NodeId) -> Option<Place> {
        match self.tree.kind(node) {
            SyntaxKind::Name => {
                let Some(local) = self.variable(node) else {
                    self.unsupported(node, "names other than variables, parameters and constants");
                    return None;
                };
                let held = self.local_type(local);
                let read = self.typed.expression(node);
                let place = Place::from(local);
                Some(match read {
                    Some(read) if read != held && self.optional_value(held) == Some(read) => {
                        place.project(Projection::Value)
                    }
                    _ => place,
                })
            }
            SyntaxKind::Attribute => {
                let value = self.tree.child(node, Role::Value)?;
                let field = self.field(value, node)?;
                if !matches!(
                    self.tree.kind(value),
                    SyntaxKind::Name | SyntaxKind::Attribute
                ) {
                    self.unsupported(node, "fields of a value that is not a variable");
                    return None;
                }
                let base = self.place(value)?;
                Some(base.project(Projection::Field(field)))
            }
            _ => None,
        }
    }

    /// A place holding the value of `node`, for a borrow: the place it names, or a temporary
    /// it is computed into, dropped when the statement ends.
    pub(crate) fn borrow(&mut self, node: NodeId) -> Place {
        if !self.is_literal(node)
            && self.named_constant(node).is_none()
            && let Some(place) = self.place(node)
        {
            return place;
        }
        let ty = self.ty(node);
        let temporary = self.temporary(ty, node);
        self.write(Place::from(temporary), ty, node);
        self.drop_later(temporary);
        Place::from(temporary)
    }

    /// The position of the field `attribute` names in the struct `value` holds, or `None` with
    /// an error when `value` is not a struct.
    fn field(&mut self, value: NodeId, attribute: NodeId) -> Option<u32> {
        let name = self.tree.child(attribute, Role::Attribute)?;
        let TypeKind::Struct(nominal) = self.ty(value).kind(self.db) else {
            self.unsupported(attribute, "attributes other than struct fields");
            return None;
        };
        let item = Item::new(self.db, self.package, nominal.file, nominal.name);
        let Declaration::Struct { fields, .. } = &self.db.declaration(item).declaration else {
            return None;
        };
        let wanted = self.text_of(name);
        let position = fields.iter().position(|field| field.name == wanted)?;
        u32::try_from(position).ok()
    }

    /// `left op right` on numbers, into `place`.
    fn binary(&mut self, place: Place, node: NodeId) {
        let (Some(left), Some(right)) = (
            self.tree.child(node, Role::Left),
            self.tree.child(node, Role::Right),
        ) else {
            return;
        };
        let ty = self.ty(node);
        if !matches!(ty.kind(self.db), TypeKind::Primitive(_)) {
            return self.unsupported(node, "operators on values other than numbers");
        }
        let syntax = self.tree.node(node).operator();
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
    fn unary(&mut self, place: Place, node: NodeId) {
        let Some(value) = self.tree.child(node, Role::Value) else {
            return;
        };
        if !matches!(self.ty(node).kind(self.db), TypeKind::Primitive(_)) {
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
    fn compare(&mut self, place: &Place, node: NodeId) {
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
    fn boolean(&mut self, place: &Place, node: NodeId) {
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
    fn conditional(&mut self, place: Place, to: Type<'db>, node: NodeId) {
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

    /// The `bool` type.
    pub(crate) fn boolean_type(&self) -> Type<'db> {
        Type::new(self.db, TypeKind::Primitive(Primitive::Bool))
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
