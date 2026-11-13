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

use crate::{Profile, Session};
use database::Db;
use diagnostics::{Diagnostic, Severity};
use functions::{Function, Test, is_generic, owners};
use instances::{Instance, Instances, ready};
use std::collections::HashMap;
use types::{Declaration, Item, Nominal, TypeDb, TypeKind};

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
        self.program(root, profile, Program::Main, 1)
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
        self.program(root, profile, Program::Main, parts)
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
        self.program(root, profile, Program::Tests, 1)
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
        self.program(root, profile, Program::Tests, parts)
    }

    /// The error for a `main` function outside the package's `main.byn`.
    fn main_outside_main_module(&self, item: Item<'_>) -> Diagnostic {
        self.at(item, "only `main.byn` may define `main`")
            .with_note(
                "a program starts at the `main` function of the `main.byn` at the package's root; \
             rename this function",
            )
    }

    /// The package as at most `parts` LLVM modules whose C `main` starts `program`, built for
    /// `profile`, with the names of the tests it runs, which are none for [`Program::Main`].
    /// With `parts` 1 it is exactly one module.
    fn program(
        &self,
        root: Option<&str>,
        profile: Profile,
        program: Program,
        parts: usize,
    ) -> Result<(Vec<String>, Vec<String>), Vec<Diagnostic>> {
        let errors: Vec<Diagnostic> = self
            .check()
            .into_iter()
            .filter(|problem| problem.severity() == Severity::Error)
            .collect();
        if !errors.is_empty() {
            return Err(errors);
        }
        let database = &self.database;
        let functions = self.functions();
        let symbols: HashMap<Item<'_>, String> = functions
            .iter()
            .flat_map(|function| {
                std::iter::once((function.item, function.symbol.clone()))
                    .chain(function.defaults.iter().cloned())
            })
            .collect();
        let symbol = |item: &Item<'_>, arguments: &[types::Type<'_>]| {
            let base = symbols.get(item).cloned().unwrap_or_default();
            if arguments.is_empty() {
                return base;
            }
            // A function's own type shows as its signature, which two functions can share, so
            // its symbol names the function instead.
            let shown: Vec<String> = arguments
                .iter()
                .map(|argument| match argument.kind(database) {
                    TypeKind::Function { item, .. } => {
                        symbols.get(item).cloned().unwrap_or_default()
                    }
                    _ => argument.display(database).to_string(),
                })
                .collect();
            format!("{base}[{}]", shown.join(", "))
        };
        let declared = declared(database, self.package);
        let names = names(database, self.package);
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
        let owners = owners(&functions);
        errors.extend(self.define(&mut module, roots, &declared, &symbol, &owners));
        let tests = self.start(&mut module, program, &entries, &mut errors);
        if errors.is_empty() {
            Ok((module.finish_parts(parts), tests))
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

    /// Defines in `module` each instance a program needs: `roots`, then each instance of a
    /// generic function, and each default's function, that a defined body calls. `owners`
    /// maps each default's function to the function it belongs to, whose errors it raises.
    /// Returns one error per function lowering or the backend cannot handle yet.
    fn define<'db>(
        &'db self,
        module: &mut llvm_backend::Module<'_, 'db>,
        roots: Vec<Instance<'db>>,
        declared: &dyn Fn(&Nominal<'db>) -> types::Declared<'db>,
        symbol: &dyn Fn(&Item<'db>, &[types::Type<'db>]) -> String,
        owners: &HashMap<Item<'db>, Item<'db>>,
    ) -> Vec<Diagnostic> {
        let database = &self.database;
        let mut errors = Vec::new();
        let mut instances = Instances::new(roots);
        let package = self.package;
        let implementation = move |method: Item<'db>, implementor: types::Type<'db>| {
            checker::implementation(database, package, method, implementor)
        };
        // Each function's lowering errors once, however many instances of it there are.
        let mut reported = std::collections::HashSet::new();
        while let Some(instance) = instances.next() {
            match ready(database, &instance, declared, &implementation) {
                Ok(body) => {
                    instances.follow(&body);
                    // A default's function raises what the function it belongs to declares.
                    let owner = owners.get(&instance.item).unwrap_or(&instance.item);
                    let raises = match &database.declaration(*owner).declaration {
                        Declaration::Function(signature) => {
                            match signature.returns.kind(database) {
                                TypeKind::Fallible { errors, .. } => errors.as_slice(),
                                _ => &[],
                            }
                        }
                        _ => &[],
                    };
                    let name = symbol(&instance.item, &instance.arguments);
                    if let Err(unsupported) = module.define(&name, &body, raises) {
                        errors.push(self.at(instance.item, unsupported.to_string()));
                    }
                }
                Err(problems) if reported.insert(instance.item) => errors.extend(problems),
                Err(_) => {}
            }
        }
        errors
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
        let message = message.into();
        let tree = self.database.parse(item.file(&self.database)).tree();
        Diagnostic::error(message.clone())
            .with_primary(tree.span(item.name(&self.database)), message)
    }
}

/// What each struct or enum of `package` declares, as layouts, the passes and the backend read it.
fn declared<'db>(
    db: &'db dyn TypeDb,
    package: database::Package,
) -> impl Fn(&Nominal<'db>) -> types::Declared<'db> {
    move |nominal| checker::declared(db, package, nominal)
}

/// The names `print` writes for each struct or enum of `package`: the class's name, then its
/// fields' or members' names in declaration order.
fn names<'db>(
    db: &'db dyn TypeDb,
    package: database::Package,
) -> impl Fn(&Nominal<'db>) -> llvm_backend::ClassNames {
    move |nominal| {
        let bare = Nominal {
            arguments: Vec::new(),
            ..nominal.clone()
        };
        let item = Item::new(db, package, nominal.file, nominal.name);
        let members = match &db.declaration(item).declaration {
            Declaration::Struct { fields, .. } => {
                fields.iter().map(|field| field.name.clone()).collect()
            }
            Declaration::Enum { variants } => variants.clone(),
            _ => Vec::new(),
        };
        llvm_backend::ClassNames {
            class: types::Type::new(db, TypeKind::Struct(bare))
                .display(db)
                .to_string(),
            members,
        }
    }
}
