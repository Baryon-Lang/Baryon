//! What a definition declares, in types: a function's signature, a struct's fields, an enum's
//! variants, an alias's type, a constant's type and value. The type checker computes these from
//! annotations; every stage after it reads them through [`TypeDb::declaration`].

use crate::{Item, Type};
use diagnostics::Diagnostic;

/// The stage queries that need types, on top of [`database::Db`]'s.
///
/// Each interface crate adds the queries for the results it defines; this one adds the type
/// checker's. A stage that needs a declaration calls this method, never the checker's crate,
/// and the application wires it to the checker (decision 0016).
#[salsa::db]
pub trait TypeDb: database::Db {
    /// What `item` declares, and the errors found reading its annotations. Cached until the
    /// file, its names, or a declaration it reads changes.
    fn declaration<'db>(&'db self, item: Item<'db>) -> &'db Checked<'db>;
}

/// A declaration and the errors found computing it.
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub struct Checked<'db> {
    /// What the item declares. Parts that had errors are [`TypeKind::Unknown`](crate::TypeKind).
    pub declaration: Declaration<'db>,
    /// One error per annotation or constant that could not be understood, in source order.
    pub diagnostics: Vec<Diagnostic>,
}

/// What one definition declares.
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub enum Declaration<'db> {
    /// A function or method.
    Function(Signature<'db>),
    /// A struct or error type: its fields, in declaration order, and its methods.
    Struct {
        /// The fields, in declaration order, which is also their order in memory.
        fields: Vec<Field<'db>>,
        /// Whether the struct is declared `Copy`.
        copy: bool,
        /// Whether it is an error type, a class with the base `Exception`.
        error: bool,
        /// The methods, in source order.
        methods: Vec<Item<'db>>,
    },
    /// An enum: the names of its variants, in order.
    Enum {
        /// The variant names, in declaration order.
        variants: Vec<String>,
    },
    /// A type alias, `Vec3 = Fixed[f64, (3,)]`: the type it names.
    Alias(Type<'db>),
    /// A constant, `N: Final = 64`: its type and, when the compiler could compute it, its value.
    Constant {
        /// The constant's type.
        ty: Type<'db>,
        /// Its value, if it is a number the compiler computed.
        value: Option<Value>,
    },
}

/// A function's signature: everything a caller sees.
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub struct Signature<'db> {
    /// The type parameters, `T` in `def f[T]`, as [`TypeKind::Parameter`](crate::TypeKind)
    /// types, in order.
    pub type_parameters: Vec<Type<'db>>,
    /// The parameters, in order, `self` first for a method.
    pub parameters: Vec<Parameter<'db>>,
    /// The return type; `None` when the function returns nothing.
    pub returns: Type<'db>,
}

/// One parameter of a function.
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub struct Parameter<'db> {
    /// The parameter's name.
    pub name: String,
    /// Its type, without the mode: `Array[f64, 1]` for `InOut[Array[f64, 1]]`.
    pub ty: Type<'db>,
    /// How the argument is passed.
    pub mode: Mode,
    /// Whether the parameter has a default value, `dt: f64 = 0.01`, so a call may leave it out.
    pub default: bool,
}

/// How an argument is passed (decision 0006).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    /// The default: a read-only borrow.
    Borrow,
    /// `InOut[T]`: a borrow the function may change.
    InOut,
    /// `Owned[T]`: the argument moves into the function.
    Owned,
}

/// One field of a struct.
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub struct Field<'db> {
    /// The field's name.
    pub name: String,
    /// Its type.
    pub ty: Type<'db>,
}

/// A number the compiler computed: the value of a `Final` constant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Value {
    /// An integer.
    Integer(i64),
    /// A float, stored as its IEEE 754 bits so values compare exactly; read it with
    /// `f64::from_bits`.
    Float(u64),
}
