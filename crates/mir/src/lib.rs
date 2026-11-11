//! The mid-level IR, or MIR: the form every function body takes between the type checker and the
//! backends.
//!
//! A [`Body`] is a list of typed [locals](LocalDeclaration) and a list of
//! [basic blocks](BasicBlock). Each block runs its [statements](Statement) in order and ends in
//! one [`Terminator`], the only place control flow branches. Memory is read and written through
//! [places](Place): a local followed by field, element, slice and value steps, such as
//! `particles[i].velocity` or the view `grid[:, k]`. Decision 0018 explains why the IR has this
//! shape rather than static single assignment form: the reference rules, `noalias` metadata and
//! drops are all stated over places.
//!
//! Where it sits: lowering (an implementation crate) builds one body per function from the typed
//! syntax tree. Drop elaboration, monomorphization and the optimizations rewrite bodies, and the
//! LLVM and Cranelift backends translate them. Every one of those stages reads and writes the types
//! in this crate, and none of them depends on another (decision 0001).
//!
//! What the IR makes explicit, so no later stage guesses:
//!
//! - **Drops**: a [`Drop`](StatementKind::Drop) statement ends each owned value.
//! - **Checks**: an [`Assert`](TerminatorKind::Assert) before each array index and slice
//!   (decision 0010), each division and each checked operation.
//! - **Views**: a slice of an array is a place ending in a [`Slice`](Projection::Slice) step,
//!   never a value, because a view is a borrow and cannot be bound to a name (language
//!   reference, "Slices").
//! - **Errors**: a call to a function that can fail has an [`ErrorEdge`]; a body fails with
//!   [`Raise`](TerminatorKind::Raise) (decision 0005).
//! - **Argument modes**: each call [`Argument`] is a read-only borrow, an `InOut` borrow or an
//!   owned value, as decision 0006 defines. References never become values.
//! - **Parallel loops**: a `prange` loop's body is a separate [outlined](Body::outlined) body,
//!   run by a [`ParallelFor`](TerminatorKind::ParallelFor) that lists every place it shares,
//!   every value it reduces into and every array it accumulates into, which the body adds to
//!   only through [`Accumulate`](StatementKind::Accumulate) statements.
//! - **Vector loops**: a `simd` loop stays an ordinary loop whose header block is listed in
//!   [`Body::vectorized`], for the backend to vectorize.
//!
//! [`Body::validate`] checks the structural rules every body keeps, and [`Body::display`] writes
//! the text form used in tests and when debugging the compiler.
//!
//! # Example
//!
//! `def double(x: f64) -> f64: return x * 2.0`, as lowering builds it:
//!
//! ```
//! use mir::{BinaryOperator, Body, Constant, LocalKind, Operand, Place, Rvalue, Statement,
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
//! let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
//! let span = Span::new(FileId::from_index(0), 0, 40);
//!
//! let mut body = Body::new(f64, span);
//! let x = body.push_local(f64, LocalKind::Parameter { name: "x".into(), mode: Mode::Borrow }, span);
//! let entry = body.push_block(Terminator { kind: TerminatorKind::Return, span });
//! body.blocks[entry.index()].statements.push(Statement {
//!     kind: StatementKind::Assign(
//!         Place::from(mir::Local::RETURN),
//!         Rvalue::Binary(
//!             BinaryOperator::Multiply,
//!             Operand::Copy(Place::from(x)),
//!             Operand::Constant(Constant::float(f64, 2.0)),
//!         ),
//!     ),
//!     span,
//! });
//!
//! assert!(body.validate(&database).is_empty());
//! assert_eq!(
//!     body.display(&database).to_string(),
//!     "\
//! fn(_1: f64) -> f64 {
//!     let _0: f64; // return
//!     let _1: f64; // parameter x
//!
//!     bb0: {
//!         _0 = Multiply(copy _1, 2.0_f64);
//!         return;
//!     }
//! }
//! ",
//! );
//! ```

mod call;
mod check;
mod display;
mod files;
mod operand;
mod paths;
mod place;
mod statement;
mod text;
mod validate;

pub use call::{Argument, Builtin, Callee, MathFunction};
pub use check::Check;
pub use display::DisplayedBody;
pub use files::FileMethod;
pub use operand::{
    BinaryOperator, Constant, ConstantValue, Elementwise, Operand, Piece, Rvalue, UnaryOperator,
};
pub use paths::PathMethod;
pub use place::{Place, Projection, SliceIndex};
pub use statement::{
    AccumulateOperator, ErrorEdge, Reduction, ReductionOperator, Statement, StatementKind,
    Terminator, TerminatorKind,
};
pub use text::TextMethod;
pub use validate::Invalid;

use source::Span;
use types::{Mode, Type};

/// One function body: its locals and its basic blocks.
///
/// Local [`Local::RETURN`] holds the value the body returns. The parameters follow it, in
/// order, as locals `1..=parameter_count`. Execution starts at block [`BlockId::ENTRY`].
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub struct Body<'db> {
    /// Which forms the body may contain, and so which stages may read it.
    pub phase: Phase,
    /// Every local, indexed by [`Local`].
    pub locals: Vec<LocalDeclaration<'db>>,
    /// How many parameters the function takes; they are locals `1..=parameter_count`.
    pub parameter_count: u32,
    /// Every basic block, indexed by [`BlockId`].
    pub blocks: Vec<BasicBlock<'db>>,
    /// The whole function in the source, for debug information.
    pub span: Span,
    /// The bodies of the function's `prange` loops, each called by a
    /// [`ParallelFor`](TerminatorKind::ParallelFor) that names its position here. They share
    /// the function's phase and type parameters, and pass over them as the function does.
    /// Empty in a function with no `prange`, and in an outlined body itself, since `prange`
    /// loops do not nest.
    pub outlined: Vec<Body<'db>>,
    /// The header block of each `simd` loop, in the order lowering wrote them: the block whose
    /// test decides whether another iteration runs. The loop stays an ordinary loop (decision
    /// 0018); the mark asks the backend to vectorize it, and the checker has already proved
    /// that running its iterations several at a time is safe (decision 0011). Passes that add
    /// blocks keep every existing block's id, so the marks stay on their headers.
    pub vectorized: Vec<BlockId>,
}

impl<'db> Body<'db> {
    /// An empty body in phase [`Phase::Built`] that returns a `returns`, with only the return
    /// local. Add parameters with [`push_local`](Self::push_local) before any other local, then
    /// the blocks, the entry block first.
    ///
    /// ```
    /// use mir::{Body, Local, LocalKind};
    /// use source::{FileId, Span};
    /// use types::{Float, Primitive, Type, TypeKind};
    /// # #[salsa::db]
    /// # #[derive(Clone, Default)]
    /// # struct Database { storage: salsa::Storage<Self> }
    /// # #[salsa::db]
    /// # impl salsa::Database for Database {}
    /// # #[salsa::db]
    /// # impl database::Db for Database {
    /// #     fn parse(&self, _: database::File) -> &database::ParsedFile { unreachable!() }
    /// #     fn resolve(&self, _: database::Package, _: database::File) -> &database::Resolution {
    /// #         unreachable!()
    /// #     }
    /// # }
    /// # let database = Database::default();
    /// let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    /// let body = Body::new(f64, Span::new(FileId::from_index(0), 0, 10));
    /// assert_eq!(body.locals.len(), 1);
    /// assert_eq!(body.locals[Local::RETURN.index()].kind, LocalKind::Return);
    /// assert!(body.blocks.is_empty());
    /// ```
    #[must_use]
    pub fn new(returns: Type<'db>, span: Span) -> Self {
        Self {
            phase: Phase::Built,
            locals: vec![LocalDeclaration {
                ty: returns,
                kind: LocalKind::Return,
                span,
            }],
            parameter_count: 0,
            blocks: Vec::new(),
            span,
            outlined: Vec::new(),
            vectorized: Vec::new(),
        }
    }

    /// Adds a local of type `ty` and returns it. A parameter also counts toward
    /// [`parameter_count`](Self::parameter_count); add parameters before any other local, or
    /// [`validate`](Self::validate) reports the body.
    ///
    /// ```
    /// use mir::{Body, LocalKind};
    /// use source::{FileId, Span};
    /// use types::{Float, Primitive, Type, TypeKind};
    /// use types::Mode;
    /// # #[salsa::db]
    /// # #[derive(Clone, Default)]
    /// # struct Database { storage: salsa::Storage<Self> }
    /// # #[salsa::db]
    /// # impl salsa::Database for Database {}
    /// # #[salsa::db]
    /// # impl database::Db for Database {
    /// #     fn parse(&self, _: database::File) -> &database::ParsedFile { unreachable!() }
    /// #     fn resolve(&self, _: database::Package, _: database::File) -> &database::Resolution {
    /// #         unreachable!()
    /// #     }
    /// # }
    /// # let database = Database::default();
    /// let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    /// let span = Span::new(FileId::from_index(0), 0, 10);
    /// let mut body = Body::new(f64, span);
    /// let x = body.push_local(f64, LocalKind::Parameter { name: "x".into(), mode: Mode::Borrow }, span);
    /// let t = body.push_local(f64, LocalKind::Temporary, span);
    /// assert_eq!((x.index(), t.index()), (1, 2));
    /// assert_eq!(body.parameter_count, 1);
    /// ```
    pub fn push_local(&mut self, ty: Type<'db>, kind: LocalKind, span: Span) -> Local {
        if matches!(kind, LocalKind::Parameter { .. }) {
            self.parameter_count += 1;
        }
        self.locals.push(LocalDeclaration { ty, kind, span });
        Local::new(self.locals.len() - 1)
    }

    /// Adds a block with no statements that ends in `terminator`, and returns it.
    ///
    /// ```
    /// use mir::{BlockId, Body, Terminator, TerminatorKind};
    /// use source::{FileId, Span};
    /// use types::{Float, Primitive, Type, TypeKind};
    /// # #[salsa::db]
    /// # #[derive(Clone, Default)]
    /// # struct Database { storage: salsa::Storage<Self> }
    /// # #[salsa::db]
    /// # impl salsa::Database for Database {}
    /// # #[salsa::db]
    /// # impl database::Db for Database {
    /// #     fn parse(&self, _: database::File) -> &database::ParsedFile { unreachable!() }
    /// #     fn resolve(&self, _: database::Package, _: database::File) -> &database::Resolution {
    /// #         unreachable!()
    /// #     }
    /// # }
    /// # let database = Database::default();
    /// let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    /// let span = Span::new(FileId::from_index(0), 0, 10);
    /// let mut body = Body::new(f64, span);
    /// let entry = body.push_block(Terminator { kind: TerminatorKind::Return, span });
    /// assert_eq!(entry, BlockId::ENTRY);
    /// assert!(body.blocks[entry.index()].statements.is_empty());
    /// ```
    pub fn push_block(&mut self, terminator: Terminator<'db>) -> BlockId {
        self.blocks.push(BasicBlock {
            statements: Vec::new(),
            terminator,
        });
        BlockId::new(self.blocks.len() - 1)
    }
}

/// Which forms a body may contain (decision 0018).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    /// As lowering builds it, once per function. Types may contain type parameters, a
    /// [`Drop`](StatementKind::Drop) drops its value only if the value was not moved out, and
    /// whole-array expressions are single [`Elementwise`] rvalues.
    Built,
    /// Ready for a backend: one instance of a function at concrete types, after drop elaboration
    /// and array expansion. No type parameters, every drop unconditional, no `Elementwise`.
    Ready,
}

/// One local: a numbered storage slot in a body, assigned any number of times.
///
/// Locals are small ids; read what one holds from [`Body::locals`] at [`Local::index`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Local(u32);

impl Local {
    /// The local that holds the body's return value, always the first.
    pub const RETURN: Self = Self(0);

    /// The local at `index` in [`Body::locals`].
    ///
    /// # Panics
    ///
    /// If `index` does not fit in a `u32`. A body that large is a compiler bug.
    ///
    /// ```
    /// use mir::Local;
    ///
    /// assert_eq!(Local::new(0), Local::RETURN);
    /// assert_eq!(Local::new(3).index(), 3);
    /// ```
    #[must_use]
    pub fn new(index: usize) -> Self {
        Self(to_u32(index))
    }

    /// The local's position in [`Body::locals`].
    #[must_use]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// What a body knows about one local.
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub struct LocalDeclaration<'db> {
    /// The type of the value the local holds.
    pub ty: Type<'db>,
    /// What the local is for.
    pub kind: LocalKind,
    /// Where its value comes from in the source, for debug information.
    pub span: Span,
}

/// What a local is for.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LocalKind {
    /// The return value, [`Local::RETURN`].
    Return,
    /// A parameter. A borrowed parameter's storage belongs to the caller: the backend passes it
    /// as a pointer, and nothing else in the IR knows.
    Parameter {
        /// The parameter's name in the source.
        name: String,
        /// How the argument is passed (decision 0006).
        mode: Mode,
    },
    /// A variable the program names.
    Variable {
        /// The variable's name in the source.
        name: String,
    },
    /// A value the compiler introduced: an intermediate result, an error value or a drop flag.
    Temporary,
}

/// The id of one basic block: its position in [`Body::blocks`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(u32);

impl BlockId {
    /// The block execution starts in, always the first.
    pub const ENTRY: Self = Self(0);

    /// The block at `index` in [`Body::blocks`].
    ///
    /// # Panics
    ///
    /// If `index` does not fit in a `u32`. A body that large is a compiler bug.
    ///
    /// ```
    /// use mir::BlockId;
    ///
    /// assert_eq!(BlockId::new(0), BlockId::ENTRY);
    /// assert_eq!(BlockId::new(2).index(), 2);
    /// ```
    #[must_use]
    pub fn new(index: usize) -> Self {
        Self(to_u32(index))
    }

    /// The block's position in [`Body::blocks`].
    #[must_use]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A straight run of statements that ends in one terminator. Control enters only at the top and
/// leaves only through the terminator.
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub struct BasicBlock<'db> {
    /// The statements, run in order.
    pub statements: Vec<Statement<'db>>,
    /// Where control goes next.
    pub terminator: Terminator<'db>,
}

/// Converts a position in a body's locals or blocks to the `u32` an id stores.
///
/// # Panics
///
/// If `index` does not fit in a `u32`: no function has four billion locals or blocks, so a body
/// that does is a compiler bug.
fn to_u32(index: usize) -> u32 {
    let converted = u32::try_from(index);
    assert!(
        converted.is_ok(),
        "a body has fewer than 2^32 locals and blocks"
    );
    converted.unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests;
