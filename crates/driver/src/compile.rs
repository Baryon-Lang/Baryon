//! Compiling a package to one LLVM module, for `baryon build`: each function lowered, made ready
//! for a backend, and written as LLVM IR text, with a C `main` that calls the `main` function of
//! the package's `main.byn`.
//!
//! This is the prototype of roadmap 1.11.3. It compiles every function and method that has no
//! type parameters, then collects the instances of generic functions and of methods of generic
//! classes they call (roadmap 1.9.5 and 1.9.18, decision 0018): each call of a generic function at concrete types adds that instance, whose
//! body is the generic body with the types substituted, and whose own calls are followed in turn.
//! An instance's symbol is its function's with the type arguments after it: `main.square[f64]`.
//! A function lowering or the backend cannot handle yet is reported once, at its name.

mod instances;

use crate::{Profile, Session};
use database::Db;
use diagnostics::{Diagnostic, Severity};
use instances::{Instance, Instances, ready};
use std::collections::HashMap;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Declaration, Item, Nominal, TypeDb, TypeKind};

/// One function to compile: the item, its symbol and the span of its name.
struct Function<'db> {
    /// The function or method.
    item: Item<'db>,
    /// Its symbol in the module.
    symbol: String,
    /// Whether it is a top-level function named `main`.
    main: bool,
    /// Whether its module is the package's `main`, from `main.byn` at the root: the only module
    /// whose `main` function a program starts at.
    in_main_module: bool,
    /// Whether it is decorated `@test`, and where.
    test: Test,
    /// The function that computes each default of a parameter, named by the parameter's
    /// `Name` node, with its symbol: the function's own with the
    /// parameter's name after a colon, `main.drift:buffer`, which no name can spell (decision
    /// 0027).
    defaults: Vec<(Item<'db>, String)>,
}

/// Whether a function is decorated `@test`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Test {
    /// It is not a test.
    No,
    /// A top-level function decorated `@test`: a test.
    Function,
    /// A method decorated `@test`, which is an error, since nothing gives the runner a value to
    /// call it on.
    Method,
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
        self.program(root, profile, Program::Main)
            .map(|(module, _)| module)
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
        self.program(root, profile, Program::Tests)
    }

    /// The error for a `main` function outside the package's `main.byn`.
    fn main_outside_main_module(&self, item: Item<'_>) -> Diagnostic {
        self.at(item, "only `main.byn` may define `main`")
            .with_note(
                "a program starts at the `main` function of the `main.byn` at the package's root; \
             rename this function",
            )
    }

    /// The package as one LLVM module whose C `main` starts `program`, built for `profile`, with
    /// the names of the tests it runs, which are none for [`Program::Main`].
    fn program(
        &self,
        root: Option<&str>,
        profile: Profile,
        program: Program,
    ) -> Result<(String, Vec<String>), Vec<Diagnostic>> {
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
        let mut module = llvm_backend::Module::new(database, self, &declared, &symbol);
        if let Some(root) = root {
            module.debug_info(root);
        }
        if profile == Profile::Release {
            module.wrap_on_overflow();
        }
        let mut errors = Vec::new();
        let mut entries = Vec::new();
        let mut roots = Vec::new();
        for function in &functions {
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
            Ok((module.finish(), tests))
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
        // Each function's lowering errors once, however many instances of it there are.
        let mut reported = std::collections::HashSet::new();
        while let Some(instance) = instances.next() {
            match ready(database, &instance, declared) {
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

    /// Every function and method of the package, with its symbol, in file and source order.
    fn functions(&self) -> Vec<Function<'_>> {
        let database = &self.database;
        let mut functions = Vec::new();
        for &file in &self.files {
            let Ok(module) = database::module_name(database, file) else {
                continue;
            };
            let module = module.to_string();
            let tree = database.parse(file).tree();
            let text = file.source(database).text();
            let name = |statement: NodeId| {
                tree.child(statement, Role::Name)
                    .filter(|&name| tree.kind(name) == SyntaxKind::Name)
            };
            let in_main_module = module == "main";
            let is_test = |statement: NodeId| {
                tree.children_with(statement, Role::Decorator)
                    .any(|decorator| {
                        tree.child(decorator, Role::Value).is_some_and(|value| {
                            tree.kind(value) == SyntaxKind::Name && tree.text(value, text) == "test"
                        })
                    })
            };
            let mut push =
                |statement: NodeId, node: NodeId, symbol: String, main: bool, method: bool| {
                    let decorated = match (is_test(statement), method) {
                        (false, _) => Test::No,
                        (true, false) => Test::Function,
                        (true, true) => Test::Method,
                    };
                    let defaults = defaulted(tree, statement)
                        .map(|name| {
                            let item = Item::new(database, self.package, file, name);
                            (item, format!("{symbol}:{}", tree.text(name, text)))
                        })
                        .collect();
                    functions.push(Function {
                        item: Item::new(database, self.package, file, node),
                        symbol,
                        main,
                        in_main_module,
                        test: decorated,
                        defaults,
                    });
                };
            for statement in tree.children_with(tree.root(), Role::Body) {
                let Some(node) = name(statement) else {
                    continue;
                };
                let own = tree.text(node, text);
                match tree.kind(statement) {
                    SyntaxKind::FunctionDefinition => {
                        push(
                            statement,
                            node,
                            format!("{module}.{own}"),
                            own == "main",
                            false,
                        );
                    }
                    SyntaxKind::ClassDefinition => {
                        for method in tree.children_with(statement, Role::Body) {
                            if tree.kind(method) != SyntaxKind::FunctionDefinition {
                                continue;
                            }
                            if let Some(method_name) = name(method) {
                                let symbol =
                                    format!("{module}.{own}.{}", tree.text(method_name, text));
                                push(method, method_name, symbol, false, true);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        functions
    }

    /// The error types a `main` returning `returns` can raise, each with its fields' names,
    /// for the report of an error that ends the program; empty when it cannot fail.
    fn reported<'db>(&'db self, returns: types::Type<'db>) -> Vec<(types::Type<'db>, Vec<String>)> {
        let database = &self.database;
        let TypeKind::Fallible { errors, .. } = returns.kind(database) else {
            return Vec::new();
        };
        errors
            .iter()
            .map(|&error| {
                let fields = match error.kind(database) {
                    TypeKind::Struct(nominal) => {
                        let item = Item::new(database, self.package, nominal.file, nominal.name);
                        match &database.declaration(item).declaration {
                            Declaration::Struct { fields, .. } => {
                                fields.iter().map(|field| field.name.clone()).collect()
                            }
                            _ => Vec::new(),
                        }
                    }
                    _ => Vec::new(),
                };
                (error, fields)
            })
            .collect()
    }

    /// An error at the name of `item`.
    fn at(&self, item: Item<'_>, message: impl Into<String>) -> Diagnostic {
        let message = message.into();
        let tree = self.database.parse(item.file(&self.database)).tree();
        Diagnostic::error(message.clone())
            .with_primary(tree.span(item.name(&self.database)), message)
    }
}

/// The function each default's function in `functions` belongs to.
fn owners<'db>(functions: &[Function<'db>]) -> HashMap<Item<'db>, Item<'db>> {
    functions
        .iter()
        .flat_map(|function| {
            function
                .defaults
                .iter()
                .map(|(default, _)| (*default, function.item))
        })
        .collect()
}

/// The `Name` node of each parameter of the function `function` declares that has a default,
/// which names the function that computes it.
fn defaulted(tree: &syntax::SyntaxTree, function: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    tree.child(function, Role::Parameters)
        .into_iter()
        .flat_map(|parameters| tree.children(parameters))
        .filter(|&parameter| tree.kind(parameter) == SyntaxKind::Parameter)
        .filter(|&parameter| tree.child(parameter, Role::Default).is_some())
        .filter_map(|parameter| tree.child(parameter, Role::Name))
}

/// What each struct or enum of `package` declares, as layouts, the passes and the backend read it.
fn declared<'db>(
    db: &'db dyn TypeDb,
    package: database::Package,
) -> impl Fn(&Nominal<'db>) -> types::Declared<'db> {
    move |nominal| checker::declared(db, package, nominal)
}

/// Whether the function `item`, whose signature is `signature`, is compiled once per call's
/// arguments, from the calls that reach it, rather than once on its own: it has type
/// parameters, it is a method of a class that has them (roadmap 1.9.18), or it has a
/// `Callable` parameter, which is an implicit one (decision 0026).
fn is_generic<'db>(
    database: &'db dyn TypeDb,
    item: Item<'db>,
    signature: &types::Signature<'db>,
) -> bool {
    !signature.type_parameters.is_empty()
        || !instances::class_parameters(database, item).is_empty()
        || signature
            .parameters
            .iter()
            .any(|parameter| matches!(parameter.ty.kind(database), TypeKind::Callable { .. }))
}
