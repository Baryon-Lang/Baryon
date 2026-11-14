//! Drop elaboration: the pass that makes every drop in a mid-level IR body unconditional.
//!
//! Lowering writes a [`Drop`](mir::StatementKind::Drop) of each owned local at every exit from its
//! scope, and in a built body a drop means "drop the value if it is still there"
//! (decision 0018). This pass works out, at each drop, whether the value is certainly there,
//! certainly gone, or there on some paths only:
//!
//! - **certainly there**: the drop stays as it is;
//! - **certainly gone**, because it was moved out on every path: the drop is removed;
//! - **there on some paths**: the drop is guarded by a *drop flag*, a `bool` local that is set
//!   when the value is assigned and cleared when it is moved or dropped;
//! - **partly moved**, because a field was moved out of a struct (the reference, "Moves"): the
//!   drop is replaced by drops of the fields still there, each decided the same way.
//!
//! After it runs, every drop in the body is unconditional, which is what a backend needs. It runs
//! in the instance query, after type substitution and before array expansion, so field types are
//! concrete.
//!
//! It is one replaceable implementation (decision 0001): it reads and writes only `mir` types,
//! and asks what a struct declares through a callback, as [`types::layout`] does.
//!
//! # Example
//!
//! An array moved out on one branch and dropped where the branches meet gets a flag:
//!
//! ```
//! use mir::{Body, BlockId, LocalKind, Operand, Place, Rvalue, Statement, StatementKind,
//!     Terminator, TerminatorKind};
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
//! let bool = Type::new(&database, TypeKind::Primitive(Primitive::Bool));
//! let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
//! let array = Type::new(&database, TypeKind::Array { element: f64, rank: 1 });
//! let none = Type::new(&database, TypeKind::None);
//! let end = |kind| Terminator { kind, span };
//! let statement = |kind| Statement { kind, span };
//!
//! // def keep_if(c: bool, a: Owned[Array[f64, 1]]): if c: b = a
//! let mut body = Body::new(none, span);
//! let c = body.push_local(bool, LocalKind::Parameter { name: "c".into(), mode: Mode::Borrow }, span);
//! let a = body.push_local(array, LocalKind::Parameter { name: "a".into(), mode: Mode::Owned }, span);
//! let b = body.push_local(array, LocalKind::Variable { name: "b".into() }, span);
//! body.push_block(end(TerminatorKind::Branch {
//!     condition: Operand::Copy(c.into()),
//!     then: BlockId::new(1),
//!     otherwise: BlockId::new(2),
//! }));
//! let then = body.push_block(end(TerminatorKind::Goto(BlockId::new(2))));
//! body.blocks[then.index()].statements = vec![
//!     statement(StatementKind::Assign(b.into(), Rvalue::Use(Operand::Move(a.into())))),
//!     statement(StatementKind::Drop(b.into())),
//! ];
//! let join = body.push_block(end(TerminatorKind::Return));
//! body.blocks[join.index()].statements = vec![statement(StatementKind::Drop(a.into()))];
//!
//! drop_elaboration::elaborate(&database, &mut body, &|_| unreachable!(), &|_| false);
//!
//! // `a` is still there only when `c` was false, so its drop is guarded by a new flag, `_4`.
//! let text = body.display(&database).to_string();
//! assert!(text.contains("_4 = True;\n        branch copy _1"));
//! assert!(text.contains("branch copy _4 -> [true: bb3, false: bb4]"));
//! assert!(body.validate(&database).is_empty());
//! ```

mod dataflow;
mod paths;
mod rewrite;

use dataflow::block_entries;
use paths::Paths;
use types::{Declared, Nominal};

/// Makes every drop in `body` unconditional: removes drops of values that are certainly moved
/// out, guards drops of values moved on some paths with new `bool` flag locals, and replaces the
/// drop of a partly moved struct with drops of its remaining fields. `declared` says what a
/// struct declares, with its type arguments substituted; it is called only for structs a field was
/// moved out of. `destructor` says whether a struct's class has `__del__` (decision 0063), so a
/// remaining field of such a struct keeps its drop even when it owns no memory.
///
/// The bodies of the function's `prange` loops are elaborated too, each on its own.
///
/// Run it once per body. The analysis does not read flag values, so a second run would see a
/// guarded drop as still conditional and guard it again: correct, but slower.
///
/// Cost: one pass to find the paths, a dataflow pass bounded by blocks times move paths, and one
/// rewrite. Move paths are the places a body moves or drops, usually a handful.
pub fn elaborate<'db>(
    db: &'db dyn salsa::Database,
    body: &mut mir::Body<'db>,
    declared: &dyn Fn(&Nominal<'db>) -> Declared<'db>,
    destructor: &dyn Fn(&Nominal<'db>) -> bool,
) {
    for outlined in &mut body.outlined {
        elaborate(db, outlined, declared, destructor);
    }
    let paths = Paths::collect(&body.blocks);
    if paths.is_empty() {
        return;
    }
    let entries = block_entries(&body.blocks, &paths, body.parameter_count);
    rewrite::rewrite(db, body, &paths, &entries, declared, destructor);
}

#[cfg(test)]
mod tests;
