//! The functions a package compiles: every function and method of its files and of the
//! standard library's, each with its symbol, whether it is `main` or a test, and the functions
//! that compute its defaults (decision 0027), and which of them are generic, so compiled once
//! per call's type arguments rather than as roots (decision 0018).

use crate::Session;
use database::Db;
use std::collections::HashMap;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Declaration, Item, TypeDb, TypeKind};

/// One function to compile: the item, its symbol and the span of its name.
pub(super) struct Function<'db> {
    /// The function or method.
    pub(super) item: Item<'db>,
    /// Its symbol in the module.
    pub(super) symbol: String,
    /// Whether it is a top-level function named `main`.
    pub(super) main: bool,
    /// Whether its module is the package's `main`, from `main.byn` at the root: the only module
    /// whose `main` function a program starts at.
    pub(super) in_main_module: bool,
    /// Whether it is decorated `@test`, and where.
    pub(super) test: Test,
    /// The function that computes each default of a parameter, named by the parameter's
    /// `Name` node, with its symbol: the function's own with the
    /// parameter's name after a colon, `main.drift:buffer`, which no name can spell (decision
    /// 0027).
    pub(super) defaults: Vec<(Item<'db>, String)>,
    /// Whether it is the standard library's (decision 0038). A library function is compiled
    /// only into a program that calls it, never as a root, so an unused one costs nothing.
    pub(super) library: bool,
}

/// Whether a function is decorated `@test`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Test {
    /// It is not a test.
    No,
    /// A top-level function decorated `@test`: a test.
    Function,
    /// A method decorated `@test`, which is an error, since nothing gives the runner a value to
    /// call it on.
    Method,
}

impl Session {
    /// Every function and method of the package, with its symbol, in file and source order.
    pub(super) fn functions(&self) -> Vec<Function<'_>> {
        let database = &self.database;
        let mut functions = Vec::new();
        for &file in self.library.iter().chain(&self.files) {
            // A library file's module is `<std>.linalg`, which no program's module can spell, so
            // its symbols never clash with the program's own.
            let library = database::library_module(file.source(database).name());
            let module = match library {
                Some(name) => format!("<std>.{name}"),
                None => match database::module_name(database, file) {
                    Ok(module) => module.to_string(),
                    Err(_) => continue,
                },
            };
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
                        library: library.is_some(),
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
                        // A trait's methods have no bodies: each instance of a generic function
                        // calls the implementing type's method instead (decision 0031).
                        let class = Item::new(database, self.package, file, node);
                        if matches!(
                            database.declaration(class).declaration,
                            Declaration::Trait { .. }
                        ) {
                            continue;
                        }
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
}

/// The function each default's function in `functions` belongs to.
pub(super) fn owners<'db>(functions: &[Function<'db>]) -> HashMap<Item<'db>, Item<'db>> {
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

/// Whether the function `item`, whose signature is `signature`, is compiled once per call's
/// arguments, from the calls that reach it, rather than once on its own: it has type
/// parameters, it is a method of a class that has them (roadmap 1.9.18), or it has a
/// `Callable` parameter, which is an implicit one (decision 0026).
pub(super) fn is_generic<'db>(
    database: &'db dyn TypeDb,
    item: Item<'db>,
    signature: &types::Signature<'db>,
) -> bool {
    !signature.type_parameters.is_empty()
        || !super::instances::class_parameters(database, item).is_empty()
        || signature
            .parameters
            .iter()
            .any(|parameter| matches!(parameter.ty.kind(database), TypeKind::Callable { .. }))
}
