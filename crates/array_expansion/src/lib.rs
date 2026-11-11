//! Array expansion: turns each whole-array expression in a mid-level IR body into a loop nest.
//!
//! Lowering keeps `c = a + b * k` over arrays as one [`Elementwise`](mir::Rvalue::Elementwise)
//! rvalue, so array fusion (roadmap 3.1) can merge such trees before anything else sees them
//! (decision 0018). Backends never see one: this pass replaces each with plain IR that
//!
//! 1. reads the lengths of each array in the expression, or takes them from its type for a
//!    `Fixed` array;
//! 2. works out the result's lengths by broadcasting, as `NumPy` does (roadmap 3.9): arrays
//!    line up from their last dimension, and in each dimension the result has the one length
//!    other than 1 they have; it checks that every `Array` has, in each dimension, the result's
//!    length or 1, with a [`SameLength`](mir::Check::SameLength) check that names both;
//! 3. allocates the result with [`NewArray`](mir::Rvalue::NewArray), for an `Array` result;
//! 4. loops over every index in row-major order (decision 0008), the last dimension innermost,
//!    computing each element into temporaries and storing it; the reads need no bounds checks,
//!    since each index is below a length every array was checked to fit; integer `+`, `-`, `*`
//!    and `-x` on elements are checked for overflow as lowering checks them on two integers,
//!    so a debug build panics and a release build wraps (decision 0030). When an array of
//!    length 1 in some dimension may meet a longer one, which only the run can tell for an
//!    `Array`, the loop nest is written twice: the plain nest, which indexes every array by
//!    the loop indices, and the broadcasting nest, which multiplies the index of each array
//!    that may repeat by 0 or 1. A flag computed before the loops runs the plain nest unless
//!    some array repeats, so the common case keeps the loop the optimizer vectorizes best;
//! 5. moves the result into the destination, and drops each array the expression moved.
//!
//! A [`Fused`](mir::Elementwise::Fused) subexpression, a statement that array fusion moved into
//! this one, has its own array leaves broadcast and checked against each other first, in step
//! 2, reported at its statement's span, in the order the statements ran; in the enclosing
//! expression it then counts as one array with the lengths its leaves broadcast to. Its overflow
//! checks report at its statement's span too. The loop computes its elements with the rest, and no array is ever
//! built for it, which is the point of fusing it.
//!
//! The result goes to a new temporary and moves into the destination only after the loop, so
//! `a = a * 2.0`, which reads the array it replaces, is correct.
//!
//! A destination that is a view, `a[1:n] = b + c` or `a[1:n] += b`, is written in place
//! instead: the view's own lengths set the shape, every `Array` leaf is checked to have them or
//! 1 in each dimension, as `NumPy` checks an output, and nothing is allocated. The one exception is a leaf that may overlap the view, a place
//! of the same variable other than the view itself, as in `a[1:n] = a[0:n - 1]`: writing in
//! place would read elements already overwritten, so the result goes to a new array first and
//! is copied into the view after, as `NumPy` does.  A leaf that is the view itself, as `+=`
//! makes, reads each element just before writing it, so it needs no copy.
//!
//! It runs in the instance query after drop elaboration, so types are concrete. It is one
//! replaceable implementation (decision 0001): it reads and writes only `mir` types, and asks what
//! a struct declares through a callback, as [`types::layout`] does.
//!
//! # Example
//!
//! ```
//! use mir::{BinaryOperator, Body, Elementwise, Local, LocalKind, Operand, Phase, Rvalue,
//!     Statement, StatementKind, Terminator, TerminatorKind};
//! use source::{FileId, Span};
//! use types::{Float, Mode, Primitive, Type, TypeKind};
//!
//! # #[salsa::db]
//! # #[derive(Clone, Default)]
//! # struct Database { storage: salsa::Storage<Self> }
//! # #[salsa::db]
//! # impl salsa::Database for Database {}
//! # #[salsa::db]
//! # impl database::Db for Database {
//! #     fn parse(&self, _: database::File) -> &database::ParsedFile { unreachable!() }
//! #     fn resolve(&self, _: database::Package, _: database::File) -> &database::Resolution {
//! #         unreachable!()
//! #     }
//! # }
//! let database = Database::default();
//! let span = Span::new(FileId::from_index(0), 0, 1);
//! let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
//! let array = Type::new(&database, TypeKind::Array { element: f64, rank: 1 });
//!
//! // def add(a: Array[f64, 1], b: Array[f64, 1]) -> Array[f64, 1]: return a + b
//! let mut body = Body::new(array, span);
//! let borrowed = |name: &str| LocalKind::Parameter { name: name.into(), mode: Mode::Borrow };
//! let a = body.push_local(array, borrowed("a"), span);
//! let b = body.push_local(array, borrowed("b"), span);
//! let entry = body.push_block(Terminator { kind: TerminatorKind::Return, span });
//! body.blocks[entry.index()].statements.push(Statement {
//!     kind: StatementKind::Assign(
//!         Local::RETURN.into(),
//!         Rvalue::Elementwise(Box::new(Elementwise::Binary(
//!             BinaryOperator::Add,
//!             Box::new(Elementwise::Leaf(Operand::Copy(a.into()))),
//!             Box::new(Elementwise::Leaf(Operand::Copy(b.into()))),
//!         ))),
//!     ),
//!     span,
//! });
//!
//! array_expansion::expand(&database, &mut body, &|_| unreachable!());
//!
//! body.phase = Phase::Ready;
//! assert!(body.validate(&database).is_empty());
//! let text = body.display(&database).to_string();
//! // `b` fits the result's length, which is `a`'s unless that is 1.
//! assert!(text.contains("assert(copy _12, same length in dimension 0: copy _4 == copy _9)"));
//! // The plain nest, when neither repeats, then the broadcasting one.
//! assert!(text.contains("_21 = Add(copy _1[_19], copy _2[_19]);"));
//! assert!(text.contains("_26 = Add(copy _1[_24], copy _2[_25]);"));
//! ```

mod expander;
mod leaves;
mod place_types;

use expander::Expander;
use leaves::array_leaves;
use mir::{BlockId, Body, Rvalue, StatementKind, Terminator, TerminatorKind};

/// Replaces every whole-array expression in `body` with a loop nest, as the crate documentation
/// describes. `declared` says what a struct declares, with its type arguments substituted; it
/// is called for array leaves reached through a field, such as `atoms.positions`.
///
/// The bodies of the function's `prange` loops are expanded too.
///
/// An expression with no array leaf and a destination that is not a view, or whose destination
/// is not an array, is left as it is;
/// a type-checked body has neither, and [`Body::validate`] reports what is left in a ready body.
///
/// Cost: a scan of every statement, and for each expanded statement IR proportional to the size
/// of its expression and the rank of its arrays. The generated loops do the elementwise work at
/// run time, one pass over the arrays per expression.
pub fn expand<'db>(
    db: &'db dyn salsa::Database,
    body: &mut Body<'db>,
    declared: &dyn Fn(&types::Nominal<'db>) -> types::Declared<'db>,
) {
    for outlined in &mut body.outlined {
        expand(db, outlined, declared);
    }
    let types = place_types::Types { db, declared };
    let mut block = 0;
    while block < body.blocks.len() {
        let Some(found) = next_expandable(&types, body, block) else {
            block += 1;
            continue;
        };
        let (position, leaves, result_type) = found;
        let data = &mut body.blocks[block];
        let after = data.statements.split_off(position + 1);
        let Some(statement) = data.statements.pop() else {
            block += 1;
            continue;
        };
        let StatementKind::Assign(destination, Rvalue::Elementwise(tree)) = statement.kind else {
            block += 1;
            continue;
        };
        let before = std::mem::take(&mut data.statements);
        let terminator = std::mem::replace(
            &mut data.terminator,
            Terminator {
                kind: TerminatorKind::Unreachable,
                span: statement.span,
            },
        );
        Expander::new(
            db,
            &types,
            body,
            BlockId::new(block),
            before,
            statement.span,
        )
        .expand(
            &destination,
            &tree,
            (&leaves, result_type),
            after,
            terminator,
        );
        block += 1;
    }
}

/// The first whole-array statement in `block` that can be expanded: its position, its array
/// leaves and the destination's type.
fn next_expandable<'db>(
    types: &place_types::Types<'_, 'db>,
    body: &Body<'db>,
    block: usize,
) -> Option<(usize, Vec<leaves::ArrayLeaf<'db>>, types::Type<'db>)> {
    body.blocks[block]
        .statements
        .iter()
        .enumerate()
        .find_map(|(position, statement)| {
            let StatementKind::Assign(destination, Rvalue::Elementwise(tree)) = &statement.kind
            else {
                return None;
            };
            let local = body.locals.get(destination.local.index())?.ty;
            let result_type = types.of(local, destination)?;
            types.array(result_type)?;
            let leaves = array_leaves(types, &body.locals, tree);
            // A view takes a number alone, `a[1:n] = 0.0`; anything else needs an array leaf
            // for its shape.
            (!leaves.is_empty() || destination.is_view()).then_some((position, leaves, result_type))
        })
}

#[cfg(test)]
mod tests;
