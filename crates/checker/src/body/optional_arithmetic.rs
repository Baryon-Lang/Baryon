//! Arithmetic on arrays with missing values (decision 0045, part 3): `bids + asks`,
//! `bids * 2.0` and `-bids` give an array with missing values that is `None` wherever a
//! side is `None`, as pandas's nullable columns do. A comparison, `bids > 1.0`, gives an
//! `Array[bool | None, 1]` the same way (Collins, 2026-11-14): `None` where a side is. Two
//! such masks combine with `&`, `|` and `^`, and `~` turns one around, by pandas's rules.

use super::{Body, numbers, relations};
use syntax::{NodeId, Operator};
use types::{Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// `left operator right` at `node`, where a side is an array with missing values. The
    /// other side is one too, a one-dimensional `Array` of numbers or a single number, and
    /// the elements combine as two numbers would, so the result's elements have the type
    /// the same operator gives two plain arrays, an `f64` for `/` of two integers. `+`, `-`,
    /// `*`, `/`, `//`, `%` and `**` are built, `&`, `|` and `^` of whole numbers (roadmap
    /// 3.24.1), and `&`, `|` and `^` of two masks of `bool`s. The shifts are not, since
    /// plain arrays do not have them yet.
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
        if matches!(
            operator,
            Operator::BitAnd | Operator::BitOr | Operator::BitXor
        ) && let Some(mask) = self.optional_mask_operation(left, right)
        {
            return mask;
        }
        let bits = matches!(
            operator,
            Operator::BitAnd | Operator::BitOr | Operator::BitXor
        );
        if bits && (self.is_mask(left) || self.is_mask(right)) {
            // A mask with anything but a mask: not whole numbers, and not built.
            return self.optional_array_unbuilt(node, &format!("`{symbol}`"));
        }
        if !matches!(
            operator,
            Operator::Add
                | Operator::Subtract
                | Operator::Multiply
                | Operator::Divide
                | Operator::FloorDivide
                | Operator::Modulo
                | Operator::Power
                | Operator::BitAnd
                | Operator::BitOr
                | Operator::BitXor
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
    /// nullable columns compare. `==`, `!=`, `<`, `<=`, `>` and `>=` are built, between
    /// numbers, between text, and `==` and `!=` between `bool`s, which have no order; `in`
    /// and `is` are not.
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
        let boolean = Type::new(self.db, TypeKind::Primitive(Primitive::Bool));
        let text = |ty: Type<'db>| {
            self.is_text_array(self.without_gaps(ty)) || matches!(ty.kind(self.db), TypeKind::Str)
        };
        if text(left) && text(right) {
            return Type::new(self.db, TypeKind::OptionalArray(boolean));
        }
        let operand = |ty: Type<'db>| {
            let plain = self.without_gaps(ty);
            match plain.kind(self.db) {
                TypeKind::Array { element, rank: 1 } if *element == boolean => Some(plain),
                _ if ty == boolean => Some(ty),
                _ => self.gap_operand(ty),
            }
        };
        let (Some(first), Some(second)) = (operand(left), operand(right)) else {
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
                    "an array with missing values compares with another, with an `Array` of \
                     one dimension, or with one value, of numbers, of text or of `bool`s",
                ),
            );
        };
        match self.array_comparison(node, operator, first, second) {
            Some(_) => Type::new(self.db, TypeKind::OptionalArray(boolean)),
            None => self.lowerer.unknown(),
        }
    }

    /// `-values` or `+values` at `node` on the array with missing values `operand`, whose
    /// elements must be numbers: an array of the same type, `None` where `operand` is. `~`
    /// takes a mask of `bool`s and gives one, or whole numbers, each with its bits flipped
    /// (roadmap 3.24.1).
    pub(super) fn optional_array_unary(
        &mut self,
        node: NodeId,
        operator: Operator,
        operand: Type<'db>,
    ) -> Type<'db> {
        let symbol = operator.symbol();
        if operator == Operator::Invert && self.is_mask(operand) {
            return operand;
        }
        if operator == Operator::Invert {
            let whole = matches!(
                self.without_gaps(operand).kind(self.db),
                TypeKind::Array { element, .. } if matches!(
                    element.kind(self.db),
                    TypeKind::Primitive(Primitive::Integer(_))
                )
            );
            if whole {
                return operand;
            }
            let message = format!("`~` does not apply to `{}`", self.show(operand));
            let label = "its elements are not whole numbers or `bool`s";
            return self.lowerer.error(node, &message, label, None);
        }
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

    /// Whether `ty` is a one-dimensional array of `bool`s, with or without missing values.
    fn is_mask(&self, ty: Type<'db>) -> bool {
        let boolean = |element: Type<'db>| {
            matches!(element.kind(self.db), TypeKind::Primitive(Primitive::Bool))
        };
        match ty.kind(self.db) {
            TypeKind::OptionalArray(element) | TypeKind::Array { element, rank: 1 } => {
                boolean(*element)
            }
            _ => false,
        }
    }

    /// The type of `left & right`, `left | right` or `left ^ right` where a side is a mask
    /// that may hold `None` and the other is one too or an `Array[bool, 1]`: an
    /// `Array[bool | None, 1]`, combined by the rules pandas uses, where `False & None` is
    /// `False` and `True | None` is `True`. `None` for any other sides, which are not built.
    fn optional_mask_operation(&self, left: Type<'db>, right: Type<'db>) -> Option<Type<'db>> {
        let boolean = Type::new(self.db, TypeKind::Primitive(Primitive::Bool));
        (self.is_mask(left) && self.is_mask(right))
            .then(|| Type::new(self.db, TypeKind::OptionalArray(boolean)))
    }

    /// `ty` with its missing values left out of the type: the `Array[T, 1]` of an
    /// `Array[T | None, 1]`, which for text is the text array, and any other type as it is.
    fn without_gaps(&self, ty: Type<'db>) -> Type<'db> {
        match ty.kind(self.db) {
            TypeKind::OptionalArray(element) if matches!(element.kind(self.db), TypeKind::Str) => {
                Type::new(self.db, TypeKind::TextArray)
            }
            TypeKind::OptionalArray(element) => Type::new(
                self.db,
                TypeKind::Array {
                    element: *element,
                    rank: 1,
                },
            ),
            _ => ty,
        }
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
