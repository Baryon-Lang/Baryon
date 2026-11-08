//! Compiling a package to one LLVM module, for `baryon build`: each function lowered, made ready
//! for a backend, and written as LLVM IR text, with a C `main` that calls the `main` function of
//! the package's `main.byn`.
//!
//! This is the prototype of roadmap 1.11.3. It compiles every function and method that has no
//! type parameters. Generic functions wait for monomorphization's collection of instances
//! (roadmap 1.9.5), which will replace the loop over functions here; a call to one is an error
//! for now. A function lowering or the backend cannot handle yet is reported once, at its name.

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
        let symbol =
            |item: &Item<'_>, _: &[types::Type<'_>]| symbols.get(item).cloned().unwrap_or_default();
        let declared = declared(database, self.package);
        let mut module = llvm_backend::Module::new(database, self, &declared, &symbol);
        if let Some(root) = root {
            module.debug_info(root);
        }
        let mut errors = Vec::new();
        let mut entries = Vec::new();
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
            match ready(database, function.item, &declared) {
                Ok(body) => {
                    let raises = match signature.returns.kind(database) {
                        TypeKind::Fallible { errors, .. } => errors.as_slice(),
                        _ => &[],
                    };
                    if let Err(unsupported) = module.define(&function.symbol, &body, raises) {
                        errors.push(self.at(function.item, unsupported.to_string()));
                    }
                }
                Err(problems) => errors.extend(problems),
            }
        }
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

/// The body of the non-generic function `item`, ready for a backend: lowered, its drops
/// elaborated and its whole-array expressions expanded. Errors for what lowering cannot handle
/// yet, and for a call to a generic function, which needs monomorphization (roadmap 1.9.5).
fn ready<'db>(
    db: &'db dyn TypeDb,
    item: Item<'db>,
    declared: &dyn Fn(&Nominal<'db>) -> types::Declared<'db>,
) -> Result<Body<'db>, Vec<Diagnostic>> {
    let mut body = lowering::lower(db, item)?;
    let generic_calls: Vec<Diagnostic> = body
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator.kind {
            TerminatorKind::Call {
                callee: Callee::Function { type_arguments, .. },
                ..
            } if !type_arguments.is_empty() => Some(
                Diagnostic::error("calls to generic functions are not compiled yet")
                    .with_primary(block.terminator.span, "this calls a generic function")
                    .with_note("monomorphization comes with roadmap 1.9.5"),
            ),
            _ => None,
        })
        .collect();
    if !generic_calls.is_empty() {
        return Err(generic_calls);
    }
    drop_elaboration::elaborate(db, &mut body, declared);
    array_expansion::expand(db, &mut body, declared);
    body.phase = Phase::Ready;
    Ok(body)
}
