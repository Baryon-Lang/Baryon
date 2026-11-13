//! Array methods (language reference, "Methods" in the arrays chapter): `a.copy()`, `a.sum()`,
//! `a.min()`, `a.max()`, `a.dot(b)`, `a.round()` and `a.fill(value)` on `Array` and `Fixed`
//! arrays.
//!
//! `a.copy()` is the whole-array expression whose only leaf is `a`, so array expansion allocates
//! the new array and copies each element in the same loop nest as `a + b`. `a.sum()` and
//! `a.dot(b)` of floats call the runtime's pairwise sum and dot product, so both add in
//! `NumPy`'s order; of integers, they add into an accumulator in a loop nest of their own, as
//! does `v.dot(w)` of a `Fixed` vector shorter than 8, for which the pairwise order is the plain
//! one. Each integer `+` and `*` is checked for overflow as the operators are. `a.min()` and
//! `a.max()` scan in row-major order after checking the array is not empty.
//! The same three along one axis, `a.sum(axis=k)`, are in the `axes` module, and `a.mean()`,
//! `a.var()` and `a.std()` in the `statistics` module.
//! `a.round()` copies an `Array` and rounds each element in place; on a `Fixed` array it builds
//! the new value from each element rounded, with one call per element, since a `Fixed` array is
//! small and its length is known when compiling. `a.fill(value)` writes every element.

use crate::builder::Builder;
use mir::{
    Argument, BinaryOperator, Builtin, Callee, Check, Constant, ConstantValue, Elementwise, Local,
    MathFunction, Operand, Place, Projection, Rvalue, TerminatorKind,
};
use syntax::NodeId;
use types::TypeKind;

/// The length below which the pairwise order of a float sum is the plain one, element by
/// element from the first: the runtime's tree adds fewer than 8 values one at a time.
const PAIRWISE_LANES: u64 = 8;

impl<'db> Builder<'db> {
    /// The method `method` of an `Array` or `Fixed` array, called with `arguments`, the
    /// receiver first, into `place`. Reports any method or receiver it does not build.
    pub(crate) fn array_method(
        &mut self,
        place: Place,
        method: &str,
        arguments: &[NodeId],
        node: NodeId,
    ) {
        let fixed = arguments
            .first()
            .is_some_and(|&array| matches!(self.ty(array).kind(self.db), TypeKind::Fixed { .. }));
        match (method, arguments) {
            ("copy", [array]) if !fixed => self.array_copy(place, *array, node),
            ("sum", [array]) => self.array_sum(place, *array, node),
            ("min" | "max", [array]) => self.array_extreme(place, method == "min", *array, node),
            ("sum" | "min" | "max", [array, axis]) => {
                self.axis_reduction(place, method, (*array, *axis), node);
            }
            ("mean" | "var" | "std", _) => {
                self.array_statistic(place, method, arguments, node);
            }
            ("dot", [left, right]) => self.array_dot(place, (*left, *right), node),
            ("argsort" | "unique" | "nunique", [array]) if !fixed => {
                self.array_compared(place, method, *array, node);
            }
            ("round", [array]) if fixed => self.fixed_round(place, *array, node),
            ("round", [array]) => self.array_round(place, *array, node),
            ("fill", [array, value]) => self.array_fill(place, (*array, *value), node),
            ("tofile", [array, path]) if !fixed => self.array_tofile(place, *array, *path, node),
            _ => self.unsupported(node, &format!("the array method `{method}`")),
        }
    }

    /// `a.copy()` on an `Array`, into `place`: a new array with `a`'s lengths and elements.
    pub(crate) fn array_copy(&mut self, place: Place, array: NodeId, node: NodeId) {
        let source = self.borrow(array);
        let tree = Elementwise::Leaf(Operand::Copy(source));
        self.assign(place, Rvalue::Elementwise(Box::new(tree)), node);
    }

    /// `a.sum()` on an `Array` or `Fixed` array of numbers, into `place`.
    ///
    /// Floats are added in `NumPy`'s pairwise tree, by a call of [`Builtin::Sum`], so the result
    /// has the same bits as `numpy.sum` and rounding error grows only with the logarithm of the
    /// length (Collins, 2026-11-08, roadmap I.22). Integers are added in a loop nest from 0 in
    /// row-major order (decision 0008), since integer addition is exact in any order; each
    /// addition is checked for overflow as `+` is, so a debug build panics and a release build
    /// wraps (decision 0030). An empty array sums to 0.
    pub(crate) fn array_sum(&mut self, place: Place, array: NodeId, node: NodeId) {
        let ty = self.ty(array);
        let element = self.element(ty);
        if !matches!(element.kind(self.db), TypeKind::Primitive(_)) {
            return self.unsupported(node, "`sum` of an array whose elements are not numbers");
        }
        // A complex sum must add in `NumPy`'s pairwise tree, as a float sum does, which the
        // runtime does not do for complex numbers yet; adding in a loop would give other bits.
        if self.is_complex(element) {
            return self.unsupported(node, "`sum` of an array of complex numbers");
        }
        let source = self.borrow(array);
        let result = self.ty(node);
        self.sum_of(place, (&source, ty, result), node);
    }

    /// The sum of the array at `source`, of type `ty`, into `place` as type `result`, as
    /// [`array_sum`](Self::array_sum) describes; the caller has checked the elements are real
    /// numbers. `a.mean()` and `a.var()` sum through here too.
    pub(crate) fn sum_of(
        &mut self,
        place: Place,
        (source, ty, result): (&Place, types::Type<'db>, types::Type<'db>),
        node: NodeId,
    ) {
        let element = self.element(ty);
        if self.is_float(element) {
            let total = self.temporary(element, node);
            let next = self.new_block();
            self.terminate(
                TerminatorKind::Call {
                    callee: Callee::Builtin(Builtin::Sum),
                    arguments: vec![Argument::Borrow(source.clone())],
                    destination: Place::from(total),
                    target: next,
                    error: None,
                },
                node,
            );
            self.start(next);
            let rvalue = self.coerce(Operand::Copy(Place::from(total)), element, result);
            return self.assign(place, rvalue, node);
        }
        let lengths: Vec<Operand<'db>> = (0..self.rank(ty))
            .map(|dimension| self.length(source, ty, dimension, node))
            .collect();
        let total = self.temporary(element, node);
        let zero = Operand::Constant(self.number_constant(element, 0));
        self.assign(Place::from(total), Rvalue::Use(zero), node);
        self.loop_nest(&lengths, &mut Vec::new(), node, &mut |builder, indices| {
            let value = source.clone().project(Projection::Index(indices.to_vec()));
            let operands = (Operand::Copy(Place::from(total)), Operand::Copy(value));
            builder.arithmetic(
                Place::from(total),
                BinaryOperator::Add,
                operands,
                element,
                node,
            );
        });
        let rvalue = self.coerce(Operand::Copy(Place::from(total)), element, result);
        self.assign(place, rvalue, node);
    }

    /// `a.dot(b)` on two rank-1 `Array` or `Fixed` arrays of numbers, into `place`: the sum of
    /// the products of the elements at each index. Two `Array`s must have the same length, or
    /// the program panics; two `Fixed` vectors have it by their type.
    ///
    /// Floats call [`Builtin::Dot`], which adds the products in the pairwise order of
    /// [`array_sum`](Self::array_sum), so `a.dot(b)` has the bits of `(a * b).sum()`. A `Fixed`
    /// vector shorter than 8, such as a position, adds in a loop here instead: the pairwise tree
    /// adds fewer than 8 values one at a time, so the bits are the same and the hot loops of a
    /// simulation make no call. Integers add in a loop, since integer addition is exact, with each
    /// `*` and `+` checked for overflow as the operators are (decision 0030).
    pub(crate) fn array_dot(
        &mut self,
        place: Place,
        (left, right): (NodeId, NodeId),
        node: NodeId,
    ) {
        let ty = self.ty(left);
        let element = self.element(ty);
        let float_parameter = self.is_float_parameter(element);
        if !float_parameter && !matches!(element.kind(self.db), TypeKind::Primitive(_)) {
            return self.unsupported(node, "`dot` of vectors whose elements are not numbers");
        }
        let (left, right) = (self.borrow(left), self.borrow(right));
        let length = self.length(&left, ty, 0, node);
        if matches!(ty.kind(self.db), TypeKind::Array { .. }) {
            let other = self.length(&right, ty, 0, node);
            self.same_length(0, length.clone(), other, node);
        }
        let short = match ty.kind(self.db) {
            TypeKind::Fixed { shape, .. } => shape.elements() < PAIRWISE_LANES,
            _ => false,
        };
        if (self.is_float(element) || float_parameter) && !short {
            return self.then(node, |target| TerminatorKind::Call {
                callee: Callee::Builtin(Builtin::Dot),
                arguments: vec![Argument::Borrow(left), Argument::Borrow(right)],
                destination: place,
                target,
                error: None,
            });
        }
        let total = self.temporary(element, node);
        let zero = Operand::Constant(self.number_constant(element, 0));
        self.assign(Place::from(total), Rvalue::Use(zero), node);
        let product = self.temporary(element, node);
        self.loop_nest(&[length], &mut Vec::new(), node, &mut |builder, indices| {
            let at = |place: &Place| {
                Operand::Copy(place.clone().project(Projection::Index(indices.to_vec())))
            };
            let factors = (at(&left), at(&right));
            builder.arithmetic(
                Place::from(product),
                BinaryOperator::Multiply,
                factors,
                element,
                node,
            );
            let terms = (
                Operand::Copy(Place::from(total)),
                Operand::Copy(Place::from(product)),
            );
            builder.arithmetic(
                Place::from(total),
                BinaryOperator::Add,
                terms,
                element,
                node,
            );
        });
        self.assign(place, Rvalue::Use(Operand::Copy(Place::from(total))), node);
    }

    /// Panics, through a [`Check::SameLength`], unless `expected` and `found`, two arrays'
    /// lengths in `dimension`, are equal.
    pub(crate) fn same_length(
        &mut self,
        dimension: u32,
        expected: Operand<'db>,
        found: Operand<'db>,
        node: NodeId,
    ) {
        let equal = self.temporary(self.boolean_type(), node);
        self.assign(
            Place::from(equal),
            Rvalue::Binary(BinaryOperator::Equal, expected.clone(), found.clone()),
            node,
        );
        self.then(node, |target| TerminatorKind::Assert {
            condition: Operand::Copy(Place::from(equal)),
            check: Check::SameLength {
                dimension,
                expected,
                found,
            },
            target,
        });
    }

    /// `a.min()` (`smallest`) or `a.max()` on an `Array` or `Fixed` array of numbers, and
    /// `min(a)` or `max(a)` of one too or of a list of numbers (roadmap 1.13.13), into `place`:
    /// the smallest or largest element, scanning in row-major order. An empty array or list
    /// panics, through a [`Check::NotEmpty`], since it has no smallest element. A float NaN
    /// anywhere gives NaN, as `numpy.min` does: a NaN element replaces the best so far, and
    /// nothing compares smaller or larger than a NaN to replace it. A list follows the same
    /// rule rather than Python's, whose answer depends on where the NaN is (Collins,
    /// 2026-11-10).
    pub(crate) fn array_extreme(
        &mut self,
        place: Place,
        smallest: bool,
        array: NodeId,
        node: NodeId,
    ) {
        let ty = self.ty(array);
        let (element, rank, list) = match ty.kind(self.db) {
            TypeKind::List(element) => (*element, 1, true),
            _ => (self.element(ty), self.rank(ty), false),
        };
        if !matches!(element.kind(self.db), TypeKind::Primitive(_)) {
            let name = if smallest { "min" } else { "max" };
            let sequence = if list { "a list" } else { "an array" };
            return self.unsupported(
                node,
                &format!("`{name}` of {sequence} whose elements are not numbers"),
            );
        }
        let source = self.borrow(array);
        let lengths: Vec<Operand<'db>> = (0..rank)
            .map(|dimension| self.length(&source, ty, dimension, node))
            .collect();
        if list || matches!(ty.kind(self.db), TypeKind::Array { .. }) {
            self.not_empty(&lengths, list, node);
        }
        let first: Vec<Local> = lengths.iter().map(|_| self.zero_index(node)).collect();
        let best = self.temporary(element, node);
        let start = Operand::Copy(source.clone().project(Projection::Index(first)));
        self.assign(Place::from(best), Rvalue::Use(start), node);
        let test = if smallest {
            BinaryOperator::Less
        } else {
            BinaryOperator::Greater
        };
        let float = self.is_float(element);
        let scratch = (
            self.temporary(element, node),
            self.temporary(self.boolean_type(), node),
            self.temporary(self.boolean_type(), node),
        );
        self.loop_nest(&lengths, &mut Vec::new(), node, &mut |builder, indices| {
            let at = source.clone().project(Projection::Index(indices.to_vec()));
            builder.keep_extreme(Place::from(best), at, (test, float), scratch, node);
        });
        self.assign(place, Rvalue::Use(Operand::Copy(Place::from(best))), node);
    }

    /// Replaces `best` with the element at `at` when the element is smaller, for `test`
    /// [`Less`](BinaryOperator::Less), or larger, for [`Greater`](BinaryOperator::Greater), or,
    /// when `float`, a NaN, so a NaN anywhere wins as in `numpy.min`. `scratch` is a local of
    /// the element type and two `bool` locals the comparison is worked out in.
    pub(crate) fn keep_extreme(
        &mut self,
        best: Place,
        at: Place,
        (test, float): (BinaryOperator, bool),
        (value, better, missing): (Local, Local, Local),
        node: NodeId,
    ) {
        let copy = |local: Local| Operand::Copy(Place::from(local));
        self.assign(Place::from(value), Rvalue::Use(Operand::Copy(at)), node);
        self.assign(
            Place::from(better),
            Rvalue::Binary(test, copy(value), Operand::Copy(best.clone())),
            node,
        );
        if float {
            // `x != x` holds only for NaN.
            self.assign(
                Place::from(missing),
                Rvalue::Binary(BinaryOperator::NotEqual, copy(value), copy(value)),
                node,
            );
            self.assign(
                Place::from(better),
                Rvalue::Binary(BinaryOperator::BitOr, copy(better), copy(missing)),
                node,
            );
        }
        let (replace, join) = (self.new_block(), self.new_block());
        self.terminate(
            TerminatorKind::Branch {
                condition: copy(better),
                then: replace,
                otherwise: join,
            },
            node,
        );
        self.start(replace);
        self.assign(best, Rvalue::Use(copy(value)), node);
        self.terminate(TerminatorKind::Goto(join), node);
        self.start(join);
    }

    /// Panics, through a [`Check::NotEmpty`] reported at `node`, when any of `lengths` is 0;
    /// `list` says whether the sequence is a list, for the panic message.
    pub(crate) fn not_empty(&mut self, lengths: &[Operand<'db>], list: bool, node: NodeId) {
        let int = self.int_type();
        let size = self.temporary(int, node);
        let one = Operand::Constant(Constant::integer(int, 1));
        self.assign(Place::from(size), Rvalue::Use(one), node);
        for length in lengths {
            self.assign(
                Place::from(size),
                Rvalue::Binary(
                    BinaryOperator::Multiply,
                    Operand::Copy(Place::from(size)),
                    length.clone(),
                ),
                node,
            );
        }
        let any = self.temporary(self.boolean_type(), node);
        self.assign(
            Place::from(any),
            Rvalue::Binary(
                BinaryOperator::Greater,
                Operand::Copy(Place::from(size)),
                Operand::Constant(Constant::integer(int, 0)),
            ),
            node,
        );
        self.then(node, |target| TerminatorKind::Assert {
            condition: Operand::Copy(Place::from(any)),
            check: Check::NotEmpty { list },
            target,
        });
    }

    /// A new `i64` local holding 0, an index into the first element of a dimension.
    pub(crate) fn zero_index(&mut self, node: NodeId) -> Local {
        let int = self.int_type();
        let local = self.temporary(int, node);
        let zero = Operand::Constant(Constant::integer(int, 0));
        self.assign(Place::from(local), Rvalue::Use(zero), node);
        local
    }

    /// `a.round()` on an `Array`, into `place`: a copy of `a`, then each float element rounded
    /// in place to the nearest whole number, ties to even, still a float. Integers are already
    /// whole, so their copy is the result.
    pub(crate) fn array_round(&mut self, place: Place, array: NodeId, node: NodeId) {
        let ty = self.ty(array);
        let element = self.element(ty);
        let copy = self.temporary(ty, node);
        self.array_copy(Place::from(copy), array, node);
        if self.is_float(element) {
            let lengths: Vec<Operand<'db>> = (0..self.rank(ty))
                .map(|dimension| self.length(&Place::from(copy), ty, dimension, node))
                .collect();
            self.loop_nest(&lengths, &mut Vec::new(), node, &mut |builder, indices| {
                let at = Place::from(copy).project(Projection::Index(indices.to_vec()));
                builder.math_call(
                    at.clone(),
                    MathFunction::RoundEven,
                    vec![Operand::Copy(at)],
                    node,
                );
            });
        }
        let moved = self.take(Place::from(copy), ty);
        self.assign(place, Rvalue::Use(moved), node);
    }

    /// `a.fill(value)` on an `Array` or `Fixed` array that can be changed: `value`, converted
    /// to the element type once, written to every element in row-major order. The call's own
    /// value, `None`, goes to `place`.
    pub(crate) fn array_fill(
        &mut self,
        place: Place,
        (array, value): (NodeId, NodeId),
        node: NodeId,
    ) {
        let ty = self.ty(array);
        let element = self.element(ty);
        let value = self.operand_as(value, element);
        let Some(target) = self.changed_place(array) else {
            return;
        };
        let lengths: Vec<Operand<'db>> = (0..self.rank(ty))
            .map(|dimension| self.length(&target, ty, dimension, node))
            .collect();
        self.loop_nest(&lengths, &mut Vec::new(), node, &mut |builder, indices| {
            let at = target.clone().project(Projection::Index(indices.to_vec()));
            builder.assign(at, Rvalue::Use(value.clone()), node);
        });
        let nothing = self.ty(node);
        self.assign(
            place,
            Rvalue::Use(Operand::Constant(Constant {
                ty: nothing,
                value: ConstantValue::None,
            })),
            node,
        );
    }

    /// `v.round()` on a `Fixed` array of floats, into `place`: each element rounded to the
    /// nearest whole number, ties to even, still a float. On integers it is a copy.
    pub(crate) fn fixed_round(&mut self, place: Place, array: NodeId, node: NodeId) {
        let ty = self.ty(array);
        let TypeKind::Fixed { element, shape } = ty.kind(self.db) else {
            return self.unsupported(node, "`round` of this array");
        };
        let (element, lengths) = (*element, shape.lengths().to_vec());
        let source = self.borrow(array);
        if !self.is_float(element) {
            return self.assign(place, Rvalue::Use(Operand::Copy(source)), node);
        }
        let int = self.int_type();
        let mut rounded = Vec::new();
        let mut position = vec![0u64; lengths.len()];
        while position
            .iter()
            .zip(&lengths)
            .all(|(index, length)| index < length)
        {
            let indices = position
                .iter()
                .map(|&index| {
                    let local = self.temporary(int, node);
                    let constant = Constant::integer(int, i128::from(index));
                    self.assign(
                        Place::from(local),
                        Rvalue::Use(Operand::Constant(constant)),
                        node,
                    );
                    local
                })
                .collect();
            let value = Operand::Copy(source.clone().project(Projection::Index(indices)));
            let result = self.temporary(element, node);
            self.math_call(
                Place::from(result),
                MathFunction::RoundEven,
                vec![value],
                node,
            );
            rounded.push(Operand::Copy(Place::from(result)));
            // The next index in row-major order: the last dimension counts fastest.
            for dimension in (0..lengths.len()).rev() {
                position[dimension] += 1;
                if position[dimension] < lengths[dimension] || dimension == 0 {
                    break;
                }
                position[dimension] = 0;
            }
        }
        self.assign(place, Rvalue::Aggregate(ty, rounded), node);
    }
}
