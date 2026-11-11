//! Whole-array arithmetic and the matrix product (language reference, "Arrays"):
//!
//! - Arithmetic works element by element on two arrays, or on an array and a number; the number
//!   mixing rules apply to the elements. Two arrays broadcast as `NumPy`'s do (roadmap 3.9): the
//!   result has the higher rank of the two, and for `Fixed` arrays each dimension, lined up from
//!   the last, must have the same length in both or 1 in one, which is checked here. `Array`
//!   lengths are checked when the line runs.
//! - `a @ b` is the matrix product of two rank-2 arrays, of a matrix and a vector, or of a
//!   vector and a matrix.

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
                let (a, b, rank) = (*a, *b, *m.max(n));
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
                let Some(shape) = broadcast(s, t) else {
                    return Some(self.shape_error(node, symbol, left, right));
                };
                let (a, b) = (*a, *b);
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

    /// `a @ b`: the matrix product of two matrices, of a matrix and a vector, or of a vector
    /// and a matrix.
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
                Array {
                    element: a,
                    rank: 1,
                },
                Array {
                    element: b,
                    rank: 2,
                },
            ) => {
                let (a, b) = (*a, *b);
                let element = self.combine(node, Operator::Multiply, a, b);
                Some(Array { element, rank: 1 })
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
            ) if s.rank() <= 2
                && t.rank() <= 2
                && s.rank() + t.rank() >= 3
                && s.lengths()[s.rank() - 1] == t.lengths()[0] =>
            {
                let mut lengths = s.lengths()[..s.rank() - 1].to_vec();
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
        let fixed = |ty: Type<'db>| matches!(ty.kind(self.db), TypeKind::Fixed { .. });
        let array = |ty: Type<'db>| matches!(ty.kind(self.db), TypeKind::Array { .. });
        let hint = if symbol == "@" {
            "`@` multiplies a matrix by a matrix or a vector whose lengths match"
        } else if fixed(left) && fixed(right) {
            "two `Fixed` arrays combine when, in each dimension counted from the last, their lengths are equal or one of them is 1"
        } else if (fixed(left) || array(left)) && (fixed(right) || array(right)) {
            "an `Array` and a `Fixed` array do not combine"
        } else {
            "combine two arrays, or an array and a single number"
        };
        self.lowerer
            .error(node, &message, "shapes do not match", Some(hint))
    }
}

/// The shape two `Fixed` shapes broadcast to, as `NumPy` broadcasts: lined up from the last
/// dimension, each pair of lengths is equal or one of them is 1, and a missing dimension counts
/// as 1. `None` when they do not broadcast.
fn broadcast(left: &Shape, right: &Shape) -> Option<Shape> {
    let (left, right) = (left.lengths(), right.lengths());
    let rank = left.len().max(right.len());
    let length = |lengths: &[u64], dimension: usize| {
        (dimension + lengths.len())
            .checked_sub(rank)
            .map_or(1, |position| lengths[position])
    };
    let mut lengths = Vec::with_capacity(rank);
    for dimension in 0..rank {
        let (a, b) = (length(left, dimension), length(right, dimension));
        lengths.push(match (a, b) {
            _ if a == b => a,
            (1, other) | (other, 1) => other,
            _ => return None,
        });
    }
    Shape::new(lengths)
}
