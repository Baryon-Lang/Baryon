//! What the type checker learned about one function body, for the stages after it: the type
//! of every expression, what each call calls, and what each fallible call or `raise` can fail
//! with. Lowering to the mid-level IR (roadmap 1.9.3, decision 0018) reads it instead of
//! checking the body again.
//!
//! Every map is keyed by syntax node, in the file the function is defined in, wrapped in a
//! [`Key`] so the result can be cached in the database; the getters take a plain [`NodeId`].
//! Names are not repeated here: what a name refers to is the resolver's answer, through
//! [`database::Db::resolve`].

use crate::{Item, Type};
use diagnostics::Diagnostic;
use std::collections::BTreeMap;
use syntax::NodeId;

/// One function body, checked.
///
/// The type checker fills one per function; a later stage reads it by node:
///
/// ```
/// use source::SourceMap;
/// use syntax::{Role, SyntaxKind, TreeBuilder};
/// use types::{Key, TypedBody};
///
/// let mut sources = SourceMap::new();
/// let file = sources.add("main.npl", "pass")?;
/// let mut builder = TreeBuilder::new(file);
/// builder.leaf(SyntaxKind::Module, Role::Root, 0, 4);
/// let node = builder.finish().root();
///
/// let mut body = TypedBody::default();
/// body.patterns.insert(Key(node), 1);
/// assert_eq!(body.pattern(node), Some(1));
/// assert_eq!(body.expression(node), None);
/// assert!(body.errors(node).is_empty());
/// # Ok::<(), source::SourceError>(())
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, salsa::SalsaValue)]
pub struct TypedBody<'db> {
    /// The type of every expression node, after inference:
    ///
    /// - a number literal has the type its context gave it, so in `x * 2` with `x: f32` the
    ///   `2` is an `f32`;
    /// - each operand of an operator has its own type, so where mixing converts one, the two
    ///   operands' types differ from each other and from the operator node's;
    /// - a read of a `T | None` variable after a `None` check has the narrowed `T`, while
    ///   [`locals`](Self::locals) keeps `T | None`;
    /// - a call to a function that returns `Raises[T, E]` has the type `T`.
    ///
    /// An expression whose type could not be found, after an error, is
    /// [`TypeKind::Unknown`](crate::TypeKind::Unknown) or missing.
    pub expressions: BTreeMap<Key, Type<'db>>,
    /// The declared type of every local variable and parameter, keyed by the `Name` node of
    /// its first binding, the node the resolver gives every use.
    pub locals: BTreeMap<Key, Type<'db>>,
    /// What each `Call` node calls.
    pub calls: BTreeMap<Key, Call<'db>>,
    /// For each fallible call and each `raise`, the error types it can fail with, in the order
    /// the callee's `Raises` writes them. That position is the error's index in the
    /// mid-level IR. A bare `raise` lists the errors its `except` caught.
    pub errors: BTreeMap<Key, Vec<Type<'db>>>,
    /// For each `case` pattern that names an enum member, the member's position in its enum,
    /// counting from zero in declaration order.
    pub patterns: BTreeMap<Key, u32>,
    /// The errors and warnings found in the body, in source order.
    pub diagnostics: Vec<Diagnostic>,
}

impl<'db> TypedBody<'db> {
    /// The type of the expression `node`, if it was found.
    #[must_use]
    pub fn expression(&self, node: NodeId) -> Option<Type<'db>> {
        self.expressions.get(&Key(node)).copied()
    }

    /// The declared type of the local variable or parameter whose first binding is `binding`.
    #[must_use]
    pub fn local(&self, binding: NodeId) -> Option<Type<'db>> {
        self.locals.get(&Key(binding)).copied()
    }

    /// What the `Call` node `node` calls, if the checker could tell.
    #[must_use]
    pub fn call(&self, node: NodeId) -> Option<&Call<'db>> {
        self.calls.get(&Key(node))
    }

    /// The error types the call or `raise` at `node` can fail with, in `Raises` order; empty
    /// when it cannot fail.
    #[must_use]
    pub fn errors(&self, node: NodeId) -> &[Type<'db>] {
        self.errors.get(&Key(node)).map_or(&[], Vec::as_slice)
    }

    /// The position in its enum of the member the `case` pattern `node` names.
    #[must_use]
    pub fn pattern(&self, node: NodeId) -> Option<u32> {
        self.patterns.get(&Key(node)).copied()
    }
}

/// A syntax node as a map key in a [`TypedBody`]. It wraps [`NodeId`] because the database
/// caches only values whose parts it can check carry no borrowed data, and `NodeId` belongs to
/// the `syntax` crate, which does not depend on the database.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, salsa::SalsaValue)]
pub struct Key(pub NodeId);

/// One call, resolved.
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub struct Call<'db> {
    /// What is called.
    pub callee: Callee<'db>,
    /// The type arguments of a generic callee, in the order of its type parameters; empty
    /// for anything else.
    pub type_arguments: Vec<Type<'db>>,
    /// The argument expression for each parameter, in parameter order: keyword arguments in
    /// their parameter's place, and `None` where the parameter's default is used. A method's
    /// receiver comes first, for `self`. Modes come from the callee's signature. A
    /// [`Callee::Builtin`] or [`Callee::Conversion`] has no signature: its arguments are as
    /// written, the receiver first, then the positional arguments, then the keyword ones.
    pub arguments: Vec<Option<Key>>,
}

/// What a call calls.
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub enum Callee<'db> {
    /// A module-level function.
    Function(Item<'db>),
    /// A method of a struct; the receiver is the first argument.
    Method(Item<'db>),
    /// A struct's constructor, which takes its fields in declaration order.
    Constructor(Item<'db>),
    /// A conversion to a number type, `f64(x)`, or the construction of a `Fixed` vector from
    /// its elements, `Vec3(1.0, 0.0, 0.0)`: the type built.
    Conversion(Type<'db>),
    /// A function or method the language provides, by name: `len`, `print`, `range`, `Array`
    /// (from a list), `Array.zeros`, `Fixed.zeros` (on an alias such as `Vec3`), or a method
    /// of a built-in type written as the type's name and the method's: `list.append`,
    /// `Array.sum`, `Fixed.dot`, `str.copy`, or `struct.copy` for the `copy` every struct has.
    /// For a method the receiver is the first argument.
    Builtin(String),
    /// A call of a local variable or parameter holding a `Callable`.
    Value,
}
