//! Lowering: turns each type-checked function body into a mid-level IR body (roadmap 1.9.3,
//! decision 0018).
//!
//! Where it sits: after the type checker and before the passes. [`lower`] reads one function's
//! syntax tree, its names from the resolver, its signature from [`TypeDb::declaration`] and the
//! type of every expression from [`TypeDb::body`], and builds a [`Body`] in
//! [`Phase::Built`](mir::Phase): drops still conditional, whole-array expressions still whole,
//! type parameters still open. Drop elaboration, monomorphization and array expansion take it
//! from there.
//!
//! What it makes explicit:
//!
//! - **Locals.** Local 0 is the return value, then one local per parameter, then one per
//!   variable the function names, in source order, then the temporaries it needs.
//! - **Control flow.** `if`, `while`, `for` over `range`, `and`, `or`, chained comparisons and
//!   conditional expressions become basic blocks joined by branches.
//! - **Moves and borrows.** A read of a copy type copies; a read of any other type in a place
//!   that takes the value moves it. Each call argument is passed in the mode its parameter
//!   declares (decision 0006).
//! - **Drops.** Every exit from the function drops each variable and `Owned` parameter that
//!   owns memory, and each statement drops the owning temporaries it made. In a built body a
//!   drop of a value that was moved out does nothing; drop elaboration makes that exact.
//! - **Checks.** Integer `//` and `%` assert a non-zero divisor; each array index asserts it is
//!   in bounds, except under `a.unchecked` and for a constant index into a `Fixed` array, which
//!   the checker already proved; `assert` becomes an [`Assert`](mir::TerminatorKind::Assert).
//! - **Generic bodies.** A generic function lowers once, with its type parameters left open:
//!   `+`, `-` and `*` on a `T: Number` and its number literals become the instance's arithmetic
//!   and constants once monomorphization substitutes `T`.
//! - **Errors.** A call that can fail gets an error edge (decision 0018). The edge, and each
//!   `raise`, goes to the innermost `except` that names the error, or else drops what the
//!   function owns and [`Raise`](mir::TerminatorKind::Raise)s it to the caller. A bare `raise`
//!   raises the caught error again.
//! - **`match`.** Each case tests its pattern part by part, branching to the next case at the
//!   first part that does not match, then assigns its captures and tests its guard.
//! - **Arrays.** `a[i, j]` is a place with one index local per dimension, and a negative literal
//!   counts back from the length. Whole-array arithmetic stays one
//!   [`Elementwise`](mir::Rvalue::Elementwise) expression for array expansion to loop over.
//!   `Array.zeros`, `ones` and `fill` allocate with [`NewArray`](mir::Rvalue::NewArray) and fill
//!   the elements in a loop nest; a `Fixed` array is an [`Aggregate`](mir::Rvalue::Aggregate) of
//!   its elements in row-major order.
//! - **Conversions.** Where number mixing converts an operand (decision 0014), a
//!   [`Convert`](mir::Rvalue::Convert) does it, and `/` on integers converts both sides to `f64`.
//!
//! Integer `+`, `-` and `*` lower to wrapping [`Binary`](mir::Rvalue::Binary) operations until
//! decision D.11 settles overflow; the IR's [`CheckedBinary`](mir::Rvalue::CheckedBinary) is
//! ready for whichever answer it gives.
//!
//! # What it does not lower yet
//!
//! Lowering grows in slices. A construct outside them gives an error that names it and roadmap
//! 1.9.3, and no body: `/`, `//` and `%` on a type parameter, slices, indexing lists and strings,
//! `prange`, `match` captures of values that are not copy types or inside `|`, `raise ... from`,
//! an `except` that names its error through a module, calls that leave out a defaulted argument,
//! the built-in functions and methods other than `print`, `len` of an array and the array
//! constructors, calls of a `Callable` value, `**`, `@`, `in`, unpacking assignment, `with` other
//! than `with unsafe:`, and comprehensions.
//!
//! # Example
//!
//! Lowering `def double(x: f64) -> f64: return x * 2.0`. A real program gets its tree, names,
//! declarations and types from the parser, resolver and checker that the driver wires in; this
//! example builds them by hand.
//!
//! ```
//! use database::{Db, Definition, File, Package, ParsedFile, Resolution};
//! use source::{FileId, SourceFile};
//! use syntax::{Operator, Role, SyntaxKind, TreeBuilder};
//! use types::{
//!     Checked, Declaration, Float, Item, Key, Mode, Parameter, Primitive, Signature, Type,
//!     TypeDb, TypeKind, TypedBody,
//! };
//!
//! #[salsa::db]
//! #[derive(Clone, Default)]
//! struct Database {
//!     storage: salsa::Storage<Self>,
//! }
//!
//! #[salsa::db]
//! impl salsa::Database for Database {}
//!
//! const TEXT: &str = "def double(x: f64) -> f64:\n    return x * 2.0\n";
//!
//! #[salsa::db]
//! impl Db for Database {
//!     fn parse(&self, file: File) -> &ParsedFile {
//!         parse(self, file)
//!     }
//!     fn resolve(&self, package: Package, file: File) -> &Resolution {
//!         resolve(self, package, file)
//!     }
//! }
//!
//! #[salsa::db]
//! impl TypeDb for Database {
//!     fn declaration<'db>(&'db self, item: Item<'db>) -> &'db Checked<'db> {
//!         declaration(self, item)
//!     }
//!     fn body<'db>(&'db self, item: Item<'db>) -> &'db TypedBody<'db> {
//!         body(self, item)
//!     }
//! }
//!
//! /// The tree of `TEXT`, node by node in preorder.
//! #[salsa::tracked(returns(ref), no_eq)]
//! fn parse(db: &dyn Db, file: File) -> ParsedFile {
//!     let mut builder = TreeBuilder::new(file.id(db));
//!     builder.open(SyntaxKind::Module, Role::Root, 0, 46);
//!     builder.open(SyntaxKind::FunctionDefinition, Role::Body, 0, 45); // 1
//!     builder.leaf(SyntaxKind::Name, Role::Name, 4, 10); // 2: double
//!     builder.open(SyntaxKind::Parameters, Role::Parameters, 10, 18); // 3
//!     builder.open(SyntaxKind::Parameter, Role::Item, 11, 17); // 4
//!     builder.leaf(SyntaxKind::Name, Role::Name, 11, 12); // 5: x
//!     builder.leaf(SyntaxKind::Name, Role::Annotation, 14, 17); // 6: f64
//!     builder.close();
//!     builder.close();
//!     builder.leaf(SyntaxKind::Name, Role::Returns, 22, 25); // 7: f64
//!     builder.open(SyntaxKind::Return, Role::Body, 31, 45); // 8
//!     builder.open(SyntaxKind::BinaryOperation, Role::Value, 38, 45); // 9
//!     builder.set_operator(Operator::Multiply);
//!     builder.leaf(SyntaxKind::Name, Role::Left, 38, 39); // 10: x
//!     builder.leaf(SyntaxKind::Float, Role::Right, 42, 45); // 11: 2.0
//!     builder.close();
//!     builder.close();
//!     builder.close();
//!     builder.close();
//!     ParsedFile::new(builder.finish(), Vec::new())
//! }
//!
//! /// The id of node `index` of `file`'s tree.
//! fn node(db: &dyn Db, file: File, index: usize) -> syntax::NodeId {
//!     let tree = db.parse(file).tree();
//!     let mut nodes = vec![tree.root()];
//!     while let Some(next) = nodes.pop() {
//!         if next.index() == index {
//!             return next;
//!         }
//!         nodes.extend(tree.children(next));
//!     }
//!     unreachable!("the tree has {index} nodes")
//! }
//!
//! /// Both uses of `x` refer to the parameter.
//! #[salsa::tracked(returns(ref))]
//! fn resolve(db: &dyn Db, _: Package, file: File) -> Resolution {
//!     let x = node(db, file, 5);
//!     let references = vec![(x, Definition::Local(x)), (node(db, file, 10), Definition::Local(x))];
//!     Resolution::new(references, Vec::new())
//! }
//!
//! fn f64_type(db: &dyn salsa::Database) -> Type<'_> {
//!     Type::new(db, TypeKind::Primitive(Primitive::Float(Float::F64)))
//! }
//!
//! #[salsa::tracked(returns(ref))]
//! fn declaration<'db>(db: &'db dyn TypeDb, _: Item<'db>) -> Checked<'db> {
//!     let x = Parameter { name: "x".into(), ty: f64_type(db), mode: Mode::Borrow, default: false };
//!     let signature =
//!         Signature { type_parameters: Vec::new(), parameters: vec![x], returns: f64_type(db) };
//!     Checked { declaration: Declaration::Function(signature), diagnostics: Vec::new() }
//! }
//!
//! /// Every expression is an `f64`.
//! #[salsa::tracked(returns(ref))]
//! fn body<'db>(db: &'db dyn TypeDb, item: Item<'db>) -> TypedBody<'db> {
//!     let file = item.file(db);
//!     let mut typed = TypedBody::default();
//!     typed.locals.insert(Key(node(db, file, 5)), f64_type(db));
//!     for expression in [9, 10, 11] {
//!         typed.expressions.insert(Key(node(db, file, expression)), f64_type(db));
//!     }
//!     typed
//! }
//!
//! let database = Database::default();
//! let source = SourceFile::new("double.byn", TEXT)?;
//! let file = File::new(&database, FileId::from_index(0), source);
//! let package = Package::new(&database, vec![file]);
//! let double = Item::new(&database, package, file, node(&database, file, 2));
//!
//! let body = lowering::lower(&database, double).expect("`double` is in the first slice");
//! assert!(body.validate(&database).is_empty());
//! assert_eq!(
//!     body.display(&database).to_string(),
//!     "\
//! fn(_1: f64) -> f64 {
//!     let _0: f64; // return
//!     let _1: f64; // parameter x
//!
//!     bb0: {
//!         _0 = Multiply(copy _1, 2.0_f64);
//!         return;
//!     }
//! }
//! "
//! );
//! # Ok::<(), source::SourceError>(())
//! ```

mod arrays;
mod builder;
mod calls;
mod constants;
mod creation;
mod errors;
mod expressions;
mod facts;
mod operators;
mod patterns;
mod statements;
mod text;
#[cfg(test)]
mod tests;

use builder::Builder;
use diagnostics::Diagnostic;
use mir::Body;
use syntax::{NodeId, Role, SyntaxKind, SyntaxTree};
use types::{Declaration, Item, TypeDb};

/// The IR body of the function or method `item`, in [`Phase::Built`](mir::Phase).
///
/// Call it only on a function the checker accepted: lowering trusts the types and names it
/// reads, and does not check them again.
///
/// Cost: one walk over the function's syntax, plus the cached queries it reads.
///
/// # Errors
///
/// Fails with one error per construct the first slice does not lower yet (see the crate
/// documentation), in source order. Fails with no errors when `item` is not a function, or its
/// declaration or body has errors, which the checker already reports.
pub fn lower<'db>(db: &'db dyn TypeDb, item: Item<'db>) -> Result<Body<'db>, Vec<Diagnostic>> {
    let Declaration::Function(signature) = &db.declaration(item).declaration else {
        return Err(Vec::new());
    };
    let typed = db.body(item);
    if !db.declaration(item).diagnostics.is_empty() || !typed.diagnostics.is_empty() {
        return Err(Vec::new());
    }
    let tree = db.parse(item.file(db)).tree();
    let Some((function, class)) = definition(tree, item.name(db)) else {
        return Err(Vec::new());
    };
    let mut builder = Builder::new(db, item, signature, typed, function, class);
    builder.function();
    let body = builder.finish()?;
    debug_assert_eq!(body.validate(db), [], "lowering built an invalid body");
    Ok(body)
}

/// The `FunctionDefinition` whose `Name` node is `name`, among the top-level statements of
/// `tree` and the bodies of its classes, with the class for a method.
fn definition(tree: &SyntaxTree, name: NodeId) -> Option<(NodeId, Option<NodeId>)> {
    let named = |statement: NodeId| {
        tree.kind(statement) == SyntaxKind::FunctionDefinition
            && tree.child(statement, Role::Name) == Some(name)
    };
    tree.children_with(tree.root(), Role::Body)
        .find_map(|statement| {
            if named(statement) {
                return Some((statement, None));
            }
            (tree.kind(statement) == SyntaxKind::ClassDefinition).then(|| {
                tree.children_with(statement, Role::Body)
                    .find(|&method| named(method))
                    .map(|method| (method, Some(statement)))
            })?
        })
}
