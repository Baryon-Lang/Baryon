//! Compiling a package to one LLVM module, for `baryon build`: each function lowered, made ready
//! for a backend, and written as LLVM IR text, with a C `main` that calls the `main` function of
//! the package's `main.byn`.
//!
//! This is the prototype of roadmap 1.11.3. It compiles every function and method that has no
//! type parameters, then collects the instances of generic functions they call (roadmap 1.9.5,
//! decision 0018): each call of a generic function at concrete types adds that instance, whose
//! body is the generic body with the types substituted, and whose own calls are followed in turn.
//! An instance's symbol is its function's with the type arguments after it: `main.square[f64]`.
//! A function lowering or the backend cannot handle yet is reported once, at its name.

use crate::Session;
use database::Db;
use diagnostics::{Diagnostic, Severity};
use mir::{Body, Callee, Phase, TerminatorKind};
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
    /// leaves them out.
    ///
    /// ```
    /// use driver::Session;
    ///
    /// let mut session = Session::new();
    /// session.add_file("main.byn", "def main():\n    assert 6 * 7 == 42\n")?;
    /// let module = session.compile(None).unwrap();
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
    pub fn compile(&self, root: Option<&str>) -> Result<String, Vec<Diagnostic>> {
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
            .map(|function| (function.item, function.symbol.clone()))
            .collect();
        let symbol = |item: &Item<'_>, arguments: &[types::Type<'_>]| {
            let base = symbols.get(item).cloned().unwrap_or_default();
            if arguments.is_empty() {
                return base;
            }
            let shown: Vec<String> = arguments
                .iter()
                .map(|argument| argument.display(database).to_string())
                .collect();
            format!("{base}[{}]", shown.join(", "))
        };
        let declared = declared(database, self.package);
        let mut module = llvm_backend::Module::new(database, self, &declared, &symbol);
        if let Some(root) = root {
            module.debug_info(root);
        }
        let mut errors = Vec::new();
        let mut entries = Vec::new();
        let mut roots = Vec::new();
        for function in &functions {
            let Declaration::Function(signature) = &database.declaration(function.item).declaration
            else {
                continue;
            };
            if !signature.type_parameters.is_empty() {
                continue;
            }
            if function.main && !function.in_main_module {
                errors.push(
                    self.at(function.item, "only `main.byn` may define `main`")
                        .with_note(
                            "a program starts at the `main` function of the `main.byn` at the \
                             package's root; rename this function",
                        ),
                );
                continue;
            }
            if function.main {
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
                    errors.push(self.at(
                        function.item,
                        "`main` must take no parameters and return `None`",
                    ));
                    continue;
                }
            }
            roots.push(Instance {
                item: function.item,
                arguments: Vec::new(),
            });
        }
        errors.extend(self.define(&mut module, roots, &declared, &symbol));
        // `main.byn` defines `main` at most once: a second top-level definition is a check error.
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
        if errors.is_empty() {
            Ok(module.finish())
        } else {
            Err(errors)
        }
    }

    /// Defines in `module` each instance a program needs: `roots`, then each instance of a
    /// generic function that a defined body calls. Returns one error per function lowering or the
    /// backend cannot handle yet.
    fn define<'db>(
        &'db self,
        module: &mut llvm_backend::Module<'_, 'db>,
        roots: Vec<Instance<'db>>,
        declared: &dyn Fn(&Nominal<'db>) -> types::Declared<'db>,
        symbol: &dyn Fn(&Item<'db>, &[types::Type<'db>]) -> String,
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
                    let Declaration::Function(signature) =
                        &database.declaration(instance.item).declaration
                    else {
                        continue;
                    };
                    let raises = match signature.returns.kind(database) {
                        TypeKind::Fallible { errors, .. } => errors.as_slice(),
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
            let mut push = |node: NodeId, symbol: String, main: bool| {
                functions.push(Function {
                    item: Item::new(database, self.package, file, node),
                    symbol,
                    main,
                    in_main_module,
                });
            };
            for statement in tree.children_with(tree.root(), Role::Body) {
                let Some(node) = name(statement) else {
                    continue;
                };
                let own = tree.text(node, text);
                match tree.kind(statement) {
                    SyntaxKind::FunctionDefinition => {
                        push(node, format!("{module}.{own}"), own == "main");
                    }
                    SyntaxKind::ClassDefinition => {
                        for method in tree.children_with(statement, Role::Body) {
                            if tree.kind(method) != SyntaxKind::FunctionDefinition {
                                continue;
                            }
                            if let Some(method_name) = name(method) {
                                let symbol =
                                    format!("{module}.{own}.{}", tree.text(method_name, text));
                                push(method_name, symbol, false);
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

/// What each struct or enum of `package` declares, as layouts, the passes and the backend read it.
fn declared<'db>(
    db: &'db dyn TypeDb,
    package: database::Package,
) -> impl Fn(&Nominal<'db>) -> types::Declared<'db> {
    move |nominal| checker::declared(db, package, nominal)
}

/// A function and the type arguments it is compiled at: none for a function that is not
/// generic (decision 0018).
#[derive(Clone, PartialEq, Eq, Hash)]
struct Instance<'db> {
    /// The function or method.
    item: Item<'db>,
    /// One concrete type per type parameter, in the order the signature lists them.
    arguments: Vec<types::Type<'db>>,
}

/// The instances a program needs, collected as they are found: the roots first, then each
/// instance of a generic function that a collected body calls, once.
struct Instances<'db> {
    /// Every instance found so far, in the order found.
    found: Vec<Instance<'db>>,
    /// The same instances, to find a repeat in constant time.
    seen: std::collections::HashSet<Instance<'db>>,
    /// How many of `found` [`next`](Self::next) has handed out.
    handed: usize,
}

impl<'db> Instances<'db> {
    /// Starts from `roots`, each compiled whether or not anything calls it.
    fn new(roots: Vec<Instance<'db>>) -> Self {
        Self {
            seen: roots.iter().cloned().collect(),
            found: roots,
            handed: 0,
        }
    }

    /// The next instance to compile, or `None` when every instance found so far has been.
    fn next(&mut self) -> Option<Instance<'db>> {
        let instance = self.found.get(self.handed)?.clone();
        self.handed += 1;
        Some(instance)
    }

    /// Adds each instance of a generic function that `body` calls and is not yet found. The body
    /// is an instance's, so its type arguments are concrete.
    fn follow(&mut self, body: &Body<'db>) {
        for block in &body.blocks {
            if let TerminatorKind::Call {
                callee:
                    Callee::Function {
                        item,
                        type_arguments,
                    },
                ..
            } = &block.terminator.kind
                && !type_arguments.is_empty()
            {
                let instance = Instance {
                    item: *item,
                    arguments: type_arguments.clone(),
                };
                if self.seen.insert(instance.clone()) {
                    self.found.push(instance);
                }
            }
        }
    }
}

/// The body of `instance`, ready for a backend: lowered, its type parameters replaced by the
/// instance's arguments, its drops elaborated and its whole-array expressions expanded. Errors
/// for what lowering cannot handle yet, and for a generic method of a generic class, whose
/// class's type parameters are not collected yet.
fn ready<'db>(
    db: &'db dyn TypeDb,
    instance: &Instance<'db>,
    declared: &dyn Fn(&Nominal<'db>) -> types::Declared<'db>,
) -> Result<Body<'db>, Vec<Diagnostic>> {
    let mut body = lowering::lower(db, instance.item)?;
    if !instance.arguments.is_empty() {
        let Declaration::Function(signature) = &db.declaration(instance.item).declaration else {
            return Err(Vec::new());
        };
        if signature.type_parameters.len() != instance.arguments.len() {
            return Err(vec![
                Diagnostic::error("methods of generic classes are not compiled yet")
                    .with_primary(body.span, "a method of a generic class")
                    .with_note("collecting a class's type arguments comes after roadmap 1.9.5"),
            ]);
        }
        body =
            monomorphize::instantiate(db, &body, &signature.type_parameters, &instance.arguments);
    }
    drop_elaboration::elaborate(db, &mut body, declared);
    array_expansion::expand(db, &mut body, declared);
    body.phase = Phase::Ready;
    Ok(body)
}
