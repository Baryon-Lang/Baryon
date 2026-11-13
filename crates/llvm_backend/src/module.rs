//! [`Module`]: one LLVM module being written, the backend's public entry point. It collects
//! function definitions, the declarations of what they call, and the constants they share, and
//! writes them out as one textual LLVM IR module (decision 0020). Each function body is written
//! by the `function` module and the modules beside it; this one only gathers their text.

use crate::{ClassNames, Test, Unsupported, constants, debug, entry, function, parts, split, ty};
use mir::Body;
use std::collections::{BTreeMap, BTreeSet};
use types::{Declared, Item, Nominal, Type};

/// One LLVM module being written: the functions defined so far, and a declaration for each
/// function they call that the module does not define.
pub struct Module<'a, 'db> {
    /// How to represent each type.
    types: ty::Types<'a, 'db>,
    /// The symbol of a function instance: an item with its type arguments.
    symbol: &'a dyn Fn(&Item<'db>, &[Type<'db>]) -> String,
    /// Each definition, in the order they were made: a function with the outlined `prange`
    /// bodies only it calls, or the C `main`.
    pub(crate) definitions: Vec<Definition>,
    /// The messages and source locations failed checks point to.
    pub(crate) constants: constants::Constants<'a>,
    /// The declaration of each symbol called, written from the call, by symbol.
    pub(crate) declarations: BTreeMap<String, String>,
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
            constants: constants::Constants::new(sources),
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
        let mut constants = self.constants.local();
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
                &mut constants,
                &name,
                body,
                errors,
                internal,
            )?;
            written.push(text);
            calls.extend(called);
        }
        self.constants.adopt(constants.first_file());
        self.definitions.push(Definition {
            symbol: symbol.to_owned(),
            text: written.join("\n"),
            calls: calls.keys().cloned().collect(),
            pieces: constants.pieces(),
        });
        self.declarations.extend(calls);
        Ok(())
    }

    /// Makes the function `symbol` the program's entry point: adds a C `main` that calls it and
    /// returns 0, the exit status of a program that ran to its end, after the runtime writes out
    /// a line `print(..., end="")` left unended. `symbol` takes no arguments and returns `None`;
    /// a failed check inside it ends the program through the runtime with status 101 instead.
    ///
    /// `symbol` is declared, so the C `main` can sit in a different module from it
    /// ([`finish_parts`](Self::finish_parts)); a module that defines `symbol` leaves the
    /// declaration out.
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
    ///      declare void @baryon_rt_flush() nounwind\n\
    ///      declare void @main.main()\n"
    /// );
    /// # Ok::<(), llvm_backend::Unsupported>(())
    /// ```
    pub fn entry(&mut self, symbol: &str, errors: &[Type<'db>]) -> Result<(), Unsupported> {
        let mut constants = self.constants.local();
        let body = entry::no_body(self.types.db);
        let function = function::Function::new(self.types, self.symbol, &mut constants, &body, &[]);
        let (text, calls) = function.entry(symbol, errors)?;
        self.constants.adopt(constants.first_file());
        self.definitions.push(Definition {
            symbol: "main".to_owned(),
            text,
            calls: calls.keys().cloned().collect(),
            pieces: constants.pieces(),
        });
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
        let mut constants = self.constants.local();
        let body = entry::no_body(self.types.db);
        let function = function::Function::new(self.types, self.symbol, &mut constants, &body, &[]);
        let (text, calls) = function.tests(tests)?;
        self.constants.adopt(constants.first_file());
        self.definitions.push(Definition {
            symbol: "main".to_owned(),
            text,
            calls: calls.keys().cloned().collect(),
            pieces: constants.pieces(),
        });
        self.declarations.extend(calls);
        Ok(())
    }

    /// The module's text: every definition in order, then the message and location constants
    /// their checks point to, then a declaration of each function called and not defined here,
    /// in symbol order, so the text is the same on every run.
    #[must_use]
    pub fn finish(self) -> String {
        let all: Vec<usize> = (0..self.definitions.len()).collect();
        self.split(&[all]).concat()
    }

    /// The program as at most `parts` modules that `clang` can compile at once, one per core,
    /// and link together (roadmap 2.6). Each module holds some of the definitions, whole, with
    /// every constant, a declaration of each function called and defined in another module, and
    /// the line tables; each function is defined in exactly one module. A program with little
    /// code comes back as one module, the same text [`finish`](Self::finish) writes, since a
    /// module of its own is worth a `clang` only past about 200 KB of function text.
    ///
    /// Splitting changes no result: each function's text is the same in any module.
    ///
    /// Cost: the constants and line-table metadata are written once per module.
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
    /// let parts = module.finish_parts(4);
    /// assert_eq!(parts.len(), 1, "a C `main` alone is too small to split");
    /// assert!(parts[0].starts_with("define i32 @main()"), "{}", parts[0]);
    /// # Ok::<(), llvm_backend::Unsupported>(())
    /// ```
    #[must_use]
    pub fn finish_parts(self, parts: usize) -> Vec<String> {
        let sizes: Vec<usize> = self
            .definitions
            .iter()
            .map(|definition| definition.text.len())
            .collect();
        let assigned = parts::assign(&sizes, parts);
        self.split(&assigned)
    }

    /// The program as modules whose text an edit changes only where it reaches, for a compiler
    /// that keeps each module's machine code and recompiles only the modules that changed
    /// (`baryon watch`, roadmap 2.2.2). Each module holds a run of definitions in the order
    /// they were made, ended by its last symbol's hash or its size, so an edit that grows or
    /// adds a function moves the ends of a few runs at most. Each module carries only the
    /// constants, declarations and line-table metadata its functions point to, numbered within
    /// the module, so its text depends on its functions alone. A small program is one module.
    ///
    /// Splitting changes no result, as for [`finish_parts`](Self::finish_parts).
    ///
    /// Cost: one more pass over the text, to number each module's constants and metadata.
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
    /// let chunks = module.finish_chunks();
    /// assert_eq!(chunks.len(), 1, "a C `main` alone is one chunk");
    /// assert!(chunks[0].starts_with("define i32 @main()"), "{}", chunks[0]);
    /// # Ok::<(), llvm_backend::Unsupported>(())
    /// ```
    #[must_use]
    pub fn finish_chunks(self) -> Vec<String> {
        let symbols: Vec<&str> = self
            .definitions
            .iter()
            .map(|definition| definition.symbol.as_str())
            .collect();
        let sizes: Vec<usize> = self
            .definitions
            .iter()
            .map(|definition| definition.text.len())
            .collect();
        let assigned = parts::chunks(&symbols, &sizes);
        self.split(&assigned)
    }

    /// One module's text for each list of definition indices in `assigned`, each carrying only
    /// the constants, declarations and metadata its functions point to, numbered within it
    /// (the `split` module).
    fn split(self, assigned: &[Vec<usize>]) -> Vec<String> {
        let (header, reserved) = self.constants.header();
        assigned
            .iter()
            .map(|indices| {
                let definitions = indices.iter().map(|&index| {
                    let definition = &self.definitions[index];
                    (definition.symbol.as_str(), &definition.calls)
                });
                let declared = split::declared(definitions);
                split::module(
                    indices.iter().map(|&index| {
                        let definition = &self.definitions[index];
                        (definition.text.as_str(), &definition.pieces)
                    }),
                    declared
                        .into_iter()
                        .filter_map(|symbol| self.declarations.get(symbol))
                        .map(String::as_str),
                    &header,
                    reserved,
                )
            })
            .collect()
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

/// One definition in a [`Module`]: a function, with the outlined `prange` bodies and chunk
/// functions only it calls, or the C `main`. A definition is never split across modules, since
/// its outlined functions are internal to the module that holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Definition {
    /// The symbol of the function, or `main` for the C `main`; other modules declare it.
    symbol: String,
    /// The text of every function in the definition.
    text: String,
    /// The symbols of the functions it calls, which a module that does not define them
    /// declares.
    calls: BTreeSet<String>,
    /// The constants and metadata nodes it made, numbered within it.
    pieces: split::Pieces,
}
