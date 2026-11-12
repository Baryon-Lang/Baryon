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
//! - [`union()`]: the one canonical form of a union type such as `Periodic | Dirichlet`
//!   (decision 0032).
//! - [`Column`]: one column of a `DataFrame` type, with how a frame
//!   [is stored](Type::frame_storage) (decision 0045).
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
mod frame;
mod interned;
mod layout;
mod primitive;
mod shape;
mod union;

pub use body::{Call, Callee, Key, TypedBody};
pub use declaration::{Checked, Declaration, Field, Mode, Parameter, Signature, TypeDb, Value};
pub use display::Displayed;
pub use format::{Align, FormatClass, FormatError, FormatSpec, Presentation, Sign};
pub use interned::{Item, Type};
pub use layout::{Declared, Layout, LayoutError, Record, layout};
pub use primitive::{Complex, Float, Integer, Primitive};
pub use shape::Shape;
pub use union::union;

use database::File;
use syntax::NodeId;

/// A type of the standard library that the compiler knows by name (decision 0028): one whose
/// value is runtime state a Baryon class cannot describe yet. Each one has a fixed layout; the
/// checker types its operations, and lowering and the backend build them as built-in calls rather
/// than Baryon code. A standard type that is plain data, such as `OSError`, is a class in the
/// library's own Baryon source instead (decision 0038).
///
/// ```
/// assert_eq!(types::StandardType::Philox.name(), "Philox");
/// assert!(types::StandardType::File.owns_resources());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub enum StandardType {
    /// `random.Philox`: a Philox4x64-10 counter-based random number generator whose numbers match
    /// `numpy.random.Philox` bit for bit. 88 bytes: the key (2 words), the counter (4 words), the
    /// current block's outputs (4 words) and how many of them are used (1 word). It moves rather
    /// than copies, so a copy can never repeat its original's numbers by accident.
    Philox,
    /// A text file `open` returned (decision 0036, part 3): one word, a pointer to the runtime's
    /// open file, which holds the file and its 64 KiB buffer. It moves rather than copies, and
    /// a drop that is not a `close()` or the end of a `with` block writes out the buffer and
    /// closes the file, panicking if that last write fails.
    File,
}

impl StandardType {
    /// The name a program writes the type by.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Philox => "Philox",
            Self::File => "File",
        }
    }

    /// The module of the standard library that defines it.
    ///
    /// ```
    /// assert_eq!(types::StandardType::Philox.module(), "random");
    /// ```
    #[must_use]
    pub fn module(self) -> &'static str {
        match self {
            Self::Philox => "random",
            Self::File => "builtins",
        }
    }

    /// Whether a value of the type holds something a drop must release: `File` holds an open
    /// file, while `Philox` is only numbers.
    #[must_use]
    pub fn owns_resources(self) -> bool {
        match self {
            Self::Philox => false,
            Self::File => true,
        }
    }
}

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
    /// `dict[K, V]`: a hash map from keys of type `K` to values of type `V`, in insertion order,
    /// stored on the heap (decision 0035). The checker allows only keys that compare exactly:
    /// integers, `bool`, `str`, enums without data and tuples of them.
    Dict {
        /// The key type.
        key: Type<'db>,
        /// The value type.
        value: Type<'db>,
    },
    /// `T | None`: a `T` or `None`.
    Optional(Type<'db>),
    /// `A | B | C`: a value of exactly one of the member types, such as
    /// `Periodic | Dirichlet | Neumann` (decision 0032). Built only by [`union()`], which keeps
    /// the members in one canonical order: at least two of them besides `None`, none of them a
    /// union or a `T | None`, no repeats, and `None` last when it is a member. A member's
    /// position in this list is its tag.
    Union(Vec<Type<'db>>),
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
    /// `DataFrame[S]`: a table of named columns, each a rank-1 array, all of one length
    /// (decision 0045). Lowering stores it as its [`columns`](Type::frame_storage), so the
    /// mid-level IR and the stages after it see only a tuple of arrays.
    DataFrame {
        /// The columns, in order. Two frames with the same columns are the
        /// [same table](Type::same_table), whatever struct names their rows.
        columns: Vec<Column<'db>>,
        /// The struct `S` that names the schema, whose fields are the columns, when the frame
        /// was declared with one; `None` for a schema the checker worked out, such as the
        /// result of choosing columns. A row read from the frame is a value of this struct.
        row: Option<Type<'db>>,
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
    /// One module-level function's name read as a value, such as `lorenz` in
    /// `rk4_step(lorenz, t, state, h)`: a type of its own for each function, which takes no bytes
    /// because the function is known when compiling. It matches a [`Callable`](Self::Callable)
    /// with the function's signature. A `Callable` parameter is compiled once per function
    /// passed to it, so the call is direct and can be inlined (decision 0026).
    Function {
        /// The function.
        item: Item<'db>,
        /// Its signature as a [`Callable`](Self::Callable), which diagnostics show in place of
        /// a type the program cannot spell.
        callable: Type<'db>,
    },
    /// A type of the standard library that the compiler builds in until the library can define
    /// it in Baryon (decision 0028), such as `random.Philox`.
    Standard(StandardType),
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

/// One column of a [`DataFrame`](TypeKind::DataFrame): its name and the type of its elements.
#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub struct Column<'db> {
    /// The column's name, as the program writes it in `df["name"]` and `df.name`.
    pub name: String,
    /// The type of one element, such as `f64`: the column is an `Array` of it of rank 1.
    pub element: Type<'db>,
}

impl<'db> Column<'db> {
    /// The column with its element type replaced by `change` of it and its name kept, as
    /// putting types in place of type parameters does.
    ///
    /// # Example
    ///
    /// ```
    /// use types::{Column, Float, Primitive, Type, TypeKind};
    ///
    /// # #[salsa::db]
    /// # #[derive(Clone, Default)]
    /// # struct Database { storage: salsa::Storage<Self> }
    /// # #[salsa::db]
    /// # impl salsa::Database for Database {}
    /// let database = Database::default();
    /// let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    /// let unknown = Type::new(&database, TypeKind::Unknown);
    /// let column = Column { name: "price".to_owned(), element: unknown };
    /// assert_eq!(column.map(|_| f64), Column { name: "price".to_owned(), element: f64 });
    /// ```
    #[must_use]
    pub fn map(&self, change: impl FnOnce(Type<'db>) -> Type<'db>) -> Self {
        Self {
            name: self.name.clone(),
            element: change(self.element),
        }
    }
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
