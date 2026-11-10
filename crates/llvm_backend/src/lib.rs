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
//!   become byte constants.
//! - **Math functions are LLVM intrinsics.** A [`Builtin::Math`](mir::Builtin::Math) call, such
//!   as `x ** y` on floats, becomes `llvm.pow.f64`, which LLVM folds, turns into one instruction
//!   where the target has one, or calls the C math library for.
//!
//! - **A `str` is its bytes and a length** (decision 0022). A literal's bytes are a constant
//!   that is never copied; the top bit of its length says so, and a drop frees only a string
//!   whose bit is clear.
//!
//! The backend so far (roadmap 1.10.1 to 1.10.10) covers numbers, `bool`, enums without data,
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
mod convert;
mod debug;
mod entry;
mod errors;
mod function;
mod list;
mod literal;
mod math;
mod panic;
mod parallel;
mod print;
mod random;
mod rvalue;
mod strings;
mod text;
mod ty;
mod view;

use mir::Body;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use types::{Declared, Item, Nominal, Type};

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

/// One LLVM module being written: the functions defined so far, and a declaration for each
/// function they call that the module does not define.
pub struct Module<'a, 'db> {
    /// How to represent each type.
    types: ty::Types<'a, 'db>,
    /// The symbol of a function instance: an item with its type arguments.
    symbol: &'a dyn Fn(&Item<'db>, &[Type<'db>]) -> String,
    /// Each function definition's text, in the order they were defined.
    definitions: Vec<String>,
    /// The messages and source locations failed checks point to.
    constants: panic::Constants<'a>,
    /// The symbols this module defines, so they are not declared too.
    defined: BTreeSet<String>,
    /// The declaration of each symbol called, written from the call, by symbol.
    declarations: BTreeMap<String, String>,
}

impl<'a, 'db> Module<'a, 'db> {
    /// An empty module. `sources` holds the files the bodies' spans point into, for the file,
    /// line and column a panic reports; `declared` says what each struct or enum declares, with
    /// its type arguments substituted, as [`types::layout`] needs; `names` gives the names
    /// `print` writes for each struct or enum; `symbol` names the instance of a function at its
    /// type arguments, the same way for every module of a program so calls between modules link.
    pub fn new(
        db: &'db dyn database::Db,
        sources: &'a dyn source::Sources,
        declared: &'a dyn Fn(&Nominal<'db>) -> Declared<'db>,
        names: &'a dyn Fn(&Nominal<'db>) -> ClassNames,
        symbol: &'a dyn Fn(&Item<'db>, &[Type<'db>]) -> String,
    ) -> Self {
        Self {
            types: ty::Types {
                db,
                declared,
                names,
            },
            symbol,
            definitions: Vec::new(),
            constants: panic::Constants::new(sources),
            defined: BTreeSet::new(),
            declarations: BTreeMap::new(),
        }
    }

    /// Adds the function `symbol` with the body `body`, which must be in phase
    /// [`Ready`](mir::Phase::Ready) and valid. `errors` are the error types its `Raises` lists,
    /// in order, or none when it cannot fail; a function that can fail takes a pointer to where
    /// it writes its error, after its parameters, and returns a failure flag beside its value.
    /// Each of the body's outlined `prange` bodies becomes an internal function
    /// `symbol.body0`, `symbol.body1`, …, beside the chunk function `symbol.chunk0`, … that runs
    /// it (the `parallel` module).
    ///
    /// Cost: one pass over the body, writing text proportional to its size.
    ///
    /// # Errors
    ///
    /// [`Unsupported`] when the body uses a type or form this slice of the backend does not
    /// translate yet; the module is left as it was.
    pub fn define(
        &mut self,
        symbol: &str,
        body: &Body<'db>,
        errors: &[Type<'db>],
    ) -> Result<(), Unsupported> {
        let mark = self.constants.mark();
        let mut written = Vec::new();
        let mut calls = BTreeMap::new();
        let bodies = std::iter::once((symbol.to_owned(), body, errors, false)).chain(
            body.outlined.iter().enumerate().map(|(index, outlined)| {
                (format!("{symbol}.body{index}"), outlined, &[][..], true)
            }),
        );
        for (name, body, errors, internal) in bodies {
            let (text, called) = function::Function::write(
                self.types,
                self.symbol,
                &mut self.constants,
                &name,
                body,
                errors,
                internal,
            )
            .inspect_err(|_| self.constants.roll_back(mark))?;
            written.push(text);
            calls.extend(called);
        }
        self.definitions.extend(written);
        self.defined.insert(symbol.to_owned());
        self.declarations.extend(calls);
        Ok(())
    }

    /// Makes the function `symbol` the program's entry point: adds a C `main` that calls it and
    /// returns 0, the exit status of a program that ran to its end, after the runtime writes out
    /// a line `print(..., end="")` left unended. `symbol` takes no arguments and returns `None`;
    /// a failed check inside it ends the program through the runtime with status 101 instead.
    ///
    /// `errors` are the error types `symbol` can raise, in its `Raises` order; empty when it
    /// cannot fail. An error that reaches the end of `main` is printed to standard error as
    /// `error: ParseError(position=-3)`, as `print` writes a struct, and the program returns 1
    /// (the reference, "Errors").
    ///
    /// # Errors
    ///
    /// [`Unsupported`] when an error has a field this slice of the backend cannot print, such
    /// as an array; the module is left as it was.
    ///
    /// ```
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
    /// let database = Database::default();
    /// let sources = source::SourceMap::new();
    /// let declared = |_: &types::Nominal<'_>| types::Declared::Struct(Vec::new());
    /// let symbol = |_: &types::Item<'_>, _: &[types::Type<'_>]| String::new();
    /// let names = |_: &types::Nominal<'_>| llvm_backend::ClassNames::default();
    /// let mut module =
    ///     llvm_backend::Module::new(&database, &sources, &declared, &names, &symbol);
    /// module.entry("main.main", &[])?;
    /// assert_eq!(
    ///     module.finish(),
    ///     "define i32 @main() {\nstart:\n  call void @main.main()\n  \
    ///      call void @baryon_rt_flush()\n  ret i32 0\n}\n\n\
    ///      declare void @baryon_rt_flush() nounwind\n"
    /// );
    /// # Ok::<(), llvm_backend::Unsupported>(())
    /// ```
    pub fn entry(&mut self, symbol: &str, errors: &[Type<'db>]) -> Result<(), Unsupported> {
        let mark = self.constants.mark();
        let body = entry::no_body(self.types.db);
        let function =
            function::Function::new(self.types, self.symbol, &mut self.constants, &body, &[]);
        let (text, calls) = function
            .entry(symbol, errors)
            .inspect_err(|_| self.constants.roll_back(mark))?;
        self.definitions.push(text);
        self.defined.insert("main".to_owned());
        self.declarations.extend(calls);
        Ok(())
    }

    /// Makes the module a test program: adds a C `main` that runs one function of `tests`, the
    /// one at the position its first command-line argument names, counting from 0, and exits as
    /// the C `main` of [`entry`](Self::entry) would for that function: 0 when it returns, 1 after
    /// printing an error it raised, 101 when a check inside it fails. It returns 2 when the
    /// argument is missing or names no test.
    ///
    /// Each test is a function's symbol and the errors it can raise, as `entry` takes them. A
    /// test runner starts the program once per test, so a panic, which ends the process, fails
    /// only its own test.
    ///
    /// # Errors
    ///
    /// [`Unsupported`] when an error has a field this slice of the backend cannot print; the
    /// module is left as it was.
    ///
    /// ```
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
    /// let database = Database::default();
    /// let sources = source::SourceMap::new();
    /// let declared = |_: &types::Nominal<'_>| types::Declared::Struct(Vec::new());
    /// let symbol = |_: &types::Item<'_>, _: &[types::Type<'_>]| String::new();
    /// let names = |_: &types::Nominal<'_>| llvm_backend::ClassNames::default();
    /// let mut module =
    ///     llvm_backend::Module::new(&database, &sources, &declared, &names, &symbol);
    /// module.tests(&[("heat.test_flux", &[]), ("heat.test_energy", &[])])?;
    /// let text = module.finish();
    /// assert!(text.starts_with("define i32 @main(i32 %count, ptr %arguments) {"), "{text}");
    /// assert!(text.contains("test1:\n  call void @heat.test_energy()\n"), "{text}");
    /// assert!(text.contains("declare i32 @atoi(ptr)\n"), "{text}");
    /// # Ok::<(), llvm_backend::Unsupported>(())
    /// ```
    pub fn tests(&mut self, tests: &[Test<'_, 'db>]) -> Result<(), Unsupported> {
        let mark = self.constants.mark();
        let body = entry::no_body(self.types.db);
        let function =
            function::Function::new(self.types, self.symbol, &mut self.constants, &body, &[]);
        let (text, calls) = function
            .tests(tests)
            .inspect_err(|_| self.constants.roll_back(mark))?;
        self.definitions.push(text);
        self.defined.insert("main".to_owned());
        self.declarations.extend(calls);
        Ok(())
    }

    /// The module's text: every definition in order, then the message and location constants
    /// their checks point to, then a declaration of each function called and not defined here, in symbol order, so the text is the same on every run.
    #[must_use]
    pub fn finish(self) -> String {
        let mut text = self.definitions.join("\n");
        let metadata = self.constants.metadata();
        let constants = self.constants.finish();
        if !constants.is_empty() {
            text.push('\n');
            text.push_str(&constants);
        }
        let declared: Vec<&String> = self
            .declarations
            .iter()
            .filter(|(symbol, _)| !self.defined.contains(*symbol))
            .map(|(_, declaration)| declaration)
            .collect();
        if !declared.is_empty() {
            text.push('\n');
            for declaration in declared {
                text.push_str(declaration);
                text.push('\n');
            }
        }
        if !metadata.is_empty() {
            text.push('\n');
            text.push_str(&metadata);
        }
        text
    }

    /// Adds line tables to the module: each function and the instructions written from each
    /// statement point back to their source line and column, so debuggers, profilers and
    /// backtraces can name them (the `debug` module). `directory` is the absolute directory the
    /// source files' names are relative to, the package's root. Call it before
    /// [`define`](Self::define); functions defined earlier have no line tables.
    ///
    /// Cost: one location per instruction in the text, which `clang` turns into a DWARF line
    /// table; the code itself is the same.
    ///
    /// ```
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
    /// let database = Database::default();
    /// let sources = source::SourceMap::new();
    /// let declared = |_: &types::Nominal<'_>| types::Declared::Struct(Vec::new());
    /// let symbol = |_: &types::Item<'_>, _: &[types::Type<'_>]| String::new();
    /// let names = |_: &types::Nominal<'_>| llvm_backend::ClassNames::default();
    /// let mut module =
    ///     llvm_backend::Module::new(&database, &sources, &declared, &names, &symbol);
    /// module.debug_info("/home/ada/heat");
    /// assert!(module.finish().contains("emissionKind: LineTablesOnly"));
    /// ```
    pub fn debug_info(&mut self, directory: &str) {
        self.constants.debug = Some(debug::Debug::new(directory));
    }

    /// Makes integer overflow wrap around instead of panicking, as a release build does
    /// (decision 0030): every [`Assert`](mir::TerminatorKind::Assert) of
    /// [`NoOverflow`](mir::Check::NoOverflow) or [`NoPowerOverflow`](mir::Check::NoPowerOverflow)
    /// is left out ([`Check::is_overflow`](mir::Check::is_overflow)), so the
    /// [`CheckedBinary`](mir::Rvalue::CheckedBinary) before it keeps its wrapped result, and
    /// `prange` reductions of integers combine with wrapping `add` and `mul`. Every other check,
    /// such as bounds and division by zero, stays. Call it before [`define`](Self::define).
    ///
    /// Cost: none at run time; LLVM folds the unused overflow flag into a plain `add`.
    ///
    /// ```
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
    /// let database = Database::default();
    /// let sources = source::SourceMap::new();
    /// let declared = |_: &types::Nominal<'_>| types::Declared::Struct(Vec::new());
    /// let symbol = |_: &types::Item<'_>, _: &[types::Type<'_>]| String::new();
    /// let names = |_: &types::Nominal<'_>| llvm_backend::ClassNames::default();
    /// let mut module =
    ///     llvm_backend::Module::new(&database, &sources, &declared, &names, &symbol);
    /// module.wrap_on_overflow();
    /// assert!(!module.finish().contains("baryon_rt_panic"));
    /// ```
    pub fn wrap_on_overflow(&mut self) {
        self.constants.wraps = true;
    }
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
