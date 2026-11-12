//! Function bodies: the type of every expression, and the errors in how values are used.
//!
//! Inference is local to one function (language reference, "Types", "Type inference"): a
//! function's signature is all its callers see, and inside the body each variable takes the
//! type of the first value assigned to it, or its annotation, and keeps that type for its whole
//! scope. One walk over the body in source order types every statement.
//!
//! What it checks: number literals, number mixing, operators and comparisons, `bool`
//! conditions, calls to functions, constructors and methods against their signatures, field
//! access, assignments and returns (roadmap 1.7.3); arrays: creating them, indexing,
//! slicing, methods, whole-array arithmetic and `@` (1.7.4, the `arrays` module); and
//! generics: type arguments inferred at calls, bounds checked, and inside a generic body only
//! the operations a bound allows (1.7.5, the `generics` module); and `match`: patterns
//! against the subject, and cases that cover every value (1.7.6, the `patterns` and
//! `exhaustive` modules); and errors: each error a call or `raise` can fail with is caught by a
//! `try` or declared in the function's `Raises` (1.7.7, the `errors` module); and optional
//! values: a `T | None` is used as a `T` only after a `None` check narrows it (1.7.8, the
//! `flow` module); and definite assignment: a variable is read only where every path has
//! assigned it, and a function that returns a value cannot reach its end (1.7.9, the
//! `assignment` module); and moves: a value that is not a copy type is read only until it
//! moves, and only a variable the function owns can move (1.8.2, the `moves` module); and
//! exclusivity: no argument refers to a value its call changes or moves, and a read-only
//! parameter is never changed (1.8.3, the `exclusive` module); and loops over collections:
//! the body changes neither the collection nor the element it borrows (1.8.4, the `loops`
//! module); and parallel loops: a `prange` body writes shared arrays only at the loop
//! variable's index and shared scalars only as reductions, never ends early, and only adds
//! into the arrays it lists in `accumulate=`, and a `simd(range(...))` body follows the same
//! rules (1.8.5, 1.8.1 and 1.9.7, the `parallel`, `shared` and `accumulate` modules).

mod accumulate;
mod array_files;
mod arrays;
mod assignment;
mod axes;
mod binding;
mod builtins;
mod calls;
mod class_patterns;
mod complex;
mod dicts;
mod elementwise;
mod errors;
mod exact_sums;
mod exclusive;
mod exhaustive;
mod expressions;
mod files;
mod fits;
mod fits_header;
mod flow;
mod formatted;
mod frame_groups;
mod frame_joins;
mod frame_rows;
mod frame_schema;
mod frame_sort;
mod frames;
mod generics;
mod library;
mod loops;
mod math;
mod members;
mod methods;
mod moves;
mod netcdf;
mod netcdf_writing;
mod npy;
mod numbers;
mod operators;
mod parallel;
mod paths;
mod patterns;
mod process;
mod random;
mod reductions;
pub(crate) mod relations;
mod selection;
mod shared;
mod statistics;
mod streams;
mod subprocess;
mod text;
mod text_tables;
mod union_methods;

use crate::lower::Lowerer;
use crate::outline::Entry;
use database::{Definition, File, Package};
pub(crate) use dicts::key_error as dict_key_error;
use std::collections::{BTreeMap, BTreeSet};
use syntax::{NodeId, Role, SyntaxKind, SyntaxTree};
use types::{Declaration, Item, Primitive, Signature, Type, TypeDb, TypeKind, TypedBody};

/// Checks the body of the function or method `entry` in `file`, returning what the stages
/// after the checker read and the errors found.
pub(crate) fn check<'db>(
    db: &'db dyn TypeDb,
    package: Package,
    file: File,
    entry: &Entry,
) -> TypedBody<'db> {
    let item = Item::new(db, package, file, entry.name);
    let Declaration::Function(signature) = &db.declaration(item).declaration else {
        return TypedBody::default();
    };
    let lowerer = Lowerer::new(db, package, file, entry.class, entry.statement);
    let mut body = Body {
        db,
        tree: lowerer.tree,
        text: lowerer.text,
        lowerer,
        signature: signature.clone(),
        function: entry.statement,
        locals: BTreeMap::new(),
        state: flow::State::default(),
        handled: Vec::new(),
        caught: Vec::new(),
        bound: BTreeSet::new(),
        modes: BTreeMap::new(),
        loops: Vec::new(),
        loop_ends: BTreeSet::new(),
        parallel: Vec::new(),
        accumulate: Vec::new(),
        typed: TypedBody::default(),
    };
    let mut bound = BTreeSet::new();
    body.statement_bindings(entry.statement, &mut bound);
    body.bound = bound;
    body.parameters(entry.statement);
    if body.block(entry.statement, Role::Body) {
        body.missing_return(entry.name);
    }
    let mut typed = body.typed;
    typed.locals = body
        .locals
        .into_iter()
        .map(|(binding, ty)| (types::Key(binding), ty))
        .collect();
    typed.diagnostics = body.lowerer.diagnostics;
    typed
}

/// The state of checking one function body.
struct Body<'db> {
    /// The database.
    db: &'db dyn TypeDb,
    /// The file's tree.
    tree: &'db SyntaxTree,
    /// The file's text.
    text: &'db str,
    /// Lowers local annotations and computes literals; holds the errors found.
    lowerer: Lowerer<'db>,
    /// The function's signature.
    signature: Signature<'db>,
    /// The function's definition statement.
    function: NodeId,
    /// The type of each local variable, by the `Name` node of its first binding, which the
    /// resolver gives every use of the variable.
    locals: BTreeMap<NodeId, Type<'db>>,
    /// What is known here about variables: which are assigned, and which `T | None` ones hold
    /// a `T` (the `flow` module).
    state: flow::State<'db>,
    /// The error types each enclosing `try` handles, innermost last.
    handled: Vec<Vec<Type<'db>>>,
    /// The error types each enclosing `except` block caught, innermost last, which a bare
    /// `raise` raises again.
    caught: Vec<Vec<Type<'db>>>,
    /// The variables statements bind, which are checked for definite assignment (the
    /// `assignment` module).
    bound: BTreeSet<NodeId>,
    /// How each parameter, by its `Name` node, receives its argument (the `moves` module).
    modes: BTreeMap<NodeId, types::Mode>,
    /// The enclosing `for` loops over collections, innermost last (the `loops` module).
    loops: Vec<loops::Loop>,
    /// The loop targets that ended a borrow: a loop variable that borrows an element is
    /// recorded as moved at its target when the loop ends, and a read of it before it is
    /// assigned again is reported as a read after the loop (the `loops` module).
    loop_ends: BTreeSet<NodeId>,
    /// The enclosing `prange` and `simd` loops, innermost last: at most one of each, the
    /// `simd` loop inside the `prange` one (the `parallel` module).
    parallel: Vec<parallel::Parallel>,
    /// The valid entries of the `accumulate=` argument of the `prange` call just checked,
    /// waiting for its loop to start (the `accumulate` module).
    accumulate: Vec<NodeId>,
    /// What the stages after the checker read about this body, filled as it is checked.
    typed: TypedBody<'db>,
}

impl<'db> Body<'db> {
    /// Binds each parameter to its declared type, and checks each default value against it.
    fn parameters(&mut self, function: NodeId) {
        let nodes: Vec<NodeId> = self
            .tree
            .child(function, Role::Parameters)
            .into_iter()
            .flat_map(|parameters| self.tree.children(parameters))
            .filter(|&parameter| self.tree.kind(parameter) == SyntaxKind::Parameter)
            .filter(|&parameter| self.tree.child(parameter, Role::Name).is_some())
            .collect();
        let parameters: Vec<(Type<'db>, types::Mode)> = self
            .signature
            .parameters
            .iter()
            .map(|parameter| (parameter.ty, parameter.mode))
            .collect();
        for (node, (ty, mode)) in nodes.into_iter().zip(parameters) {
            if let Some(name) = self.tree.child(node, Role::Name) {
                self.locals.insert(name, ty);
                self.modes.insert(name, mode);
                self.state.assigned.insert(name);
            }
            if let Some(default) = self.tree.child(node, Role::Default) {
                let found = self.expression(default, Some(ty));
                self.expect(default, ty, found, "the parameter's type is");
            }
        }
    }

    /// Checks every statement of `parent` with the role `role`, in order, and returns whether
    /// the block can fall through to what follows it. Statements after one that cannot are
    /// still checked.
    fn block(&mut self, parent: NodeId, role: Role) -> bool {
        let statements: Vec<NodeId> = self.tree.children_with(parent, role).collect();
        let mut falls = true;
        for statement in statements {
            falls &= self.statement(statement);
        }
        falls
    }

    /// Checks one statement, and returns whether it can fall through to the next: `return`,
    /// `raise`, `break` and `continue` cannot, nor can an `if` or `match` none of whose branches
    /// can.
    fn statement(&mut self, statement: NodeId) -> bool {
        let child = |role| self.tree.child(statement, role);
        match self.tree.kind(statement) {
            SyntaxKind::Raise => {
                self.raise(statement);
                return false;
            }
            SyntaxKind::Return => {
                self.return_statement(statement);
                return false;
            }
            SyntaxKind::Break | SyntaxKind::Continue => return false,
            SyntaxKind::ExpressionStatement => {
                if let Some(value) = child(Role::Value) {
                    self.expression(value, None);
                }
            }
            SyntaxKind::Assign => self.assignment(statement),
            SyntaxKind::AnnotatedAssign => self.annotated(statement),
            SyntaxKind::AugmentedAssign => self.augmented(statement),
            SyntaxKind::If => return self.if_statement(statement),
            SyntaxKind::While => return self.while_statement(statement),
            SyntaxKind::For => self.for_statement(statement),
            SyntaxKind::With => {
                let items: Vec<NodeId> = self.tree.children_with(statement, Role::Item).collect();
                for item in items {
                    match self.tree.child(item, Role::Target) {
                        // `with open(...) as f:`, the one `with` that binds (decision 0036).
                        Some(target) => {
                            if let Some(ty) = self.with_file(statement, item) {
                                let value = self.tree.child(item, Role::Value);
                                self.bind(target, ty, value);
                            }
                        }
                        None => {
                            if let Some(value) = self.tree.child(item, Role::Value) {
                                self.expression(value, None);
                            }
                        }
                    }
                }
                return self.block(statement, Role::Body);
            }
            SyntaxKind::Delete => self.delete(statement),
            SyntaxKind::Match => return self.match_statement(statement),
            SyntaxKind::Try => return self.try_statement(statement),
            SyntaxKind::Assert => {
                let condition = child(Role::Condition);
                self.condition(condition);
                if let Some(message) = child(Role::Message) {
                    self.expression(message, None);
                }
                if let Some(condition) = condition {
                    let (yes, _) = self.facts(condition);
                    self.assume(&yes);
                }
            }
            _ => {}
        }
        true
    }

    /// `return value`, checked against the function's return type. In a function that returns
    /// `Raises[T, E]`, a `return` gives the `T`.
    fn return_statement(&mut self, statement: NodeId) {
        let expected = match self.signature.returns.kind(self.db) {
            TypeKind::Fallible { value, .. } => *value,
            _ => self.signature.returns,
        };
        if let Some(value) = self.tree.child(statement, Role::Value) {
            let found = self.expression(value, Some(expected));
            self.consume(value, found);
            if self.tree.child(self.function, Role::Returns).is_none()
                && !relations::compatible(self.db, expected, found)
            {
                let message = format!(
                    "this function has no return type, so it cannot return `{}`",
                    self.show(found)
                );
                let note = format!("declare what it returns: `-> {}`", self.show(found));
                self.lowerer
                    .error(value, &message, "returns a value", Some(&note));
            } else {
                self.expect(value, expected, found, "the function returns");
            }
        } else {
            let nothing = Type::new(self.db, TypeKind::None);
            if !relations::compatible(self.db, expected, nothing) {
                let message = format!(
                    "this function returns `{}`, so `return` needs a value",
                    self.show(expected)
                );
                self.lowerer.error(statement, &message, "no value", None);
            }
        }
    }

    /// Checks that a condition is a `bool`. A missing condition, after a syntax error, is
    /// skipped.
    fn condition(&mut self, condition: Option<NodeId>) {
        let Some(condition) = condition else {
            return;
        };
        let boolean = Type::new(self.db, TypeKind::Primitive(Primitive::Bool));
        let found = self.expression(condition, Some(boolean));
        if !relations::compatible(self.db, boolean, found) {
            let message = format!("a condition must be a `bool`, found `{}`", self.show(found));
            self.lowerer.error(
                condition,
                &message,
                "not a `bool`",
                Some("compare explicitly, such as `n != 0` or `x is not None`"),
            );
        }
    }

    /// The type a target already holds, which gives a literal assigned to it its type: in
    /// `x: f32 = 0` then `x = 1`, the `1` is an `f32`.
    fn declared(&self, target: NodeId) -> Option<Type<'db>> {
        match self.lowerer.definition(target) {
            Some(Definition::Local(binding)) if self.tree.kind(target) == SyntaxKind::Name => {
                self.locals.get(binding).copied()
            }
            _ => None,
        }
    }

    /// Reports `found` where `expected` is needed, at `node`. `context` says where the expected
    /// type comes from, such as "the function returns".
    fn expect(&mut self, node: NodeId, expected: Type<'db>, found: Type<'db>, context: &str) {
        if relations::compatible(self.db, expected, found) || self.unchecked(node, expected, found)
        {
            return;
        }
        let message = format!(
            "expected `{}`, found `{}`",
            self.show(expected),
            self.show(found)
        );
        let label = format!("{context} `{}`", self.show(expected));
        self.lowerer.error(node, &message, &label, None);
    }

    /// The type a number literal on the other side of an operator should take: the other
    /// side's type when it is a number, its element type when it is an array, so `2.0 * a`
    /// with `a: Array[f32, 1]` multiplies by an `f32`, and what a type parameter's bound
    /// gives a literal (the `generics` module).
    fn number_context(&self, ty: Type<'db>) -> Option<Type<'db>> {
        match ty.kind(self.db) {
            TypeKind::Primitive(primitive) if numbers::is_number(*primitive) => Some(ty),
            TypeKind::Array { element, .. } | TypeKind::Fixed { element, .. } => {
                self.number_context(*element)
            }
            // `3 in counts` looks up a key, so the literal takes the key type.
            TypeKind::Dict { key, .. } => self.number_context(*key),
            _ => self.generic_literal(ty),
        }
    }

    /// Reports a `T | None` given where a `T` is expected, and returns whether it was one.
    fn unchecked(&mut self, node: NodeId, expected: Type<'db>, found: Type<'db>) -> bool {
        match found.kind(self.db) {
            TypeKind::Optional(value) if relations::compatible(self.db, expected, *value) => {
                self.maybe_none(node, found)
            }
            _ => false,
        }
    }

    /// A type as a program writes it.
    fn show(&self, ty: Type<'db>) -> String {
        ty.display(self.db).to_string()
    }
}
