//! Compiling a package to one LLVM module, for `baryon build`: each function lowered, made ready
//! for a backend, and written as LLVM IR text, with a C `main` that calls the `main` function of
//! the package's `main.byn`.
//!
//! This is the prototype of roadmap 1.11.3. It compiles every function and method that has no
//! type parameters, then collects the instances of generic functions and of methods of generic
//! classes they call (roadmap 1.9.5 and 1.9.18, decision 0018): each call of a generic function
//! at concrete types adds that instance, whose body is the generic body with the types
//! substituted, and whose own calls are followed in turn. It also compiles the function that
//! computes each default a call leaves out, when that call is found (decision 0027).
//! An instance's symbol is its function's with the type arguments after it: `main.square[f64]`.
//! A function lowering or the backend cannot handle yet is reported once, at its name.

pub(crate) mod functions;
mod instances;
mod queries;

use crate::{Database, Profile, Session};
use diagnostics::{Diagnostic, Severity};
use functions::{Function, Test, is_generic};
use instances::{Instance, Instances};
use types::{Declaration, Item, TypeDb, TypeKind};

/// How a compiled program is split into LLVM modules.
#[derive(Clone, Copy)]
enum Split {
    /// At most this many modules of about equal size, one per core
    /// ([`llvm_backend::Module::finish_parts`]).
    Parts(usize),
    /// Modules an edit changes only where it reaches
    /// ([`llvm_backend::Module::finish_chunks`]).
    Chunks,
}

impl Split {
    /// The text of `module`'s modules, split this way.
    fn finish(self, module: llvm_backend::Module<'_, '_>) -> Vec<String> {
        match self {
            Self::Parts(parts) => module.finish_parts(parts),
            Self::Chunks => module.finish_chunks(),
        }
    }
}

/// What a compiled program starts at.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Program {
    /// The `main` function of the package's `main.byn`: `baryon build`.
    Main,
    /// One `@test` function per run, chosen by the command line: `baryon test`.
    Tests,
}

impl Session {
    /// The package as one LLVM module, in LLVM's textual IR (decision 0020), with a C `main` that
    /// runs the `main` function of the package's `main.byn`. `clang` compiles and links it with the runtime.
    ///
    /// Each function's symbol is its module's name and its own, joined by dots, with a method's
    /// class between them: `physics.nbody.step`, `physics.nbody.Body.energy`. Dots keep them
    /// apart from every C symbol.
    ///
    /// `root` is the package's directory, absolute, when the module should carry line tables
    /// that point debuggers, profilers and backtraces back to the source files under it; `None`
    /// leaves them out. `profile` says whether integer overflow panics or wraps ([`Profile`]).
    ///
    /// ```
    /// use driver::{Profile, Session};
    ///
    /// let mut session = Session::new();
    /// session.add_file("main.byn", "def main():\n    assert 6 * 7 == 42\n")?;
    /// let module = session.compile(None, Profile::Debug).unwrap();
    /// assert!(module.contains("define void @main.main()"), "{module}");
    /// assert!(module.contains("define i32 @main()"), "{module}");
    /// # Ok::<(), source::SourceError>(())
    /// ```
    ///
    /// Cost: the type check of the whole package, then lowering, the passes and the backend over
    /// each function once.
    ///
    /// # Errors
    ///
    /// The package's errors when it does not check; otherwise one error per function that
    /// cannot be compiled yet, and an error when `main.byn` has no `main` function, when another
    /// module defines one, or when it takes parameters or returns a value. `main` may raise; an
    /// error that reaches its end is printed and the program exits with status 1.
    pub fn compile(&self, root: Option<&str>, profile: Profile) -> Result<String, Vec<Diagnostic>> {
        self.program(root, profile, Program::Main, Split::Parts(1))
            .map(|(modules, _)| modules.concat())
    }

    /// The package as [`compile`](Self::compile) writes it, split into at most `parts` LLVM
    /// modules that `clang` can compile at once, one per core, and link together (roadmap 2.6,
    /// [`llvm_backend::Module::finish_parts`]). A small package comes back as one module.
    /// `baryon` splits only debug builds: `clang` never inlines a call into another module.
    ///
    /// ```
    /// use driver::{Profile, Session};
    ///
    /// let mut session = Session::new();
    /// session.add_file("main.byn", "def main():\n    assert 6 * 7 == 42\n")?;
    /// let modules = session.compile_parts(None, Profile::Debug, 4).unwrap();
    /// assert_eq!(modules, [session.compile(None, Profile::Debug).unwrap()]);
    /// # Ok::<(), source::SourceError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// The same as [`compile`](Self::compile).
    pub fn compile_parts(
        &self,
        root: Option<&str>,
        profile: Profile,
        parts: usize,
    ) -> Result<Vec<String>, Vec<Diagnostic>> {
        self.program(root, profile, Program::Main, Split::Parts(parts))
            .map(|(modules, _)| modules)
    }

    /// The package as [`compile`](Self::compile) writes it, split into LLVM modules whose text
    /// an edit changes only where it reaches, for `baryon watch`, which keeps each module's
    /// machine code and compiles again only the modules whose text changed (roadmap 2.2.2,
    /// [`llvm_backend::Module::finish_chunks`]). A small package comes back as one module.
    ///
    /// ```
    /// use driver::{Profile, Session};
    ///
    /// let mut session = Session::new();
    /// session.add_file("main.byn", "def main():\n    assert 6 * 7 == 42\n")?;
    /// let modules = session.compile_chunks(None, Profile::Debug).unwrap();
    /// assert_eq!(modules.len(), 1);
    /// assert!(modules[0].contains("define void @main.main()"), "{}", modules[0]);
    /// # Ok::<(), source::SourceError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// The same as [`compile`](Self::compile).
    pub fn compile_chunks(
        &self,
        root: Option<&str>,
        profile: Profile,
    ) -> Result<Vec<String>, Vec<Diagnostic>> {
        self.program(root, profile, Program::Main, Split::Chunks)
            .map(|(modules, _)| modules)
    }

    /// The package as one LLVM module for `baryon test`, with the name of each `@test` function
    /// in it, as `module.function`, in file and source order. The module's C `main` runs the test
    /// at the position its first command-line argument names, counting from 0, and exits with
    /// status 0 when the test returns, 1 after printing an error the test raised, and 101 when a
    /// check in it fails ([`llvm_backend::Module::tests`]). Running each test in its own process
    /// keeps a panic in one from stopping the rest.
    ///
    /// A test is a top-level function decorated `@test` that takes no parameters and returns
    /// `None`; it may raise. The package needs no `main` function; every function is compiled as
    /// it is for [`compile`](Self::compile), and `root` and `profile` mean the same.
    ///
    /// ```
    /// use driver::{Profile, Session};
    ///
    /// let mut session = Session::new();
    /// session.add_file("heat.byn", "@test\ndef test_flux():\n    assert 2 + 2 == 4\n")?;
    /// let (module, tests) = session.compile_tests(None, Profile::Debug).unwrap();
    /// assert_eq!(tests, ["heat.test_flux"]);
    /// assert!(module.contains("define i32 @main(i32 %count, ptr %arguments)"), "{module}");
    /// # Ok::<(), source::SourceError>(())
    /// ```
    ///
    /// Cost: the same as [`compile`](Self::compile).
    ///
    /// # Errors
    ///
    /// The package's errors when it does not check; otherwise one error per function that
    /// cannot be compiled yet, and one per `@test` that is a method, has type parameters, takes
    /// parameters or returns a value.
    pub fn compile_tests(
        &self,
        root: Option<&str>,
        profile: Profile,
    ) -> Result<(String, Vec<String>), Vec<Diagnostic>> {
        self.program(root, profile, Program::Tests, Split::Parts(1))
            .map(|(modules, tests)| (modules.concat(), tests))
    }

    /// The test program as [`compile_tests`](Self::compile_tests) writes it, split into at most
    /// `parts` LLVM modules as [`compile_parts`](Self::compile_parts) splits a program.
    ///
    /// ```
    /// use driver::{Profile, Session};
    ///
    /// let mut session = Session::new();
    /// session.add_file("heat.byn", "@test\ndef test_flux():\n    assert 2 + 2 == 4\n")?;
    /// let (modules, tests) = session.compile_tests_parts(None, Profile::Debug, 4).unwrap();
    /// assert_eq!(modules.len(), 1);
    /// assert_eq!(tests, ["heat.test_flux"]);
    /// # Ok::<(), source::SourceError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// The same as [`compile_tests`](Self::compile_tests).
    pub fn compile_tests_parts(
        &self,
        root: Option<&str>,
        profile: Profile,
        parts: usize,
    ) -> Result<(Vec<String>, Vec<String>), Vec<Diagnostic>> {
        self.program(root, profile, Program::Tests, Split::Parts(parts))
    }

    /// The error for a `main` function outside the package's `main.byn`.
    fn main_outside_main_module(&self, item: Item<'_>) -> Diagnostic {
        self.at(item, "only `main.byn` may define `main`")
            .with_note(
                "a program starts at the `main` function of the `main.byn` at the package's root; \
             rename this function",
            )
    }

    /// The package as LLVM modules split as `split` says, whose C `main` starts `program`,
    /// built for `profile`, with the names of the tests it runs, which are none for
    /// [`Program::Main`]. With [`Split::Parts`] of 1 it is exactly one module.
    fn program(
        &self,
        root: Option<&str>,
        profile: Profile,
        program: Program,
        split: Split,
    ) -> Result<(Vec<String>, Vec<String>), Vec<Diagnostic>> {
        let errors: Vec<Diagnostic> = self
            .check()
            .into_iter()
            .filter(|problem| problem.severity() == Severity::Error)
            .collect();
        if !errors.is_empty() {
            return Err(errors);
        }
        let database: &dyn TypeDb = &self.database;
        let functions = self.functions();
        let package = self.package;
        let symbol = |item: &Item<'_>, arguments: &[types::Type<'_>]| {
            queries::symbol(database, package, *item, arguments)
        };
        let declared = queries::declared(database, package);
        let names = queries::names(database, package);
        let mut module = llvm_backend::Module::new(database, self, &declared, &names, &symbol);
        if let Some(root) = root {
            module.debug_info(root);
        }
        if profile == Profile::Release {
            module.wrap_on_overflow();
        }
        let mut errors = Vec::new();
        let mut entries = Vec::new();
        let mut roots = Vec::new();
        // A library function is never a root: it is compiled only when a root calls it.
        for function in functions.iter().filter(|function| !function.library) {
            let Declaration::Function(signature) = &database.declaration(function.item).declaration
            else {
                continue;
            };
            let entry = match program {
                Program::Main => function.main,
                Program::Tests => function.test == Test::Function,
            };
            if function.test == Test::Method && program == Program::Tests {
                errors.push(self.at(function.item, "`@test` goes on a top-level function"));
                continue;
            }
            if is_generic(database, function.item, signature) {
                if entry && program == Program::Tests {
                    errors.push(self.at(function.item, "a test cannot have type parameters"));
                }
                continue;
            }
            if function.main && !function.in_main_module {
                errors.push(self.main_outside_main_module(function.item));
                continue;
            }
            if entry {
                let returns_none = match signature.returns.kind(database) {
                    TypeKind::None => true,
                    TypeKind::Fallible { value, .. } => {
                        matches!(value.kind(database), TypeKind::None)
                    }
                    _ => false,
                };
                if signature.parameters.is_empty() && returns_none {
                    entries.push((function, signature.returns));
                } else {
                    let message = match program {
                        Program::Main => "`main` must take no parameters and return `None`",
                        Program::Tests => "a test must take no parameters and return `None`",
                    };
                    errors.push(self.at(function.item, message));
                    continue;
                }
            }
            roots.push(Instance {
                item: function.item,
                arguments: Vec::new(),
            });
        }
        let settings = queries::Settings::new(
            database,
            package,
            root.map(str::to_owned),
            profile == Profile::Release,
        );
        prepare(&self.database, settings, &roots);
        errors.extend(define(database, &mut module, settings, roots));
        let tests = self.start(&mut module, program, &entries, &mut errors);
        if errors.is_empty() {
            Ok((split.finish(module), tests))
        } else {
            Err(errors)
        }
    }

    /// Adds to `module` the C `main` that starts `program` at `entries`, the functions it may
    /// start at with their return types, and returns the names of the tests it runs, which are
    /// none for [`Program::Main`]. Adds an error to `errors` when there is no `main` to start
    /// at, or when the backend cannot write the C `main`.
    fn start<'db>(
        &'db self,
        module: &mut llvm_backend::Module<'_, 'db>,
        program: Program,
        entries: &[(&Function<'db>, types::Type<'db>)],
        errors: &mut Vec<Diagnostic>,
    ) -> Vec<String> {
        match program {
            // `main.byn` defines `main` at most once: a second top-level definition is a check
            // error.
            Program::Main => {
                match entries.first() {
                    Some((main, returns)) => {
                        let reported = self.reported(*returns);
                        if let Err(unsupported) = module.entry(&main.symbol, &reported) {
                            errors.push(self.at(main.item, unsupported.to_string()));
                        }
                    }
                    None => errors.push(
                        Diagnostic::error("the package has no `main` function to run")
                            .with_note("add `def main():` to `main.byn` at the package's root"),
                    ),
                }
                Vec::new()
            }
            Program::Tests => {
                let reported: Vec<_> = entries
                    .iter()
                    .map(|(_, returns)| self.reported(*returns))
                    .collect();
                let runs: Vec<llvm_backend::Test<'_, 'db>> = entries
                    .iter()
                    .zip(&reported)
                    .map(|((test, _), reported)| (test.symbol.as_str(), reported.as_slice()))
                    .collect();
                if let Err(unsupported) = module.tests(&runs) {
                    errors.push(Diagnostic::error(unsupported.to_string()));
                }
                entries
                    .iter()
                    .map(|(test, _)| test.symbol.clone())
                    .collect()
            }
        }
    }

    /// The error types a `main` returning `returns` can raise, for the report of an error that
    /// ends the program; empty when it cannot fail.
    fn reported<'db>(&'db self, returns: types::Type<'db>) -> Vec<types::Type<'db>> {
        match returns.kind(&self.database) {
            TypeKind::Fallible { errors, .. } => errors.clone(),
            _ => Vec::new(),
        }
    }

    /// An error at the name of `item`.
    fn at(&self, item: Item<'_>, message: impl Into<String>) -> Diagnostic {
        queries::at(&self.database, item, message)
    }
}

/// Adds to `module` each instance a program needs: `roots`, then each instance of a generic
/// function, and each default's function, that a defined body calls. Each instance's body and
/// text are the database's queries ([`queries::written`]), so a database kept between builds
/// writes again only what an edit reached. Returns one error per function lowering or the
/// backend cannot handle yet.
fn define<'db>(
    db: &'db dyn TypeDb,
    module: &mut llvm_backend::Module<'_, 'db>,
    settings: queries::Settings<'db>,
    roots: Vec<Instance<'db>>,
) -> Vec<Diagnostic> {
    let mut errors = Vec::new();
    let mut instances = Instances::new(roots);
    // Each function's lowering errors once, however many instances of it there are.
    let mut reported = std::collections::HashSet::new();
    while let Some(instance) = instances.next() {
        let wanted = queries::Wanted::new(db, instance.item, instance.arguments);
        match queries::ready_body(db, settings, wanted) {
            queries::Ready::Body(body) => {
                instances.follow(body);
                match queries::written(db, settings, wanted) {
                    Some(Ok(written)) => module.absorb(written),
                    Some(Err(error)) => errors.push(error.clone()),
                    None => {}
                }
            }
            queries::Ready::Errors(problems) if reported.insert(*wanted.item(db)) => {
                errors.extend(problems.iter().cloned());
            }
            queries::Ready::Errors(_) => {}
        }
    }
    errors
}

/// Runs the queries of each of `roots` on every core at once, so [`define`] finds their bodies
/// and text ready. Each thread works on its own handle to the database; `salsa` shares the
/// results between them (roadmap 2.2.2).
fn prepare<'db>(
    database: &'db Database,
    settings: queries::Settings<'db>,
    roots: &[Instance<'db>],
) {
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    if threads < 2 || roots.len() < 2 {
        return;
    }
    let size = roots.len().div_ceil(threads);
    std::thread::scope(|scope| {
        for chunk in roots.chunks(size) {
            let handle = database.clone();
            let items: Vec<(Item<'db>, Vec<types::Type<'db>>)> = chunk
                .iter()
                .map(|instance| (instance.item, instance.arguments.clone()))
                .collect();
            scope.spawn(move || {
                let db: &dyn TypeDb = &handle;
                for (item, arguments) in items {
                    let wanted = queries::Wanted::new(db, item, arguments);
                    queries::written(db, settings, wanted);
                }
            });
        }
    });
}
