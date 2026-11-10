//! How each language type is represented in LLVM IR.
//!
//! The representations follow the memory layouts in [`types::layout`], so a value the backend
//! stores has the size and alignment every other stage expects:
//!
//! | Type | LLVM type |
//! | --- | --- |
//! | `bool` | `i1` in registers, which LLVM stores as one byte |
//! | `i8` … `i64`, `u8` … `u64` | `i8` … `i64`; LLVM integers carry no sign, the operations do |
//! | `f32`, `f64` | `float`, `double` |
//! | an enum without data | the unsigned integer its layout gives it, such as `i8` |
//! | a tuple, or a struct | a literal structure of its items or fields in declaration order |
//! | `None` | nothing: a `None` value is never stored, and a function returning it returns `void` |
//! | `Fixed[T, (n, m)]` | nested arrays `[n x [m x T]]`, row-major (decision 0008) |
//! | `Raises[T, E1 \| E2]` | `{ i8, [n x iA] }`: the tag, then room for the largest of `T` and the errors |
//! | `Array[T, rank]` | its header `{ ptr, [rank x i64], [rank x i64] }`: the elements, each length, each stride |
//! | `T \| None` | `{ T, i1 }`: the value, then whether it is present; `None` is all zeros |
//! | `str` | `{ ptr, i64 }`: the bytes, then the length with a "not owned" top bit (decision 0022) |
//!
//! The other types (complex numbers and lists) are
//! [`Unsupported`] until the slices of roadmap 1.10 that add them.

use crate::text::TEXT;
use crate::{ClassNames, Unsupported};
use types::{Declared, Float, Nominal, Primitive, Type, TypeKind};

/// What representing and printing a type needs: the database types are read from, what each
/// struct and enum declares, and the names `print` writes for it.
#[derive(Clone, Copy)]
pub(crate) struct Types<'a, 'db> {
    /// The database types are read from.
    pub(crate) db: &'db dyn database::Db,
    /// What each struct or enum declares, with its type arguments substituted.
    pub(crate) declared: &'a dyn Fn(&Nominal<'db>) -> Declared<'db>,
    /// The name of each struct or enum and of its fields or members.
    pub(crate) names: &'a dyn Fn(&Nominal<'db>) -> ClassNames,
}

/// The LLVM representation of a language type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Repr {
    /// A type with no value to store: `None`, or a function passed as an argument, whose type
    /// already names it.
    Empty,
    /// A type LLVM holds in a register or memory, written as LLVM writes it, such as `double`.
    Value(String),
}

impl<'db> Types<'_, 'db> {
    /// The representation of `ty`, or what is not supported yet.
    pub(crate) fn repr(&self, ty: Type<'db>) -> Result<Repr, Unsupported> {
        match ty.kind(self.db) {
            // A function value takes no bytes: its type names the function (decision 0026).
            TypeKind::None | TypeKind::Callable { .. } | TypeKind::Function { .. } => {
                Ok(Repr::Empty)
            }
            _ => self.value(ty).map(Repr::Value),
        }
    }

    /// The LLVM type of a value of `ty`, which must not be `None`.
    pub(crate) fn value(&self, ty: Type<'db>) -> Result<String, Unsupported> {
        match ty.kind(self.db) {
            TypeKind::Primitive(Primitive::Bool) => Ok("i1".to_owned()),
            TypeKind::Primitive(Primitive::Integer(integer)) => Ok(format!("i{}", integer.bits())),
            TypeKind::Primitive(Primitive::Float(Float::F32)) => Ok("float".to_owned()),
            TypeKind::Primitive(Primitive::Float(Float::F64)) => Ok("double".to_owned()),
            TypeKind::Str => Ok(TEXT.to_owned()),
            TypeKind::Tuple(items) => self.structure(items),
            TypeKind::Struct(nominal) => match (self.declared)(nominal) {
                Declared::Struct(fields) => self.structure(&fields),
                Declared::Enum { .. } => Err(self.unsupported(ty)),
            },
            TypeKind::Fixed { element, shape } => {
                let mut nested = self.value(*element)?;
                for length in shape.lengths().iter().rev() {
                    nested = format!("[{length} x {nested}]");
                }
                Ok(nested)
            }
            TypeKind::Fallible { .. } => {
                // A one-byte tag, then the payload at its alignment, as `types::layout` lays it
                // out: the whole is the tag padded to the alignment, then the payload.
                let layout =
                    types::layout(self.db, ty, self.declared).map_err(|_| self.unsupported(ty))?;
                let align = layout.align();
                let count = (layout.size() - align) / align;
                Ok(format!("{{ i8, [{count} x i{}] }}", align * 8))
            }
            TypeKind::Array { rank, .. } => {
                Ok(format!("{{ ptr, [{rank} x i64], [{rank} x i64] }}"))
            }
            TypeKind::List(_) => Ok(crate::list::HEADER.to_owned()),
            // A pointer to the runtime's table, null while empty (the `dict` module).
            TypeKind::Dict { .. } => Ok("ptr".to_owned()),
            TypeKind::Optional(value) => Ok(format!("{{ {}, i1 }}", self.value(*value)?)),
            // The runtime's `Philox`: 11 words it reads and writes through a pointer.
            TypeKind::Standard(types::StandardType::Philox) => Ok("[11 x i64]".to_owned()),
            TypeKind::Enum(_) => {
                let layout =
                    types::layout(self.db, ty, self.declared).map_err(|_| self.unsupported(ty))?;
                Ok(format!("i{}", layout.size() * 8))
            }
            _ => Err(self.unsupported(ty)),
        }
    }

    /// The literal structure `{ a, b }` of `items`, each stored in order.
    fn structure(&self, items: &[Type<'db>]) -> Result<String, Unsupported> {
        let fields = items
            .iter()
            .map(|&item| self.value(item))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(format!("{{ {} }}", fields.join(", ")))
    }

    /// What a value of `ty` is, for the operators: an integer of a width and sign, a float of a
    /// width, or a `bool`.
    pub(crate) fn number(&self, ty: Type<'db>) -> Option<Number> {
        match ty.kind(self.db) {
            TypeKind::Primitive(Primitive::Bool) => Some(Number::Bool),
            TypeKind::Primitive(Primitive::Integer(integer)) => Some(Number::Integer {
                bits: integer.bits(),
                signed: integer.is_signed(),
            }),
            TypeKind::Primitive(Primitive::Float(float)) => Some(Number::Float(float.bits())),
            TypeKind::Enum(_) => {
                let bits = types::layout(self.db, ty, self.declared).ok()?.size() * 8;
                Some(Number::Integer {
                    bits: u32::try_from(bits).ok()?,
                    signed: false,
                })
            }
            _ => None,
        }
    }

    /// The error for a type this backend does not represent yet.
    pub(crate) fn unsupported(&self, ty: Type<'db>) -> Unsupported {
        Unsupported::new(format!("values of type `{}`", ty.display(self.db)))
    }

    /// Whether a value of `ty` is passed by value when borrowed: the copy types (decision 0006,
    /// language reference "Copy types") a read-only borrow cannot tell apart from a copy.
    pub(crate) fn by_value(&self, ty: Type<'db>) -> bool {
        match ty.kind(self.db) {
            TypeKind::Primitive(_)
            | TypeKind::None
            | TypeKind::Enum(_)
            | TypeKind::Callable { .. }
            | TypeKind::Function { .. } => true,
            TypeKind::Tuple(items) => items.iter().all(|&item| self.by_value(item)),
            TypeKind::Fixed { element, .. } | TypeKind::Optional(element) => {
                self.by_value(*element)
            }
            _ => false,
        }
    }
}

/// The kind of scalar an operator works on, which picks its LLVM instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Number {
    /// `bool`, an `i1`.
    Bool,
    /// An integer of `bits` bits, signed or not.
    Integer {
        /// The width in bits: 8, 16, 32 or 64.
        bits: u32,
        /// Whether its values can be negative.
        signed: bool,
    },
    /// A float of this many bits: 32 or 64.
    Float(u32),
}
