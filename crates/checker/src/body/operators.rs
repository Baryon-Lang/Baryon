//! Operators: arithmetic, bitwise and boolean operators, and comparisons, with the number
//! mixing rules of the `numbers` module.

use super::{Body, number_context, numbers, relations};
use syntax::{NodeId, Operator, Role};
use types::{Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// `left op right`.
    pub(super) fn binary(&mut self, node: NodeId) -> Type<'db> {
        let (Some(left), Some(right)) = (
            self.tree.child(node, Role::Left),
            self.tree.child(node, Role::Right),
        ) else {
            return self.lowerer.unknown();
        };
        let (left, right) = self.operands(left, right);
        self.combine(node, self.tree.node(node).operator(), left, right)
    }

    /// The types of two operands. A number literal takes the other side's type.
    pub(super) fn operands(&mut self, left: NodeId, right: NodeId) -> (Type<'db>, Type<'db>) {
        if self.is_number_literal(left) && !self.is_number_literal(right) {
            let right = self.expression(right, None);
            let left = self.expression(left, number_context(self.db, right));
            (left, right)
        } else {
            let left = self.expression(left, None);
            let right = self.expression(right, number_context(self.db, left));
            (left, right)
        }
    }

    /// The type of `left operator right`, reporting operands that do not combine.
    pub(super) fn combine(
        &mut self,
        node: NodeId,
        operator: Operator,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Type<'db> {
        let unknown = self.lowerer.unknown();
        if relations::is_open(self.db, left) || relations::is_open(self.db, right) {
            return unknown;
        }
        let symbol = operator.symbol();
        match (left.kind(self.db), right.kind(self.db)) {
            (TypeKind::Primitive(a), TypeKind::Primitive(b)) => {
                let result = match operator {
                    Operator::BitAnd
                    | Operator::BitOr
                    | Operator::BitXor
                    | Operator::LeftShift
                    | Operator::RightShift => match (a, b) {
                        (Primitive::Integer(_), Primitive::Integer(_)) if a == b => Some(*a),
                        (Primitive::Bool, Primitive::Bool)
                            if matches!(
                                operator,
                                Operator::BitAnd | Operator::BitOr | Operator::BitXor
                            ) =>
                        {
                            Some(*a)
                        }
                        _ => None,
                    },
                    Operator::MatrixMultiply => None,
                    _ => numbers::arithmetic(operator, *a, *b),
                };
                match result {
                    Some(result) => Type::new(self.db, TypeKind::Primitive(result)),
                    None => self.mixing_error(node, symbol, *a, *b),
                }
            }
            (TypeKind::Str, TypeKind::Str) if operator == Operator::Add => left,
            (TypeKind::List(_), TypeKind::List(_))
                if operator == Operator::Add && left == right =>
            {
                left
            }
            _ if self.is_scalar(left) && self.is_scalar(right) => {
                let message = format!(
                    "`{symbol}` cannot combine `{}` and `{}`",
                    self.show(left),
                    self.show(right)
                );
                self.lowerer
                    .error(node, &message, "no such operation", None)
            }
            _ => unknown,
        }
    }

    /// Reports two numbers an operator or a built-in function, `symbol`, cannot mix, suggesting a conversion.
    pub(super) fn mixing_error(
        &mut self,
        node: NodeId,
        symbol: &str,
        a: Primitive,
        b: Primitive,
    ) -> Type<'db> {
        let message = format!("`{symbol}` cannot mix `{a}` and `{b}`");
        let hint = numbers::suggestion(a, b).map(|target| {
            format!("convert one side so both are the same type, such as `{target}(x)`")
        });
        let label = if a == Primitive::Bool || b == Primitive::Bool {
            "`bool` is not a number"
        } else {
            "different number types"
        };
        self.lowerer.error(node, &message, label, hint.as_deref())
    }

    /// `-x`, `+x`, `~x` and `not x`.
    pub(super) fn unary(&mut self, node: NodeId) -> Type<'db> {
        let operator = self.tree.node(node).operator();
        let operand = self.child_type(node, Role::Value);
        if relations::is_open(self.db, operand) {
            return self.lowerer.unknown();
        }
        let valid = match (operator, operand.kind(self.db)) {
            (Operator::Not, TypeKind::Primitive(Primitive::Bool))
            | (Operator::Invert, TypeKind::Primitive(Primitive::Integer(_))) => true,
            (Operator::Negate | Operator::Plus, TypeKind::Primitive(primitive)) => {
                numbers::is_number(*primitive)
            }
            (Operator::Not, _) => false,
            _ => !self.is_scalar(operand),
        };
        if !valid {
            let message = format!(
                "`{}` does not apply to `{}`",
                operator.symbol(),
                self.show(operand)
            );
            return self
                .lowerer
                .error(node, &message, "no such operation", None);
        }
        if self.is_scalar(operand) {
            operand
        } else {
            self.lowerer.unknown()
        }
    }

    /// `a and b`, `a or b`: every operand is a `bool`.
    pub(super) fn boolean(&mut self, node: NodeId) -> Type<'db> {
        let boolean = Type::new(self.db, TypeKind::Primitive(Primitive::Bool));
        let symbol = self.tree.node(node).operator().symbol();
        let items: Vec<NodeId> = self.tree.children_with(node, Role::Item).collect();
        for item in items {
            let found = self.expression(item, Some(boolean));
            if !relations::compatible(self.db, boolean, found) {
                let message = format!(
                    "`{symbol}` needs `bool` operands, found `{}`",
                    self.show(found)
                );
                self.lowerer.error(
                    item,
                    &message,
                    "not a `bool`",
                    Some("compare explicitly, such as `n != 0` or `x is not None`"),
                );
            }
        }
        boolean
    }

    /// `a < b <= c`: each link compares numbers that mix, or values of one type.
    pub(super) fn compare(&mut self, node: NodeId) -> Type<'db> {
        let boolean = Type::new(self.db, TypeKind::Primitive(Primitive::Bool));
        let Some(mut left_node) = self.tree.child(node, Role::Left) else {
            return boolean;
        };
        let comparators: Vec<NodeId> = self.tree.children_with(node, Role::Item).collect();
        let mut left: Option<Type<'db>> = None;
        for comparator in comparators {
            let operator = self.tree.node(comparator).operator();
            let Some(right_node) = self.tree.child(comparator, Role::Right) else {
                continue;
            };
            let (left_type, right_type) = match left {
                Some(left_type) => {
                    let right = self.expression(right_node, number_context(self.db, left_type));
                    (left_type, right)
                }
                None => self.operands(left_node, right_node),
            };
            if !matches!(
                operator,
                Operator::Is | Operator::IsNot | Operator::In | Operator::NotIn
            ) {
                self.comparable(comparator, operator, left_type, right_type);
            }
            left = Some(right_type);
            left_node = right_node;
        }
        boolean
    }

    /// Reports two values one comparison cannot compare.
    fn comparable(&mut self, node: NodeId, operator: Operator, left: Type<'db>, right: Type<'db>) {
        if relations::is_open(self.db, left) || relations::is_open(self.db, right) {
            return;
        }
        let symbol = operator.symbol();
        let ordering = matches!(
            operator,
            Operator::Less | Operator::LessEqual | Operator::Greater | Operator::GreaterEqual
        );
        match (left.kind(self.db), right.kind(self.db)) {
            (TypeKind::Primitive(a), TypeKind::Primitive(b)) => {
                let complex =
                    matches!(a, Primitive::Complex(_)) || matches!(b, Primitive::Complex(_));
                let mixes = if a == b {
                    true
                } else {
                    numbers::common(*a, *b).is_some()
                };
                if !mixes {
                    self.mixing_error(node, symbol, *a, *b);
                } else if ordering && (complex || *a == Primitive::Bool) {
                    let message = format!("`{symbol}` cannot order `{}` values", self.show(left));
                    self.lowerer.error(node, &message, "no order", None);
                }
            }
            _ if left == right || relations::compatible(self.db, left, right) => {}
            _ if self.is_scalar(left) && self.is_scalar(right) => {
                let message = format!(
                    "`{symbol}` cannot compare `{}` and `{}`",
                    self.show(left),
                    self.show(right)
                );
                self.lowerer.error(node, &message, "different types", None);
            }
            _ => {}
        }
    }
}
