//! Array fusion: moves a whole-array temporary into the one later expression that reads it, so
//! no array is built for it (roadmap 3.1).
//!
//! Lowering keeps each whole-array statement as one [`Elementwise`](mir::Rvalue::Elementwise)
//! rvalue, and array expansion turns each into one loop. A program written in steps,
//!
//! ```python
//! t = a * b
//! u = t + d
//! c = u * 0.5 + c * 0.25
//! ```
//!
//! would then run three loops and build two arrays that are read once and freed. This pass
//! rewrites it into the one statement `c = fused(fused(a * b) + d) * 0.5 + c * 0.25`, which
//! expands to one loop, as `c = (a * b + d) * 0.5 + c * 0.25` does. On 4 million elements the
//! three-statement form ran in 1.88 s and the one-expression form in 0.86 s.
//!
//! # What is fused
//!
//! A local is fused into the statement that reads it when:
//!
//! 1. it is a variable or a temporary, never a parameter, whose type is an array of floats or
//!    complex numbers;
//! 2. it is assigned once in the whole body, a whole-array expression, and read once, by a
//!    whole-array expression later in the same block;
//! 3. no statement between the two names a local the expression reads.
//!
//! # Why it changes nothing a program can see
//!
//! Each element is computed by the same operations on the same values in the same order, so
//! every result is identical to the bit. The fused expression is wrapped in
//! [`Fused`](mir::Elementwise::Fused) with its statement's span, so its arrays' lengths are
//! still checked where it was, before the reader's are, and a mismatch panics with the same
//! message at the same line. Its operations on floats and complex numbers cannot panic, so
//! computing them interleaved with the reader's changes no panic either. Integer arrays are
//! left alone: their overflow checks would fire in a different order.
//!
//! # Where it runs
//!
//! In the instance query, after monomorphization, so element types are concrete, and before
//! drop elaboration, which then removes the drops of the temporaries that are never assigned
//! any more. It reads and writes only `mir` types and is one replaceable implementation
//! (decision 0001); removing it changes only speed.
//!
//! # Example
//!
//! ```
//! use mir::{BinaryOperator, Body, Elementwise, Local, LocalKind, Operand, Rvalue, Statement,
//!     StatementKind, Terminator, TerminatorKind};
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
//! let vector = Type::new(&database, TypeKind::Array { element: f64, rank: 1 });
//!
//! // def f(a: Array[f64, 1], b: Array[f64, 1]) -> Array[f64, 1]:
//! //     t = a * b
//! //     return t + b
//! let mut body = Body::new(vector, span);
//! let borrowed = |name: &str| LocalKind::Parameter { name: name.into(), mode: Mode::Borrow };
//! let a = body.push_local(vector, borrowed("a"), span);
//! let b = body.push_local(vector, borrowed("b"), span);
//! let t = body.push_local(vector, LocalKind::Variable { name: "t".into() }, span);
//! let leaf = |local: Local| Box::new(Elementwise::Leaf(Operand::Copy(local.into())));
//! let entry = body.push_block(Terminator { kind: TerminatorKind::Return, span });
//! body.blocks[entry.index()].statements = vec![
//!     Statement {
//!         kind: StatementKind::Assign(
//!             t.into(),
//!             Rvalue::Elementwise(Box::new(Elementwise::Binary(
//!                 BinaryOperator::Multiply, leaf(a), leaf(b),
//!             ))),
//!         ),
//!         span,
//!     },
//!     Statement {
//!         kind: StatementKind::Assign(
//!             Local::RETURN.into(),
//!             Rvalue::Elementwise(Box::new(Elementwise::Binary(BinaryOperator::Add, leaf(t), leaf(b)))),
//!         ),
//!         span,
//!     },
//! ];
//!
//! array_fusion::fuse(&database, &mut body);
//!
//! assert!(body.validate(&database).is_empty());
//! let text = body.display(&database).to_string();
//! assert!(text.contains("_0 = elementwise Add(fused(Multiply(copy _1, copy _2)), copy _2);"));
//! assert!(!text.contains("_3 = "));
//! ```

mod fusion;

use mir::Body;

/// Fuses every temporary of `body`, and of its `prange` loop bodies, that the crate
/// documentation's rules allow, into the statement that reads it.
///
/// Cost: for each fusion, a scan of the block and of the body's drops; bodies are small and
/// fusions few. The program it builds makes one pass over the arrays where it made one per
/// statement, and allocates none of the fused temporaries.
pub fn fuse(db: &dyn salsa::Database, body: &mut Body<'_>) {
    for outlined in &mut body.outlined {
        fuse(db, outlined);
    }
    let mentions = mentions(body);
    for block in 0..body.blocks.len() {
        while fusion::fuse_one(db, body, block, &mentions) {}
    }
}

/// How many times each local of `body` is named, by its statements and terminators.
///
/// Fusing a temporary leaves every other local's count as it was: the expression's leaves move
/// from one statement to another, and only the temporary, and the unnamed one that held it,
/// stop being named. So the counts are taken once.
fn mentions(body: &Body<'_>) -> Vec<usize> {
    let mut counts = vec![0; body.locals.len()];
    for block in &body.blocks {
        let named = block
            .statements
            .iter()
            .flat_map(|statement| statement.kind.locals())
            .chain(block.terminator.kind.locals());
        for local in named {
            if let Some(count) = counts.get_mut(local.index()) {
                *count += 1;
            }
        }
    }
    counts
}

#[cfg(test)]
mod tests;
