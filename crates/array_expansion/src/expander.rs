//! Expands one whole-array statement into its loop nest.

mod broadcast;
mod summed;

pub(crate) use summed::summed;

use crate::leaves::{ArrayLeaf, fused};
use crate::place_types::{Shape, Types};
use broadcast::{Broadcast, Step};
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
        let mut broadcast = Broadcast::default();
        // Each fused subexpression checks its own operands first, where its statement was.
        for (subexpression, span) in fused(tree) {
            self.fused_lengths(subexpression, span, &mut broadcast);
        }
        let into = in_place.then(|| {
            let rank = match result_type.kind(self.db) {
                TypeKind::Array { rank, .. } => *rank,
                _ => 0,
            };
            (destination, self.view_lengths(destination, rank))
        });
        let lengths = self.broadcast(tree, into, &mut broadcast);
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
        let fill = |this: &mut Self, scaled: bool| {
            this.loop_nest(&lengths, |this, indices| {
                let (value, _) = this.element(tree, leaves, indices, (&broadcast, scaled));
                let element = target.clone().project(Projection::Index(indices.to_vec()));
                this.assign(element, Rvalue::Use(value));
            });
        };
        self.versioned(&broadcast, fill);
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
        self.drop_moved(leaves);
        self.statements.extend(after);
        let current = self.current.index();
        self.body.blocks[current] = BasicBlock {
            statements: self.statements,
            terminator,
        };
    }

    /// Drops each `Array` leaf the expression moved, once: its last use was this expression.
    fn drop_moved(&mut self, leaves: &[ArrayLeaf<'db>]) {
        let mut dropped: Vec<&Place> = Vec::new();
        for leaf in leaves.iter().filter(|leaf| leaf.moved) {
            if !dropped.contains(&&leaf.place) && matches!(leaf.shape, Shape::Runtime(_)) {
                dropped.push(&leaf.place);
                self.push(StatementKind::Drop(leaf.place.clone()));
            }
        }
    }

    /// Writes `fill`'s loop nest once, when no operand can repeat, or twice behind a branch: the
    /// plain nest, when every operand of [`Broadcast::same`] has the result's lengths, and the
    /// broadcasting nest otherwise. `fill` is told which nest it writes. Continues after both.
    fn versioned(&mut self, broadcast: &Broadcast<'db>, fill: impl Fn(&mut Self, bool)) {
        let Some((first, rest)) = broadcast.same.split_first() else {
            return fill(self, false);
        };
        // Some operand may repeat: the plain nest when none does, else the broadcasting one.
        let mut none = *first;
        for same in rest {
            let both = self.local(self.bool());
            self.assign(
                Place::from(both),
                Rvalue::Binary(
                    BinaryOperator::BitAnd,
                    Operand::Copy(Place::from(none)),
                    Operand::Copy(Place::from(*same)),
                ),
            );
            none = both;
        }
        let (plain, repeating, join) = (self.new_block(), self.new_block(), self.new_block());
        self.finish(
            TerminatorKind::Branch {
                condition: Operand::Copy(Place::from(none)),
                then: plain,
                otherwise: repeating,
            },
            plain,
        );
        fill(self, false);
        self.finish(TerminatorKind::Goto(join), repeating);
        fill(self, true);
        self.finish(TerminatorKind::Goto(join), join);
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

    /// Computes one element of `tree` at `indices`, one per dimension of `tree`'s result, into
    /// temporaries, and returns the operand that holds it and its type. `indexing` is what
    /// broadcasting worked out, and whether this is the broadcasting nest, which multiplies the
    /// indices of operands that may repeat. Array leaves are read without a bounds check: each
    /// index is below its dimension's length, and every array was checked to fit those lengths.
    fn element(
        &mut self,
        tree: &Elementwise<'db>,
        leaves: &[ArrayLeaf<'db>],
        indices: &[Local],
        indexing: (&Broadcast<'db>, bool),
    ) -> (Operand<'db>, Type<'db>) {
        match tree {
            Elementwise::Leaf(operand) => {
                let place = match operand {
                    Operand::Copy(place) | Operand::Move(place) => place,
                    Operand::Constant(constant) => return (operand.clone(), constant.ty),
                };
                if let Some(leaf) = leaves.iter().find(|leaf| leaf.place == *place) {
                    let own = self.operand_indices(tree, indices, indexing);
                    let element = place.clone().project(Projection::Index(own));
                    return (Operand::Copy(element), leaf.element);
                }
                let ty = self.place_type(place).unwrap_or(self.i64);
                (Operand::Copy(place.clone()), ty)
            }
            Elementwise::Unary(operator, operand) => {
                let (value, ty) = self.element(operand, leaves, indices, indexing);
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
                let own = self.operand_indices(tree, indices, indexing);
                let outer = std::mem::replace(&mut self.span, *span);
                let element = self.element(subexpression, leaves, &own, indexing);
                self.span = outer;
                element
            }
            Elementwise::Binary(operator, left, right) => {
                let (left, left_type) = self.element(left, leaves, indices, indexing);
                let (right, right_type) = self.element(right, leaves, indices, indexing);
                let ty = self.common(left_type, right_type);
                let left = self.convert(left, left_type, ty);
                let right = self.convert(right, right_type, ty);
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

    /// The type two operands of different number types meet in, as `NumPy` promotes them: a
    /// complex type over a real one, a float over an integer, the wider of two of one kind. Lowering
    /// converts a scalar to the expression's element type, so this matters where an array of
    /// another element type meets it: `Array([1.0, 2.0]) + 0j` adds in `complex128`, and an
    /// integer array plus `1.5` in `f64`. The left type when neither is a number.
    fn common(&self, left: Type<'db>, right: Type<'db>) -> Type<'db> {
        let rank = |ty: Type<'db>| match ty.kind(self.db) {
            TypeKind::Primitive(Primitive::Complex(complex)) => Some((2, complex.part().bits())),
            TypeKind::Primitive(Primitive::Float(float)) => Some((1, float.bits())),
            TypeKind::Primitive(Primitive::Integer(integer)) => Some((0, integer.bits())),
            _ => None,
        };
        match (rank(left), rank(right)) {
            (Some(a), Some(b)) if b > a => right,
            _ => left,
        }
    }

    /// `value`, of type `from`, converted to `to` when the two differ, into a new temporary.
    fn convert(&mut self, value: Operand<'db>, from: Type<'db>, to: Type<'db>) -> Operand<'db> {
        if from == to {
            return value;
        }
        let converted = self.local(to);
        self.assign(Place::from(converted), Rvalue::Convert(value, to));
        Operand::Copy(Place::from(converted))
    }

    /// The indices of the operand at `node`, an array leaf or a fused subexpression, given the
    /// `indices` of the level it belongs to: its dimensions line up with the last of the level's,
    /// and each is indexed by its [`Step`].
    fn operand_indices(
        &mut self,
        node: &Elementwise<'db>,
        indices: &[Local],
        (broadcast, scaled): (&Broadcast<'db>, bool),
    ) -> Vec<Local> {
        let Some(steps) = broadcast.steps(node) else {
            return indices.to_vec();
        };
        let offset = indices.len().saturating_sub(steps.len());
        let mut own = Vec::with_capacity(steps.len());
        for (index, step) in indices[offset..].iter().zip(steps) {
            own.push(match *step {
                Step::Zero(zero) => zero,
                Step::Scaled(scale) if scaled => {
                    let repeated = self.local(self.i64);
                    self.assign(
                        Place::from(repeated),
                        Rvalue::Binary(
                            BinaryOperator::Multiply,
                            Operand::Copy(Place::from(*index)),
                            Operand::Copy(Place::from(scale)),
                        ),
                    );
                    repeated
                }
                Step::Same | Step::Scaled(_) => *index,
            });
        }
        own
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
