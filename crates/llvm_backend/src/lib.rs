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
//! - **Checks become traps.** A failed [`Assert`](mir::TerminatorKind::Assert), or a conversion
//!   that loses its value (language reference, "Conversions"), branches to one block that calls
//!   `llvm.trap`. The run-time's panic messages (roadmap 1.12) replace it.
//! - **Integer arithmetic wraps**, as [`Binary`](mir::Rvalue::Binary) defines, and never reaches
//!   LLVM's undefined cases: `x // -1` and `x % -1` are computed without `sdiv`'s overflow, and a
//!   shift by the width or more gives what shifting one bit at a time would.
//! - **Float `//` and `%` call the C math library**: LLVM compiles `frem` and `llvm.floor` to
//!   `fmod` and `floor`, so a program links `libm`, as C programs do.
//!
//! This first slice (roadmap 1.10.1) covers numbers, `bool`, enums without data, tuples and
//! structs, control flow, direct calls and checks. A body that uses anything else, such as an
//! array or a call that can raise, gets an [`Unsupported`] error naming it.
//!
//! # Example
//!
//! `def double(x: f64) -> f64: return x * 2.0`:
//!
//! ```
//! use mir::{BinaryOperator, Body, Constant, LocalKind, Operand, Place, Rvalue, Statement,
//!     StatementKind, Terminator, TerminatorKind};
//! use source::{FileId, Span};
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
//! let declared = |_: &types::Nominal<'_>| Declared::Struct(Vec::new());
//! let symbol = |_: &types::Item<'_>, _: &[Type<'_>]| String::new();
//! let mut module = llvm_backend::Module::new(&database, &declared, &symbol);
//! module.define("double", &body)?;
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
//! # Ok::<(), llvm_backend::Unsupported>(())
//! ```

mod convert;
mod function;
mod rvalue;
mod ty;

use mir::Body;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use types::{Declared, Item, Nominal, Type};

/// One LLVM module being written: the functions defined so far, and a declaration for each
/// function they call that the module does not define.
pub struct Module<'a, 'db> {
    /// How to represent each type.
    types: ty::Types<'a, 'db>,
    /// The symbol of a function instance: an item with its type arguments.
    symbol: &'a dyn Fn(&Item<'db>, &[Type<'db>]) -> String,
    /// Each function definition's text, in the order they were defined.
    definitions: Vec<String>,
    /// The symbols this module defines, so they are not declared too.
    defined: BTreeSet<String>,
    /// The declaration of each symbol called, written from the call, by symbol.
    declarations: BTreeMap<String, String>,
}

impl<'a, 'db> Module<'a, 'db> {
    /// An empty module. `declared` says what each struct or enum declares, with its type
    /// arguments substituted, as [`types::layout`] needs; `symbol` names the instance of a
    /// function at its type arguments, the same way for every module of a program so calls
    /// between modules link.
    pub fn new(
        db: &'db dyn database::Db,
        declared: &'a dyn Fn(&Nominal<'db>) -> Declared<'db>,
        symbol: &'a dyn Fn(&Item<'db>, &[Type<'db>]) -> String,
    ) -> Self {
        Self {
            types: ty::Types { db, declared },
            symbol,
            definitions: Vec::new(),
            defined: BTreeSet::new(),
            declarations: BTreeMap::new(),
        }
    }

    /// Adds the function `symbol` with the body `body`, which must be in phase
    /// [`Ready`](mir::Phase::Ready) and valid.
    ///
    /// Cost: one pass over the body, writing text proportional to its size.
    ///
    /// # Errors
    ///
    /// [`Unsupported`] when the body uses a type or form this slice of the backend does not
    /// translate yet; the module is left as it was.
    pub fn define(&mut self, symbol: &str, body: &Body<'db>) -> Result<(), Unsupported> {
        let (text, calls) = function::Function::write(self.types, self.symbol, symbol, body)?;
        self.definitions.push(text);
        self.defined.insert(symbol.to_owned());
        self.declarations.extend(calls);
        Ok(())
    }

    /// The module's text: every definition in order, then a declaration of each function called
    /// and not defined here, in symbol order, so the text is the same on every run.
    #[must_use]
    pub fn finish(self) -> String {
        let mut text = self.definitions.join("\n");
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
        text
    }
}

/// A form of the mid-level IR this slice of the backend does not translate yet, such as values
/// of an array type. Roadmap 1.10 adds them slice by slice; a type-checked program reaches this
/// only through features the backend has not caught up with.
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
