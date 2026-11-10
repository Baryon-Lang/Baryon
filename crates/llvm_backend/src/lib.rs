//! The LLVM backend: translates mid-level IR bodies into LLVM's textual IR.
//!
//! Where it sits: the last compiler stage before LLVM. It reads [`mir`] bodies in phase
//! [`Ready`](mir::Phase::Ready), after monomorphization, drop elaboration and array expansion,
//! and writes one LLVM module as text. The driver hands that text to `clang`, which optimizes
//! it, compiles it to an object file and links it (decision 0020). This crate runs no process
//! and touches no file.
//!
//! How a body becomes a function:
//!
//! - **Every local is a stack slot** (`alloca`), loaded and stored by each use. LLVM's `mem2reg`
//!   pass turns the slots into registers at `-O1` and above, so the backend never builds SSA
//!   form itself, as `clang -O0` and `rustc` do.
//! - **Parameters follow decision 0006.** A copy type passed as a read-only borrow, or any value
//!   passed `Owned`, arrives by value. A borrow of anything else arrives as a `noalias readonly`
//!   pointer, and an `InOut` as a `noalias` pointer: the exclusivity check guarantees nothing
//!   else refers to it during the call, which is what lets LLVM vectorize without overlap tests.
//! - **Checks panic through the runtime.** A failed [`Assert`](mir::TerminatorKind::Assert), or
//!   a conversion that loses its value (language reference, "Conversions"), branches to a block
//!   of its own that calls the runtime's panic function (decision 0021) with the message and the
//!   file, line and column, so the program prints what failed and where, and exits with status
//!   101. The program links the `runtime` library for them.
//! - **Integer overflow panics, or wraps in a release build.** A
//!   [`CheckedBinary`](mir::Rvalue::CheckedBinary) is LLVM's `with.overflow` intrinsic, and the
//!   `Assert` on its flag panics with "integer overflow in `+`". After
//!   [`Module::wrap_on_overflow`] those asserts are left out (decision 0030).
//! - **Integer arithmetic wraps**, as [`Binary`](mir::Rvalue::Binary) defines, and never reaches
//!   LLVM's undefined cases: `x // -1` and `x % -1` are computed without `sdiv`'s overflow, and a
//!   shift by the width or more gives what shifting one bit at a time would.
//! - **Float `//` and `%` call the C math library**: LLVM compiles `frem` and `llvm.floor` to
//!   `fmod` and `floor`, so a program links `libm`, as C programs do.
//! - **Errors are values, not unwinding** (decision 0005). A function that can fail writes its
//!   error through a pointer it is passed last and returns a failure flag beside its value; the
//!   caller branches on the flag to its error edge. An error that ends `main` is printed to
//!   standard error by the C `main` [`Module::entry`] writes, which returns 1.
//! - **Arrays are a header and heap elements.** An `Array[T, rank]` value is the header
//!   [`types::layout`] describes: the data pointer, each length and each stride. A new array is
//!   allocated by the runtime, an element is found through the strides, and a drop frees the
//!   elements with the size they were allocated with.
//! - **Lists are a header and a growable buffer.** A `list[T]` is `{ ptr, i64, i64 }`: the
//!   data pointer, the length and the capacity. An append into a full buffer moves the elements
//!   to one twice as large, at least 4 elements (the `list` module).
//! - **Dictionaries are a pointer to the runtime's table.** A `dict[K, V]` is one `ptr`, null
//!   while empty. Finding, adding, removing and stepping are runtime calls, given a constant that
//!   describes the key type; an entry's key and value are read and written in place, at their
//!   offset in the table's entries array (the `dict` module, decision 0035).
//! - **Line tables, when asked for.** With [`Module::debug_info`], each function and each
//!   instruction points back to its source line and column (the `debug` module), so `gdb`,
//!   `lldb` and profilers show `.byn` lines.
//! - **`prange` loops run on the runtime's thread pool.** The outlined body becomes a function
//!   of its own, and a chunk function the runtime calls runs a chunk of iterations through it,
//!   each reduction into a private copy; the copies combine in a fixed pairwise tree after the
//!   loop, so a float sum gives the same bits on any number of threads (the `parallel` module).
//! - **`simd` loops ask LLVM to vectorize them.** The back edge to a loop header the body marks
//!   in [`vectorized`](mir::Body::vectorized) carries `llvm.loop.vectorize.enable` metadata,
//!   which overrides LLVM's cost model. A loop LLVM still declines, such as one whose bounds
//!   checks it cannot hoist, runs unvectorized for now; reporting it is roadmap 3.7.
//! - **`print` calls the runtime once per argument**, which writes the value as Python does.
//!   Lowering passes the space between arguments and the line end as `str` constants, which
//!   become byte constants. A tuple or a struct is taken apart into those calls when the program
//!   is compiled; a list or a dict is a loop over its elements that makes them as it runs.
//! - **Math functions are LLVM intrinsics.** A [`Builtin::Math`](mir::Builtin::Math) call, such
//!   as `x ** y` on floats, becomes `llvm.pow.f64`, which LLVM folds, turns into one instruction
//!   where the target has one, or calls the C math library for.
//! - **Complex numbers are a pair of floats.** A `complex128` is `{ double, double }`, the real
//!   part then the imaginary part. `+`, `-`, `*`, `/` and `==` work on the parts inline, with
//!   CPython's formulas, so results have Python's bits; `**` and `print` call the runtime (the
//!   `complex` module).
//!
//! - **A `str` is its bytes and a length** (decision 0022). A literal's bytes are a constant
//!   that is never copied; the top bit of its length says so, and a drop frees only a string
//!   whose bit is clear.
//!
//! The backend so far (roadmap 1.10.1 to 1.10.10, and 1.10.13) covers numbers, complex numbers
//! included, `bool`, enums without data,
//! tuples and structs, `Array` and `Fixed`, lists of copy types, optionals, `str` values, control
//! flow, direct calls, errors, checks, line tables, `print` and the math functions. A body that
//! uses anything else, such as a call of a `Callable` value, gets an [`Unsupported`] error
//! naming it.
//!
//! # Example
//!
//! `def double(x: f64) -> f64: return x * 2.0`:
//!
//! ```
//! use mir::{BinaryOperator, Body, Constant, LocalKind, Operand, Place, Rvalue, Statement,
//!     StatementKind, Terminator, TerminatorKind};
//! use source::{SourceMap, Span};
//! use types::{Declared, Float, Mode, Primitive, Type, TypeKind};
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
//! let mut sources = SourceMap::new();
//! let file = sources.add("double.byn", "def double(x: f64) -> f64: return x * 2.0")?;
//! let span = Span::new(file, 0, 40);
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
//! let declared = |_: &types::Nominal<'_>| Declared::Struct(Vec::new());
//! let symbol = |_: &types::Item<'_>, _: &[Type<'_>]| String::new();
//! let names = |_: &types::Nominal<'_>| llvm_backend::ClassNames::default();
//! let mut module = llvm_backend::Module::new(&database, &sources, &declared, &names, &symbol);
//! module.define("double", &body, &[])?;
//! assert_eq!(
//!     module.finish(),
//!     "\
//! define double @double(double %a1) {
//! start:
//!   %_0 = alloca double
//!   %_1 = alloca double
//!   store double %a1, ptr %_1
//!   br label %bb0
//! bb0:
//!   %t0 = load double, ptr %_1
//!   %t1 = fmul double %t0, 0x4000000000000000
//!   store double %t1, ptr %_0
//!   %t2 = load double, ptr %_0
//!   ret double %t2
//! }
//! ",
//! );
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod accumulate;
mod array;
mod call;
mod complex;
mod convert;
mod debug;
mod dict;
mod entry;
mod errors;
mod function;
mod list;
mod literal;
mod math;
mod module;
mod panic;
mod parallel;
mod print;
mod print_array;
mod print_collections;
mod process;
mod random;
mod rvalue;
mod sort;
mod string_methods;
mod strings;
mod text;
mod ty;
mod view;

use std::fmt;
use types::Type;

pub use module::Module;

/// One test of a test program, as [`Module::tests`] takes it: the symbol of its function, and the
/// error types the function can raise.
pub type Test<'a, 'db> = (&'a str, &'a [Type<'db>]);

/// The names `print` writes for a struct or an enum, and the report of an error that ends `main`
/// writes for an error type: `Point(x=1.0, y=2.0)`, `Phase.GAS`. The type checker knows them;
/// the application passes them to [`Module::new`] through a callback, so this crate needs no
/// type checker.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClassNames {
    /// The class's name, without type arguments: `Pair`, not `Pair[f64]`, as a Python
    /// dataclass prints itself.
    pub class: String,
    /// A struct's field names, or an enum's member names, in declaration order.
    pub members: Vec<String>,
}

/// A form of the mid-level IR this slice of the backend does not translate yet, such as a call
/// of a `Callable` value. Roadmap 1.10 adds them slice by slice; a type-checked program reaches
/// this only through features the backend has not caught up with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unsupported {
    /// What is not supported, as a phrase: "values of type `Array[f64, 1]`".
    what: String,
}

impl Unsupported {
    /// The error for `what`.
    fn new(what: impl Into<String>) -> Self {
        Self { what: what.into() }
    }
}

impl fmt::Display for Unsupported {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "the LLVM backend does not compile {} yet",
            self.what
        )
    }
}

impl std::error::Error for Unsupported {}

#[cfg(test)]
mod tests;
