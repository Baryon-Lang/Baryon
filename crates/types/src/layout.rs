//! Memory layout: how many bytes a value of each type takes, how it must be aligned, and where
//! each field of a record sits.
//!
//! The rules, which the backends and any zero-copy view of the data rely on:
//!
//! - **Primitives** take their width and align to it, except complex numbers, which are two
//!   floats and align like one.
//! - **Records** (structs, tuples) store their fields in declaration order, each at the next
//!   offset that suits its alignment, like a C struct. The compiler never reorders fields, so a
//!   struct's layout is the one its author wrote, and an array of structs can be shared with C,
//!   a `numpy` structured array or a file format byte for byte. Order fields from largest to
//!   smallest to avoid padding.
//! - **`Fixed[T, shape]`** is its elements back to back, row-major (decision 0008), with `T`'s
//!   alignment.
//! - **`Array[T, rank]`** is a header pointing to the elements on the heap: a data pointer,
//!   then the length of each dimension, then the stride of each dimension in elements, all
//!   64-bit. A view of an array has the same header with its own pointer and strides.
//! - **`list[T]`** is a pointer, a length and a capacity; **`str`** a pointer and a length in
//!   bytes.
//! - **`T | None`** is a `T` followed by a one-byte flag, as a record.
//! - **Enums** are the smallest unsigned integer that numbers their variants.
//! - **`Raises[T, E]`** is a one-byte tag followed by room for the largest of `T` and the errors.
//! - **`Callable`** and a function's own type take no bytes: the function passed is known when
//!   compiling, and the callee is compiled once per function (language reference, "Types",
//!   "Functions as arguments", and decision 0026).

use crate::{Nominal, Primitive, StandardType, Type, TypeKind};
use std::error::Error;
use std::fmt;

/// The size and alignment of a type, in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Layout {
    /// Bytes a value occupies, including trailing padding; always a multiple of `align`.
    size: u64,
    /// The address of a value is a multiple of this; always a power of two.
    align: u64,
}

impl Layout {
    /// The layout of a value that occupies no memory.
    pub const EMPTY: Self = Self { size: 0, align: 1 };

    /// A layout of `size` bytes aligned to `align`, or `None` unless `align` is a power of two
    /// and `size` a multiple of it.
    ///
    /// ```
    /// use types::Layout;
    ///
    /// assert!(Layout::new(16, 8).is_some());
    /// assert!(Layout::new(12, 8).is_none());
    /// ```
    #[must_use]
    pub fn new(size: u64, align: u64) -> Option<Self> {
        (align.is_power_of_two() && size.is_multiple_of(align)).then_some(Self { size, align })
    }

    /// Bytes a value occupies, including trailing padding.
    #[must_use]
    pub fn size(self) -> u64 {
        self.size
    }

    /// The alignment: a value's address is a multiple of this.
    #[must_use]
    pub fn align(self) -> u64 {
        self.align
    }

    /// `count` values back to back, or `None` if the size does not fit in a `u64`.
    #[must_use]
    pub fn repeat(self, count: u64) -> Option<Self> {
        Some(Self {
            size: self.size.checked_mul(count)?,
            align: self.align,
        })
    }

    /// Lays out fields in the order given, each at the next offset its alignment allows, and
    /// rounds the total up to the largest alignment. `None` if the size does not fit in a `u64`.
    ///
    /// ```
    /// use types::Layout;
    ///
    /// // A `u8` flag, then an `f64`: 7 bytes of padding between them.
    /// let byte = Layout::new(1, 1).unwrap();
    /// let double = Layout::new(8, 8).unwrap();
    /// let record = Layout::record([byte, double]).unwrap();
    /// assert_eq!(record.offsets(), [0, 8]);
    /// assert_eq!((record.layout().size(), record.layout().align()), (16, 8));
    /// ```
    pub fn record(fields: impl IntoIterator<Item = Layout>) -> Option<Record> {
        let mut offsets = Vec::new();
        let mut end = 0_u64;
        let mut align = 1;
        for field in fields {
            let offset = end.checked_next_multiple_of(field.align)?;
            offsets.push(offset);
            end = offset.checked_add(field.size)?;
            align = align.max(field.align);
        }
        Some(Record {
            layout: Self {
                size: end.checked_next_multiple_of(align)?,
                align,
            },
            offsets,
        })
    }
}

/// The layout of a record and the offset of each of its fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// The size and alignment of the whole record.
    layout: Layout,
    /// The byte offset of each field, in declaration order.
    offsets: Vec<u64>,
}

impl Record {
    /// The size and alignment of the whole record.
    #[must_use]
    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// The byte offset of each field from the start of the record, in declaration order.
    #[must_use]
    pub fn offsets(&self) -> &[u64] {
        &self.offsets
    }
}

/// What a class declares, as far as layout needs it. The type checker knows this; [`layout()`]
/// asks for it through a callback, so this crate needs no type checker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Declared<'db> {
    /// A struct or error type: its field types in declaration order, with the class's type
    /// arguments already substituted.
    Struct(Vec<Type<'db>>),
    /// An enum: how many variants it has.
    Enum {
        /// The number of variants.
        variants: u64,
    },
}

/// Why a type has no layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutError {
    /// The type contains a type parameter. Layouts exist only once generics are instantiated.
    Generic,
    /// The type is [`TypeKind::Unknown`], from an error already reported.
    Unknown,
    /// A struct contains itself by value, so it would be infinitely large.
    Recursive,
    /// The size does not fit in a `u64`.
    TooLarge,
}

impl fmt::Display for LayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Generic => "the type has a type parameter, so its size is not known yet",
            Self::Unknown => "the type is unknown because of an earlier error",
            Self::Recursive => "the struct contains itself, so it would be infinitely large",
            Self::TooLarge => "the type is larger than 2^64 bytes",
        })
    }
}

impl Error for LayoutError {}

/// Bytes in a pointer or a 64-bit length.
const WORD: Layout = Layout { size: 8, align: 8 };

/// The layout of a value of type `ty`. `declared` says what a class declares; it is called once
/// for each class the type contains.
///
/// Cost: one visit per type nested in `ty`; nothing is cached, so callers that ask often should
/// cache the result in a query.
///
/// # Errors
///
/// Fails when the type contains a type parameter or an unknown type, when a struct contains
/// itself by value, or when the size does not fit in a `u64`.
pub fn layout<'db>(
    db: &'db dyn salsa::Database,
    ty: Type<'db>,
    declared: &dyn Fn(&Nominal<'db>) -> Declared<'db>,
) -> Result<Layout, LayoutError> {
    Layouter {
        db,
        declared,
        open: Vec::new(),
    }
    .layout(ty)
}

/// Computes layouts, remembering which structs are being laid out to catch recursion.
struct Layouter<'a, 'db> {
    /// The database the types are interned in.
    db: &'db dyn salsa::Database,
    /// Says what each class declares.
    declared: &'a dyn Fn(&Nominal<'db>) -> Declared<'db>,
    /// The structs whose layout is being computed, outermost first.
    open: Vec<Type<'db>>,
}

impl<'db> Layouter<'_, 'db> {
    /// The layout of `ty`.
    fn layout(&mut self, ty: Type<'db>) -> Result<Layout, LayoutError> {
        let too_large = LayoutError::TooLarge;
        match ty.kind(self.db) {
            TypeKind::Primitive(primitive) => Ok(primitive_layout(*primitive)),
            TypeKind::Str => WORD.repeat(2).ok_or(too_large),
            TypeKind::List(_) => WORD.repeat(3).ok_or(too_large),
            TypeKind::Standard(StandardType::Philox) => WORD.repeat(11).ok_or(too_large),
            TypeKind::None | TypeKind::Callable { .. } | TypeKind::Function { .. } => {
                Ok(Layout::EMPTY)
            }
            TypeKind::Tuple(items) => {
                let fields = self.all(items)?;
                Ok(Layout::record(fields).ok_or(too_large)?.layout())
            }
            TypeKind::Optional(value) => {
                let flag = primitive_layout(Primitive::Bool);
                let value = self.layout(*value)?;
                Ok(Layout::record([value, flag]).ok_or(too_large)?.layout())
            }
            TypeKind::Array { rank, .. } => WORD.repeat(1 + 2 * u64::from(*rank)).ok_or(too_large),
            TypeKind::Fixed { element, shape } => self
                .layout(*element)?
                .repeat(shape.elements())
                .ok_or(too_large),
            TypeKind::Struct(nominal) => self.structure(ty, nominal),
            TypeKind::Enum(nominal) => match (self.declared)(nominal) {
                Declared::Enum { variants } => Ok(tag(variants)),
                Declared::Struct(_) => Err(LayoutError::Unknown),
            },
            TypeKind::Fallible { value, errors } => {
                let mut payload = Layout::EMPTY;
                for &option in std::iter::once(value).chain(errors) {
                    let option = self.layout(option)?;
                    payload = Layout {
                        size: payload.size.max(option.size),
                        align: payload.align.max(option.align),
                    };
                }
                let flag = primitive_layout(Primitive::Bool);
                Ok(Layout::record([flag, payload]).ok_or(too_large)?.layout())
            }
            TypeKind::Parameter { .. } => Err(LayoutError::Generic),
            TypeKind::Unknown => Err(LayoutError::Unknown),
        }
    }

    /// The layouts of several types, in order.
    fn all(&mut self, types: &[Type<'db>]) -> Result<Vec<Layout>, LayoutError> {
        types.iter().map(|&ty| self.layout(ty)).collect()
    }

    /// The layout of a struct type: its fields as a record.
    fn structure(&mut self, ty: Type<'db>, nominal: &Nominal<'db>) -> Result<Layout, LayoutError> {
        if self.open.contains(&ty) {
            return Err(LayoutError::Recursive);
        }
        let Declared::Struct(fields) = (self.declared)(nominal) else {
            return Err(LayoutError::Unknown);
        };
        self.open.push(ty);
        let fields = self.all(&fields);
        self.open.pop();
        Ok(Layout::record(fields?)
            .ok_or(LayoutError::TooLarge)?
            .layout())
    }
}

/// The layout of a primitive.
fn primitive_layout(primitive: Primitive) -> Layout {
    Layout {
        size: primitive.size(),
        align: primitive.align(),
    }
}

/// The smallest unsigned integer that numbers `variants` variants: one byte up to 256.
fn tag(variants: u64) -> Layout {
    let bytes = match variants {
        0..=0x100 => 1,
        0x101..=0x1_0000 => 2,
        0x1_0001..=0x1_0000_0000 => 4,
        _ => 8,
    };
    Layout {
        size: bytes,
        align: bytes,
    }
}
