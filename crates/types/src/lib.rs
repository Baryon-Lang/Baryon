//! Types: how the compiler represents the type of every value, and how big each one is.
//!
//! This interface crate is the vocabulary that the type checker produces and the reference
//! checker, the mid-level IR and the backends read. It holds:
//!
//! - [`Type`]: one type, *interned*: the database stores each distinct type once and hands out
//!   a small id, so comparing two types is comparing two integers and a type costs eight bytes
//!   wherever it is stored. [`TypeKind`] is what the id stands for.
//! - The [primitives](Primitive): `bool` and the fixed-width numbers, `complex64` and
//!   `complex128` among them, with Python's `int`, `float` and `complex` as other names for
//!   `i64`, `f64` and `complex128`.
//! - [`Shape`]: the compile-time dimension lengths of a `Fixed` array.
//! - [`Layout`] and [`layout()`]: the size and alignment of a value of a type in memory, and
//!   the field offsets of a record.
//! - [`FormatSpec`]: a parsed f-string format specification, such as `.6f`, and the check of
//!   it against the kind of value it formats.
//! - [`Item`], [`Declaration`] and the [`TypeDb`] trait: what each definition declares in
//!   types, which the type checker computes and later stages read.
//!
//! The rules come from the language reference ("Types" and "Arrays") and decisions 0007
//! (`Array` and `Fixed`), 0008 (row-major) and 0014 (number mixing).
//!
//! # Example
//!
//! ```
//! use types::{Float, Primitive, Shape, Type, TypeKind};
//!
//! # #[salsa::db]
//! # #[derive(Clone, Default)]
//! # struct Database { storage: salsa::Storage<Self> }
//! # #[salsa::db]
//! # impl salsa::Database for Database {}
//! let database = Database::default();
//! let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
//! let vec3 = Type::new(
//!     &database,
//!     TypeKind::Fixed { element: f64, shape: Shape::new(vec![3]).unwrap() },
//! );
//! let positions = Type::new(&database, TypeKind::Array { element: vec3, rank: 1 });
//!
//! // Interned: building the same type again gives the same id.
//! let again = Type::new(&database, TypeKind::Array { element: vec3, rank: 1 });
//! assert_eq!(positions, again);
//!
//! let layout = types::layout(&database, vec3, &|_| unreachable!()).unwrap();
//! assert_eq!((layout.size(), layout.align()), (24, 8));
//! ```

mod body;
mod declaration;
mod display;
mod format;
mod interned;
mod layout;
mod primitive;
mod shape;

pub use body::{Call, Callee, Key, TypedBody};
pub use declaration::{Checked, Declaration, Field, Mode, Parameter, Signature, TypeDb, Value};
pub use display::Displayed;
pub use format::{Align, FormatClass, FormatError, FormatSpec, Presentation, Sign};
pub use interned::{Item, Type};
pub use layout::{Declared, Layout, LayoutError, Record, layout};
pub use primitive::{Complex, Float, Integer, Primitive};
pub use shape::Shape;

use database::File;
use syntax::NodeId;

/// What a [`Type`] is.
#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub enum TypeKind<'db> {
    /// `bool` or a number type.
    Primitive(Primitive),
    /// `str`: immutable UTF-8 text.
    Str,
    /// The type of `None`, and what a function that returns nothing returns.
    None,
    /// `tuple[A, B]`: a fixed number of values, each of its own type.
    Tuple(Vec<Type<'db>>),
    /// `list[T]`: a growable sequence of one type, stored on the heap.
    List(Type<'db>),
    /// `T | None`: a `T` or `None`.
    Optional(Type<'db>),
    /// `Array[T, rank]`: an array whose shape is known at run time, on the heap (decision 0007).
    Array {
        /// The element type, always a copy type.
        element: Type<'db>,
        /// The number of dimensions, at least 1.
        rank: u32,
    },
    /// `Fixed[T, shape]`: an array whose shape is part of its type, stored inline.
    Fixed {
        /// The element type, always a copy type.
        element: Type<'db>,
        /// The dimension lengths.
        shape: Shape,
    },
    /// A class with the base `Struct` or `Exception`: named fields stored together.
    Struct(Nominal<'db>),
    /// A class with the base `Enum`: one of a fixed set of named constants.
    Enum(Nominal<'db>),
    /// `Callable[[A, B], R]`: a function passed as an argument.
    Callable {
        /// The parameter types, in order.
        parameters: Vec<Type<'db>>,
        /// The return type.
        returns: Type<'db>,
    },
    /// `Raises[T, E]`: what a function that can fail returns, a `T` or one of the errors.
    Fallible {
        /// The type returned on success.
        value: Type<'db>,
        /// The error types it can raise, at least one.
        errors: Vec<Type<'db>>,
    },
    /// A type parameter, `T` in `def f[T](x: T)`, inside the definition that declares it.
    Parameter {
        /// The file of the declaring definition.
        file: File,
        /// The parameter's `Name` node in that file.
        name: NodeId,
    },
    /// A type that could not be worked out because of an error already reported. Stages accept
    /// it anywhere without a further error, so one mistake gives one message.
    Unknown,
}

/// A type declared by a class: which class, and its type arguments when it is generic.
#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub struct Nominal<'db> {
    /// The file that declares the class.
    pub file: File,
    /// The class's `Name` node in that file.
    pub name: NodeId,
    /// The type arguments, in the order of the class's type parameters; empty when it has
    /// none.
    pub arguments: Vec<Type<'db>>,
}

#[cfg(test)]
mod tests;
