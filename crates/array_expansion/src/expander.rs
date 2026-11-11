//! Expands one whole-array statement into its loop nest.

use crate::leaves::{ArrayLeaf, fused, shape_leaves};
use crate::place_types::{Shape, Types};
use mir::{
    BasicBlock, BinaryOperator, BlockId, Body, Check, Constant, Elementwise, Local, LocalKind,
    Operand, Place, Projection, Rvalue, Statement, StatementKind, Terminator, TerminatorKind,
    UnaryOperator,
};
use source::Span;
use types::{Integer, Primitive, Type, TypeKind};

/// Writes the blocks of one expansion, one block at a time.
pub(crate) struct Expander<'a, 'b, 'db> {
    /// The database types are interned in.
    db: &'db dyn salsa::Database,
    /// How to work out the type of a place.
    types: &'a Types<'b, 'db>,
    /// The body being rewritten.
    body: &'a mut Body<'db>,
    /// The block being written.
    current: BlockId,
    /// The statements of the block being written, so far.
    statements: Vec<Statement<'db>>,
    /// The span of the statement being expanded, given to everything written.
    span: Span,
    /// The `i64` type, for lengths and indices.
    i64: Type<'db>,
}

impl<'a, 'b, 'db> Expander<'a, 'b, 'db> {
    /// An expander that continues writing block `current`, which already holds `statements`.
    pub(crate) fn new(
        db: &'db dyn salsa::Database,
        types: &'a Types<'b, 'db>,
        body: &'a mut Body<'db>,
        current: BlockId,
        statements: Vec<Statement<'db>>,
        span: Span,
    ) -> Self {
        let i64 = Type::new(db, TypeKind::Primitive(Primitive::Integer(Integer::I64)));
        Self {
            db,
            types,
            body,
            current,
            statements,
            span,
            i64,
        }
    }

    /// Writes `destination = tree` as a loop nest, then `after` and `terminator` in the block
    /// the nest exits to. `leaves` are the expression's array leaves from
    /// [`array_leaves`](crate::leaves::array_leaves);
    /// `result_type` is the destination's array type. The shape is the destination's when it is
    /// a view, which is written in place, and otherwise that of `leaves[0]`, which must exist.
    pub(crate) fn expand(
        mut self,
        destination: &Place,
        tree: &Elementwise<'db>,
        (leaves, result_type): (&[ArrayLeaf<'db>], Type<'db>),
        after: Vec<Statement<'db>>,
        terminator: Terminator<'db>,
    ) {
        let in_place = destination.is_view();
        // Each fused subexpression checks its own leaves first, where its statement was.
        for (subexpression, span) in fused(tree) {
            let outer = std::mem::replace(&mut self.span, span);
            let group = shape_leaves(self.types, &self.body.locals, subexpression);
            if let Some((first, rest)) = group.split_first() {
                let lengths = self.lengths(first);
                for leaf in rest {
                    self.check_lengths(leaf, &lengths);
                }
            }
            self.span = outer;
        }
        let group = shape_leaves(self.types, &self.body.locals, tree);
        let (lengths, checked) = if in_place {
            let rank = match result_type.kind(self.db) {
                TypeKind::Array { rank, .. } => *rank,
                _ => 0,
            };
            (self.view_lengths(destination, rank), &group[..])
        } else {
            (self.lengths(&leaves[0]), &group[1..])
        };
        // The view itself, as `+=` reads it, has its own lengths.
        for leaf in checked.iter().filter(|leaf| leaf.place != *destination) {
            self.check_lengths(leaf, &lengths);
        }
        let staged = in_place
            && leaves
                .iter()
                .any(|leaf| leaf.place.local == destination.local && leaf.place != *destination);
        let target = if in_place && !staged {
            destination.clone()
        } else {
            let result = self.local(result_type);
            if matches!(result_type.kind(self.db), TypeKind::Array { .. }) {
                self.assign(
                    Place::from(result),
                    Rvalue::NewArray(result_type, lengths.clone()),
                );
            }
            Place::from(result)
        };
        self.loop_nest(&lengths, |this, indices| {
            let (value, _) = this.element(tree, leaves, indices);
            let element = target.clone().project(Projection::Index(indices.to_vec()));
            this.assign(element, Rvalue::Use(value));
        });
        if staged {
            self.loop_nest(&lengths, |this, indices| {
                let from = target.clone().project(Projection::Index(indices.to_vec()));
                let to = destination
                    .clone()
                    .project(Projection::Index(indices.to_vec()));
                this.assign(to, Rvalue::Use(Operand::Copy(from)));
            });
            self.push(StatementKind::Drop(target));
        } else if !in_place {
            self.assign(destination.clone(), Rvalue::Use(Operand::Move(target)));
        }
        let mut dropped: Vec<&Place> = Vec::new();
        for leaf in leaves.iter().filter(|leaf| leaf.moved) {
            if !dropped.contains(&&leaf.place) && matches!(leaf.shape, Shape::Runtime(_)) {
                dropped.push(&leaf.place);
                self.push(StatementKind::Drop(leaf.place.clone()));
            }
        }
        self.statements.extend(after);
        let current = self.current.index();
        self.body.blocks[current] = BasicBlock {
            statements: self.statements,
            terminator,
        };
    }

    /// Writes a loop over every index below `lengths`, in row-major order with the last
    /// dimension innermost, whose body `body` writes given the index locals; continues after
    /// the loops.
    fn loop_nest(&mut self, lengths: &[Operand<'db>], body: impl FnOnce(&mut Self, &[Local])) {
        let indices: Vec<Local> = lengths.iter().map(|_| self.local(self.i64)).collect();
        let mut exits = Vec::new();
        for (index, length) in indices.iter().zip(lengths) {
            exits.push(self.open_loop(*index, length.clone()));
        }
        body(self, &indices);
        for (index, (header, exit)) in indices.iter().zip(exits).rev() {
            let next = Rvalue::Binary(
                BinaryOperator::Add,
                Operand::Copy(Place::from(*index)),
                Operand::Constant(Constant::integer(self.i64, 1)),
            );
            self.assign(Place::from(*index), next);
            self.finish(TerminatorKind::Goto(header), exit);
        }
    }

    /// The length of each of the `rank` dimensions of the view `view`, read into new locals.
    fn view_lengths(&mut self, view: &Place, rank: u32) -> Vec<Operand<'db>> {
        (0..rank)
            .map(|dimension| {
                let length = self.local(self.i64);
                self.assign(Place::from(length), Rvalue::Length(view.clone(), dimension));
                Operand::Copy(Place::from(length))
            })
            .collect()
    }

    /// The length of each dimension of `reference`: constants for a `Fixed`, otherwise read
    /// into new locals.
    fn lengths(&mut self, reference: &ArrayLeaf<'db>) -> Vec<Operand<'db>> {
        match &reference.shape {
            Shape::Fixed(lengths) => lengths
                .iter()
                .map(|&length| Operand::Constant(Constant::integer(self.i64, length.into())))
                .collect(),
            Shape::Runtime(rank) => (0..*rank)
                .map(|dimension| {
                    let length = self.local(self.i64);
                    self.assign(
                        Place::from(length),
                        Rvalue::Length(reference.place.clone(), dimension),
                    );
                    Operand::Copy(Place::from(length))
                })
                .collect(),
        }
    }

    /// Checks that each dimension of the `Array` leaf `leaf` has the length in `lengths`. A
    /// `Fixed` leaf needs no check: its type, which the checker matched, fixes its lengths.
    fn check_lengths(&mut self, leaf: &ArrayLeaf<'db>, lengths: &[Operand<'db>]) {
        if !matches!(leaf.shape, Shape::Runtime(_)) {
            return;
        }
        let bool = Type::new(self.db, TypeKind::Primitive(Primitive::Bool));
        for (dimension, expected) in (0u32..).zip(lengths) {
            let found = self.local(self.i64);
            self.assign(
                Place::from(found),
                Rvalue::Length(leaf.place.clone(), dimension),
            );
            let same = self.local(bool);
            self.assign(
                Place::from(same),
                Rvalue::Binary(
                    BinaryOperator::Equal,
                    Operand::Copy(Place::from(found)),
                    expected.clone(),
                ),
            );
            let next = self.new_block();
            self.finish(
                TerminatorKind::Assert {
                    condition: Operand::Copy(Place::from(same)),
                    check: Check::SameLength {
                        dimension,
                        expected: expected.clone(),
                        found: Operand::Copy(Place::from(found)),
                    },
                    target: next,
                },
                next,
            );
        }
    }

    /// Starts `for index in range(length)`: sets the index to zero and writes the loop header.
    /// Continues in the loop body; returns the header, which the body's end jumps back to, and
    /// the block the loop exits to.
    fn open_loop(&mut self, index: Local, length: Operand<'db>) -> (BlockId, BlockId) {
        self.assign(
            Place::from(index),
            Rvalue::Use(Operand::Constant(Constant::integer(self.i64, 0))),
        );
        let header = self.new_block();
        self.finish(TerminatorKind::Goto(header), header);
        let bool = Type::new(self.db, TypeKind::Primitive(Primitive::Bool));
        let inside = self.local(bool);
        self.assign(
            Place::from(inside),
            Rvalue::Binary(
                BinaryOperator::Less,
                Operand::Copy(Place::from(index)),
                length,
            ),
        );
        let body = self.new_block();
        let exit = self.new_block();
        self.finish(
            TerminatorKind::Branch {
                condition: Operand::Copy(Place::from(inside)),
                then: body,
                otherwise: exit,
            },
            body,
        );
        (header, exit)
    }

    /// Computes one element of `tree` at `indices`, into temporaries, and returns the operand
    /// that holds it and its type. Array leaves are read at the indices without a bounds check: each index is
    /// below its dimension's length, and every array was checked to have those lengths.
    fn element(
        &mut self,
        tree: &Elementwise<'db>,
        leaves: &[ArrayLeaf<'db>],
        indices: &[Local],
    ) -> (Operand<'db>, Type<'db>) {
        match tree {
            Elementwise::Leaf(operand) => {
                let place = match operand {
                    Operand::Copy(place) | Operand::Move(place) => place,
                    Operand::Constant(constant) => return (operand.clone(), constant.ty),
                };
                if let Some(leaf) = leaves.iter().find(|leaf| leaf.place == *place) {
                    let element = place.clone().project(Projection::Index(indices.to_vec()));
                    return (Operand::Copy(element), leaf.element);
                }
                let ty = self.place_type(place).unwrap_or(self.i64);
                (Operand::Copy(place.clone()), ty)
            }
            Elementwise::Unary(operator, operand) => {
                let (value, ty) = self.element(operand, leaves, indices);
                if *operator == UnaryOperator::Negate
                    && let TypeKind::Primitive(Primitive::Integer(integer)) = ty.kind(self.db)
                    && integer.is_signed()
                {
                    // `-x` is `0 - x`, which overflows only for the type's minimum.
                    let zero = Operand::Constant(Constant::integer(ty, 0));
                    let negated = self.checked(BinaryOperator::Subtract, (zero, value), ty);
                    return (negated, ty);
                }
                let result = self.local(ty);
                self.assign(Place::from(result), Rvalue::Unary(*operator, value));
                (Operand::Copy(Place::from(result)), ty)
            }
            Elementwise::Fused(subexpression, span) => {
                // Its overflow checks report where its statement was.
                let outer = std::mem::replace(&mut self.span, *span);
                let element = self.element(subexpression, leaves, indices);
                self.span = outer;
                element
            }
            Elementwise::Binary(operator, left, right) => {
                let (left, ty) = self.element(left, leaves, indices);
                let (right, _) = self.element(right, leaves, indices);
                let overflows = matches!(
                    operator,
                    BinaryOperator::Add | BinaryOperator::Subtract | BinaryOperator::Multiply
                );
                if overflows
                    && matches!(ty.kind(self.db), TypeKind::Primitive(Primitive::Integer(_)))
                {
                    return (self.checked(*operator, (left, right), ty), ty);
                }
                let ty = if is_comparison(*operator) {
                    Type::new(self.db, TypeKind::Primitive(Primitive::Bool))
                } else {
                    ty
                };
                let result = self.local(ty);
                self.assign(Place::from(result), Rvalue::Binary(*operator, left, right));
                (Operand::Copy(Place::from(result)), ty)
            }
        }
    }

    /// `left op right` for integer `+`, `-` or `*` on elements of type `ty`, checked for
    /// overflow as lowering checks the same operator on two integers (decision 0030): a
    /// [`CheckedBinary`](Rvalue::CheckedBinary) into a `tuple[T, bool]` local and an `Assert`
    /// that its flag is `False`, so a debug build panics at the expression and a release
    /// build's backend drops the check and keeps the wrapped result. Returns the result.
    fn checked(
        &mut self,
        operator: BinaryOperator,
        (left, right): (Operand<'db>, Operand<'db>),
        ty: Type<'db>,
    ) -> Operand<'db> {
        let bool = Type::new(self.db, TypeKind::Primitive(Primitive::Bool));
        let pair = self.local(Type::new(self.db, TypeKind::Tuple(vec![ty, bool])));
        self.assign(
            Place::from(pair),
            Rvalue::CheckedBinary(operator, left, right),
        );
        let fits = self.local(bool);
        let flag = Operand::Copy(Place::from(pair).project(Projection::Field(1)));
        self.assign(Place::from(fits), Rvalue::Unary(UnaryOperator::Not, flag));
        let next = self.new_block();
        self.finish(
            TerminatorKind::Assert {
                condition: Operand::Copy(Place::from(fits)),
                check: Check::NoOverflow(operator),
                target: next,
            },
            next,
        );
        Operand::Copy(Place::from(pair).project(Projection::Field(0)))
    }

    /// The type of the value at `place` in the body.
    fn place_type(&self, place: &Place) -> Option<Type<'db>> {
        let local = self.body.locals.get(place.local.index())?.ty;
        self.types.of(local, place)
    }

    /// A new temporary local of type `ty`.
    fn local(&mut self, ty: Type<'db>) -> Local {
        self.body.push_local(ty, LocalKind::Temporary, self.span)
    }

    /// Appends `place = rvalue` to the block being written.
    fn assign(&mut self, place: Place, rvalue: Rvalue<'db>) {
        self.push(StatementKind::Assign(place, rvalue));
    }

    /// Appends a statement to the block being written.
    fn push(&mut self, kind: StatementKind<'db>) {
        self.statements.push(Statement {
            kind,
            span: self.span,
        });
    }

    /// A new, empty block, to be written later.
    fn new_block(&mut self) -> BlockId {
        self.body.push_block(Terminator {
            kind: TerminatorKind::Unreachable,
            span: self.span,
        })
    }

    /// Ends the block being written with `terminator` and continues writing `next`.
    fn finish(&mut self, terminator: TerminatorKind<'db>, next: BlockId) {
        let current = self.current.index();
        self.body.blocks[current] = BasicBlock {
            statements: std::mem::take(&mut self.statements),
            terminator: Terminator {
                kind: terminator,
                span: self.span,
            },
        };
        self.current = next;
    }
}

/// Whether `operator` compares, giving a `bool` whatever its operands are.
fn is_comparison(operator: BinaryOperator) -> bool {
    matches!(
        operator,
        BinaryOperator::Equal
            | BinaryOperator::NotEqual
            | BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual
    )
}
