//! Bounds-check hoisting: makes the index checks of a counted loop once, before it starts, and
//! runs a copy of the loop without them when they all pass (roadmap 3.2); and runs innermost
//! loops in a copy that skips multiplying by strides of 1 (roadmap 3.3.1).
//!
//! Lowering checks every index, `a[i]`, against its dimension's length before it reads or
//! writes the element (decision 0010). In a loop that is a compare and a branch per access, and
//! the branch to a panic stops the backend from vectorizing the loop. A three-point stencil,
//!
//! ```python
//! for i in range(1, n - 1):
//!     b[i] = a[i - 1] + 2.0 * a[i] + a[i + 1]
//! ```
//!
//! ran 100 passes over 4 million elements in 0.82 s with its checks and in 0.36 s without.
//!
//! # What it does
//!
//! A counted loop is the loop lowering writes for `for i in range(start, stop)`: the counter
//! goes from its value on entry up to `stop`, by 1, and nothing else in the loop writes it. A
//! check in such a loop is hoisted when its index is the counter, a local the loop does not
//! write, or a constant, plus a constant, and its length is a constant, a local the loop does
//! not write, or the length of an array the loop does not replace. Then the check passes on
//! every pass exactly when it passes at the two ends of the counter's range, which a few
//! comparisons before the loop decide:
//!
//! ```text
//! pre-check: fast = start >= 1 & stop <= len(a) - 1 & ...
//!            branch fast -> [true: the copy, false: the loop as it was]
//! ```
//!
//! The copy is the loop with each hoisted check, and each overflow check on the way to its
//! index, replaced by a jump to where it would go when it passes. The steps that computed the
//! check's condition stay, unread, and the backend removes them.
//!
//! # Why it changes nothing a program can see
//!
//! The copy runs only when no check it lacks could fail, so it computes what the loop computes,
//! in the same order. When some check could fail, the loop runs as it was and panics at the same
//! element with the same message. The comparisons before the loop read only integers and
//! lengths the loop reads anyway, and cannot fail.
//!
//! # Unit strides
//!
//! Then each innermost counted loop that indexes an `Array` it does not replace gets a second
//! kind of copy, run when every such array's last stride is 1, whose blocks are listed in
//! `Body::unit_strides` so the backend finds elements without multiplying by that stride (the
//! `unit` module, roadmap 3.3.1). Lennard-Jones's force loop runs in 5.70 s instead of 6.31 s
//! with it, against gfortran's 5.6.
//!
//! # Where it runs
//!
//! In the instance query, last, after array expansion, so it also sees the loops array
//! expansion writes, though those index without checks. It reads and writes only `mir` types
//! and is one replaceable implementation (decision 0001); removing it changes only speed.
//!
//! # Cost
//!
//! A versioned loop is in the program twice, and a loop nested `d` deep in versioned loops up
//! to `2^d` times. The analysis of one loop walks its blocks and the definitions its indices
//! are computed through, and finds the body's dominators in `blocks²` bits.
//!
//! # Example
//!
//! ```
//! use mir::{BinaryOperator, BlockId, Body, Check, Constant, Local, LocalKind, Operand, Place,
//!     Rvalue, Statement, StatementKind, Terminator, TerminatorKind};
//! use source::{FileId, Span};
//! use types::{Integer, Primitive, Type, TypeKind};
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
//! let int = Type::new(&database, TypeKind::Primitive(Primitive::Integer(Integer::I64)));
//! let boolean = Type::new(&database, TypeKind::Primitive(Primitive::Bool));
//!
//! // for i in range(0, n): check that i lies in 0..8, the index of a `Fixed` array of 8.
//! let mut body = Body::new(int, span);
//! let n = body.push_local(int, LocalKind::Parameter { name: "n".into(), mode: types::Mode::Borrow }, span);
//! let [counter, test, above, below, inside] =
//!     [int, boolean, boolean, boolean, boolean].map(|ty| body.push_local(ty, LocalKind::Temporary, span));
//! let copy = |local: Local| Operand::Copy(Place::from(local));
//! let integer = |value: i128| Operand::Constant(Constant::integer(int, value));
//! let assign = |local: Local, rvalue| Statement { kind: StatementKind::Assign(local.into(), rvalue), span };
//! let blocks = vec![
//!     (vec![assign(counter, Rvalue::Use(integer(0)))], TerminatorKind::Goto(BlockId::new(1))),
//!     (
//!         vec![assign(test, Rvalue::Binary(BinaryOperator::Less, copy(counter), copy(n)))],
//!         TerminatorKind::Branch { condition: copy(test), then: BlockId::new(2), otherwise: BlockId::new(4) },
//!     ),
//!     (
//!         vec![
//!             assign(above, Rvalue::Binary(BinaryOperator::GreaterEqual, copy(counter), integer(0))),
//!             assign(below, Rvalue::Binary(BinaryOperator::Less, copy(counter), integer(8))),
//!             assign(inside, Rvalue::Binary(BinaryOperator::BitAnd, copy(above), copy(below))),
//!         ],
//!         TerminatorKind::Assert {
//!             condition: copy(inside),
//!             check: Check::IndexInBounds { dimension: 0, index: copy(counter), length: integer(8) },
//!             target: BlockId::new(3),
//!         },
//!     ),
//!     (
//!         vec![assign(counter, Rvalue::Binary(BinaryOperator::Add, copy(counter), integer(1)))],
//!         TerminatorKind::Goto(BlockId::new(1)),
//!     ),
//!     (Vec::new(), TerminatorKind::Return),
//! ];
//! for (statements, kind) in blocks {
//!     let id = body.push_block(Terminator { kind, span });
//!     body.blocks[id.index()].statements = statements;
//! }
//!
//! bounds_hoisting::hoist(&database, &mut body, &|_| types::Declared::Struct(Vec::new()));
//!
//! assert!(body.validate(&database).is_empty());
//! // The loop as it was, its copy without the check, and the block that picks one.
//! assert_eq!(body.blocks.len(), 5 + 3 + 1);
//! assert!(matches!(body.blocks[0].terminator.kind, TerminatorKind::Goto(pre_check) if pre_check.index() == 8));
//! assert!(matches!(body.blocks[2].terminator.kind, TerminatorKind::Assert { .. }));
//! assert_eq!(body.blocks[6].terminator.kind, TerminatorKind::Goto(BlockId::new(7)));
//! ```

mod loops;
mod unit;
mod values;
mod version;

use mir::Body;
use types::{Declared, Nominal};

/// Versions every counted loop of `body`, and of its `prange` loop bodies, that has index checks
/// the crate documentation's rules allow it to make before the loop starts; then versions each
/// innermost counted loop on the strides of the arrays it reads, as the `unit` module describes
/// (roadmap 3.3.1). `declared` says what each struct declares, to find the arrays in its fields.
///
/// Loops are found again after each one is versioned, since the copy of an outer loop holds
/// copies of the loops inside it, which are versioned in turn.
pub fn hoist<'db>(
    db: &'db dyn salsa::Database,
    body: &mut Body<'db>,
    declared: &dyn Fn(&Nominal<'db>) -> Declared<'db>,
) {
    for outlined in &mut body.outlined {
        hoist(db, outlined, declared);
    }
    hoist_checks(db, body);
    unit::version_all(db, body, declared);
}

/// Versions every counted loop of `body` that has checks it can make before the loop.
fn hoist_checks<'db>(db: &'db dyn salsa::Database, body: &mut Body<'db>) {
    let mut done = Vec::new();
    loop {
        let mut next = None;
        for counted in loops::counted_loops(body, &done) {
            let hoisted = values::hoistable(body, &counted);
            if hoisted.is_empty() {
                done.push(counted.header);
            } else {
                next = Some((counted, hoisted));
                break;
            }
        }
        let Some((counted, hoisted)) = next else {
            return;
        };
        version::version(db, body, &counted, &hoisted, &mut done);
    }
}

#[cfg(test)]
mod tests;
