//! Arithmetic on arrays with missing values (decision 0045, part 3): `bids + asks`,
//! `bids * 2.0` and `-bids` give an array with missing values that is `None` wherever a
//! side is `None`, as pandas's nullable columns do. A comparison, `bids > 1.0`, gives an
//! `Array[bool | None, 1]` the same way (Collins, 2026-11-14): `None` where a side is.

use super::{Body, numbers, relations};
use syntax::{NodeId, Operator};
use types::{Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// `left operator right` at `node`, where a side is an array with missing values. The
    /// other side is one too, a one-dimensional `Array` of numbers or a single number, and
    /// the elements combine as two numbers would, so the result's elements have the type
    /// the same operator gives two plain arrays, an `f64` for `/` of two integers. `+`, `-`,
    /// `*`, `/`, `//` and `%` are built.
    pub(super) fn optional_array_arithmetic(
        &mut self,
        node: NodeId,
        operator: Operator,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Type<'db> {
        let symbol = operator.symbol();
        if relations::is_open(self.db, left) || relations::is_open(self.db, right) {
            return self.lowerer.unknown();
        }
        if !matches!(
            operator,
            Operator::Add
                | Operator::Subtract
                | Operator::Multiply
                | Operator::Divide
                | Operator::FloorDivide
                | Operator::Modulo
        ) {
            return self.optional_array_unbuilt(node, &format!("`{symbol}`"));
        }
        let (Some(first), Some(second)) = (self.gap_operand(left), self.gap_operand(right)) else {
            let message = format!(
                "`{symbol}` cannot combine `{}` and `{}`",
                self.show(left),
                self.show(right)
            );
            return self.lowerer.error(
                node,
                &message,
                "an array that may hold `None`",
                Some(
                    "an array of numbers with missing values combines with another, with an \
                     `Array` of numbers of one dimension, or with a number",
                ),
            );
        };
        let result = self.combine(node, operator, first, second);
        match result.kind(self.db) {
            TypeKind::Array { element, rank: 1 } => {
                Type::new(self.db, TypeKind::OptionalArray(*element))
            }
            _ => result,
        }
    }

    /// `left operator right` at the comparator `node`, where a side is an array with missing
    /// values: an `Array[bool | None, 1]` that is `None` wherever a side is, as pandas's
    /// nullable columns compare. The sides are what arithmetic takes, and `==`, `!=`, `<`,
    /// `<=`, `>` and `>=` of numbers are built; `in`, `is` and comparing text or `bool`s are
    /// not.
    pub(super) fn optional_array_comparison(
        &mut self,
        node: NodeId,
        operator: Operator,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Type<'db> {
        let symbol = operator.symbol();
        if relations::is_open(self.db, left) || relations::is_open(self.db, right) {
            return self.lowerer.unknown();
        }
        if !matches!(
            operator,
            Operator::Equal
                | Operator::NotEqual
                | Operator::Less
                | Operator::LessEqual
                | Operator::Greater
                | Operator::GreaterEqual
        ) {
            return self.optional_array_unbuilt(node, &format!("`{symbol}`"));
        }
        let unbuilt = |ty: Type<'db>| match ty.kind(self.db) {
            TypeKind::OptionalArray(element) => matches!(
                element.kind(self.db),
                TypeKind::Str | TypeKind::Primitive(Primitive::Bool)
            ),
            _ => false,
        };
        if unbuilt(left) || unbuilt(right) {
            let what = format!("`{symbol}` of text or of `bool`s");
            return self.optional_array_unbuilt(node, &what);
        }
        let (Some(first), Some(second)) = (self.gap_operand(left), self.gap_operand(right)) else {
            let message = format!(
                "`{symbol}` cannot compare `{}` and `{}`",
                self.show(left),
                self.show(right)
            );
            return self.lowerer.error(
                node,
                &message,
                "an array that may hold `None`",
                Some(
                    "an array of numbers with missing values compares with another, with an \
                     `Array` of numbers of one dimension, or with a number",
                ),
            );
        };
        match self.array_comparison(node, operator, first, second) {
            Some(_) => {
                let boolean = Type::new(self.db, TypeKind::Primitive(Primitive::Bool));
                Type::new(self.db, TypeKind::OptionalArray(boolean))
            }
            None => self.lowerer.unknown(),
        }
    }

    /// `-values` or `+values` at `node` on the array with missing values `operand`, whose
    /// elements must be numbers: an array of the same type, `None` where `operand` is. `~`
    /// waits for comparisons and masks on such arrays.
    pub(super) fn optional_array_unary(
        &mut self,
        node: NodeId,
        operator: Operator,
        operand: Type<'db>,
    ) -> Type<'db> {
        let symbol = operator.symbol();
        if !matches!(operator, Operator::Negate | Operator::Plus) {
            return self.optional_array_unbuilt(node, &format!("`{symbol}`"));
        }
        if self.gap_operand(operand).is_some() {
            return operand;
        }
        let message = format!("`{symbol}` does not apply to `{}`", self.show(operand));
        self.lowerer
            .error(node, &message, "its elements are not numbers", None)
    }

    /// The type that stands for `ty` when the checker combines the operands of arithmetic on
    /// an array with missing values: the `Array[T, 1]` of an `Array[T | None, 1]`, and a
    /// one-dimensional `Array` or a single value as it is. `None` unless the elements, or the
    /// value, are numbers.
    fn gap_operand(&self, ty: Type<'db>) -> Option<Type<'db>> {
        let number = |element: Type<'db>| {
            matches!(element.kind(self.db), TypeKind::Primitive(primitive)
                if numbers::is_number(*primitive))
        };
        match ty.kind(self.db) {
            TypeKind::OptionalArray(element) if number(*element) => Some(Type::new(
                self.db,
                TypeKind::Array {
                    element: *element,
                    rank: 1,
                },
            )),
            TypeKind::Array { element, rank: 1 } if number(*element) => Some(ty),
            _ if number(ty) => Some(ty),
            _ => None,
        }
    }
}
