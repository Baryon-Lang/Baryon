//! Whole-array arithmetic and the matrix product (language reference, "Arrays"):
//!
//! - Arithmetic works element by element on two arrays of the same rank (`Array`) or shape
//!   (`Fixed`), or on an array and a number; the number mixing rules apply to the elements.
//! - `a @ b` is the matrix product of two rank-2 arrays, or of a matrix and a vector.

use super::Body;
use syntax::{NodeId, Operator};
use types::{Shape, Type, TypeKind};

impl<'db> Body<'db> {
    /// `left op right` where at least one side is an array, or `None` when neither is.
    pub(super) fn array_arithmetic(
        &mut self,
        node: NodeId,
        operator: Operator,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Option<Type<'db>> {
        use TypeKind::{Array, Fixed};
        let symbol = operator.symbol();
        if operator == Operator::MatrixMultiply {
            return Some(self.matrix_product(node, left, right));
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
        ) {
            return None;
        }
        let scalar = |ty: Type<'db>| matches!(ty.kind(self.db), TypeKind::Primitive(_));
        let result = match (left.kind(self.db), right.kind(self.db)) {
            (
                Array {
                    element: a,
                    rank: m,
                },
                Array {
                    element: b,
                    rank: n,
                },
            ) => {
                if m != n {
                    return Some(self.shape_error(node, symbol, left, right));
                }
                let (a, b, rank) = (*a, *b, *m);
                let element = self.combine(node, operator, a, b);
                Array { element, rank }
            }
            (
                Fixed {
                    element: a,
                    shape: s,
                },
                Fixed {
                    element: b,
                    shape: t,
                },
            ) => {
                if s != t {
                    return Some(self.shape_error(node, symbol, left, right));
                }
                let (a, b, shape) = (*a, *b, s.clone());
                let element = self.combine(node, operator, a, b);
                Fixed { element, shape }
            }
            (Array { element, rank }, _) if scalar(right) => {
                let (element, rank) = (*element, *rank);
                let element = self.combine(node, operator, element, right);
                Array { element, rank }
            }
            (_, Array { element, rank }) if scalar(left) => {
                let (element, rank) = (*element, *rank);
                let element = self.combine(node, operator, left, element);
                Array { element, rank }
            }
            (Fixed { element, shape }, _) if scalar(right) => {
                let (element, shape) = (*element, shape.clone());
                let element = self.combine(node, operator, element, right);
                Fixed { element, shape }
            }
            (_, Fixed { element, shape }) if scalar(left) => {
                let (element, shape) = (*element, shape.clone());
                let element = self.combine(node, operator, left, element);
                Fixed { element, shape }
            }
            (Array { .. } | Fixed { .. }, _) | (_, Array { .. } | Fixed { .. }) => {
                return Some(self.shape_error(node, symbol, left, right));
            }
            _ => return None,
        };
        Some(Type::new(self.db, result))
    }

    /// `a @ b`: the matrix product of two matrices, or of a matrix and a vector.
    fn matrix_product(&mut self, node: NodeId, left: Type<'db>, right: Type<'db>) -> Type<'db> {
        use TypeKind::{Array, Fixed};
        let product = match (left.kind(self.db), right.kind(self.db)) {
            (
                Array {
                    element: a,
                    rank: 2,
                },
                Array { element: b, rank },
            ) if *rank <= 2 => {
                let (a, b, rank) = (*a, *b, *rank);
                let element = self.combine(node, Operator::Multiply, a, b);
                Some(Array { element, rank })
            }
            (
                Fixed {
                    element: a,
                    shape: s,
                },
                Fixed {
                    element: b,
                    shape: t,
                },
            ) if s.rank() == 2 && t.rank() <= 2 && s.lengths()[1] == t.lengths()[0] => {
                let mut lengths = vec![s.lengths()[0]];
                lengths.extend_from_slice(&t.lengths()[1..]);
                let (a, b) = (*a, *b);
                let element = self.combine(node, Operator::Multiply, a, b);
                Shape::new(lengths).map(|shape| Fixed { element, shape })
            }
            _ => None,
        };
        match product {
            Some(kind) => Type::new(self.db, kind),
            None => self.shape_error(node, "@", left, right),
        }
    }

    /// Reports two operands whose ranks or shapes do not match.
    fn shape_error(
        &mut self,
        node: NodeId,
        symbol: &str,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Type<'db> {
        let message = format!(
            "`{symbol}` cannot combine `{}` and `{}`",
            self.show(left),
            self.show(right)
        );
        let hint = if symbol == "@" {
            "`@` multiplies a matrix by a matrix or a vector whose lengths match"
        } else {
            "both arrays need the same rank, or the same shape for `Fixed` arrays, or one side a single number"
        };
        self.lowerer
            .error(node, &message, "shapes do not match", Some(hint))
    }
}
