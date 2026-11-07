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
//!
//! The other types (complex numbers, arrays, strings, lists, optionals and errors) are
//! [`Unsupported`] until the slices of roadmap 1.10 that add them.

use crate::Unsupported;
use types::{Declared, Float, Nominal, Primitive, Type, TypeKind};

/// What representing a type needs: the database types are read from, and what each struct and
/// enum declares.
#[derive(Clone, Copy)]
pub(crate) struct Types<'a, 'db> {
    /// The database types are read from.
    pub(crate) db: &'db dyn database::Db,
    /// What each struct or enum declares, with its type arguments substituted.
    pub(crate) declared: &'a dyn Fn(&Nominal<'db>) -> Declared<'db>,
}

/// The LLVM representation of a language type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Repr {
    /// A type with no value to store: `None`.
    Empty,
    /// A type LLVM holds in a register or memory, written as LLVM writes it, such as `double`.
    Value(String),
}

impl<'db> Types<'_, 'db> {
    /// The representation of `ty`, or what is not supported yet.
    pub(crate) fn repr(&self, ty: Type<'db>) -> Result<Repr, Unsupported> {
        match ty.kind(self.db) {
            TypeKind::None => Ok(Repr::Empty),
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
            TypeKind::Tuple(items) => self.structure(items),
            TypeKind::Struct(nominal) => match (self.declared)(nominal) {
                Declared::Struct(fields) => self.structure(&fields),
                Declared::Enum { .. } => Err(self.unsupported(ty)),
            },
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
            TypeKind::Primitive(_) | TypeKind::None | TypeKind::Enum(_) => true,
            TypeKind::Tuple(items) => items.iter().all(|&item| self.by_value(item)),
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
