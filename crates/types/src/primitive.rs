//! The number types and `bool`: the types a value of which fits in a register.

use std::fmt;

/// A fixed-width integer type (language reference, "Types", "Numbers").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Integer {
    /// `i8`: signed, 8 bits.
    I8,
    /// `i16`: signed, 16 bits.
    I16,
    /// `i32`: signed, 32 bits.
    I32,
    /// `i64`: signed, 64 bits. Python's `int` means this type.
    I64,
    /// `u8`: unsigned, 8 bits.
    U8,
    /// `u16`: unsigned, 16 bits.
    U16,
    /// `u32`: unsigned, 32 bits.
    U32,
    /// `u64`: unsigned, 64 bits.
    U64,
}

impl Integer {
    /// The width in bits: 8, 16, 32 or 64.
    #[must_use]
    pub fn bits(self) -> u32 {
        match self {
            Self::I8 | Self::U8 => 8,
            Self::I16 | Self::U16 => 16,
            Self::I32 | Self::U32 => 32,
            Self::I64 | Self::U64 => 64,
        }
    }

    /// Whether the type holds negative numbers.
    #[must_use]
    pub fn is_signed(self) -> bool {
        matches!(self, Self::I8 | Self::I16 | Self::I32 | Self::I64)
    }

    /// The type's name as a program writes it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
        }
    }
}

/// An IEEE 754 binary floating-point type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Float {
    /// `f32`: single precision.
    F32,
    /// `f64`: double precision. Python's `float` means this type.
    F64,
}

impl Float {
    /// The width in bits: 32 or 64.
    #[must_use]
    pub fn bits(self) -> u32 {
        match self {
            Self::F32 => 32,
            Self::F64 => 64,
        }
    }

    /// The type's name as a program writes it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }
}

/// A complex number type: a real and an imaginary part of one float type, stored in that order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Complex {
    /// `complex64`: two `f32`s.
    C64,
    /// `complex128`: two `f64`s. Python's `complex` means this type.
    C128,
}

impl Complex {
    /// The float type of each part: `f32` for `complex64`, `f64` for `complex128`.
    #[must_use]
    pub fn part(self) -> Float {
        match self {
            Self::C64 => Float::F32,
            Self::C128 => Float::F64,
        }
    }

    /// The complex type whose parts are `part`, the result of mixing `part` with a complex
    /// number (language reference, "Types", "Number mixing", rule 3).
    #[must_use]
    pub fn with_part(part: Float) -> Self {
        match part {
            Float::F32 => Self::C64,
            Float::F64 => Self::C128,
        }
    }

    /// The type's name as a program writes it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::C64 => "complex64",
            Self::C128 => "complex128",
        }
    }
}

/// A primitive type: a number or `bool`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Primitive {
    /// `bool`.
    Bool,
    /// A fixed-width integer.
    Integer(Integer),
    /// A floating-point number.
    Float(Float),
    /// A complex number.
    Complex(Complex),
}

impl Primitive {
    /// The primitive a built-in type name means, including Python's names: `int` is `i64`,
    /// `float` is `f64` and `complex` is `complex128`. `None` for any other name.
    ///
    /// ```
    /// use types::{Float, Integer, Primitive};
    ///
    /// assert_eq!(Primitive::from_name("float"), Some(Primitive::Float(Float::F64)));
    /// assert_eq!(Primitive::from_name("u8"), Some(Primitive::Integer(Integer::U8)));
    /// assert_eq!(Primitive::from_name("str"), None);
    /// ```
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "bool" => Self::Bool,
            "i8" => Self::Integer(Integer::I8),
            "i16" => Self::Integer(Integer::I16),
            "i32" => Self::Integer(Integer::I32),
            "i64" | "int" => Self::Integer(Integer::I64),
            "u8" => Self::Integer(Integer::U8),
            "u16" => Self::Integer(Integer::U16),
            "u32" => Self::Integer(Integer::U32),
            "u64" => Self::Integer(Integer::U64),
            "f32" => Self::Float(Float::F32),
            "f64" | "float" => Self::Float(Float::F64),
            "complex64" => Self::Complex(Complex::C64),
            "complex128" | "complex" => Self::Complex(Complex::C128),
            _ => return None,
        })
    }

    /// The size in bytes, which is also the alignment except for complex numbers, which align
    /// like one of their parts.
    #[must_use]
    pub fn size(self) -> u64 {
        match self {
            Self::Bool => 1,
            Self::Integer(integer) => u64::from(integer.bits() / 8),
            Self::Float(float) => u64::from(float.bits() / 8),
            Self::Complex(complex) => 2 * u64::from(complex.part().bits() / 8),
        }
    }

    /// The alignment in bytes.
    #[must_use]
    pub fn align(self) -> u64 {
        match self {
            Self::Complex(complex) => u64::from(complex.part().bits() / 8),
            _ => self.size(),
        }
    }
}

impl fmt::Display for Primitive {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Bool => "bool",
            Self::Integer(integer) => integer.name(),
            Self::Float(float) => float.name(),
            Self::Complex(complex) => complex.name(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_round_trips() {
        for name in [
            "bool",
            "i8",
            "i16",
            "i32",
            "i64",
            "u8",
            "u16",
            "u32",
            "u64",
            "f32",
            "f64",
            "complex64",
            "complex128",
        ] {
            let primitive = Primitive::from_name(name).unwrap();
            assert_eq!(primitive.to_string(), name);
        }
    }

    #[test]
    fn python_names_are_the_wide_types() {
        assert_eq!(Primitive::from_name("int").unwrap().to_string(), "i64");
        assert_eq!(Primitive::from_name("float").unwrap().to_string(), "f64");
        assert_eq!(
            Primitive::from_name("complex").unwrap().to_string(),
            "complex128"
        );
    }

    #[test]
    fn sizes_and_alignments_match_c() {
        let size_align = |name| {
            let primitive = Primitive::from_name(name).unwrap();
            (primitive.size(), primitive.align())
        };
        assert_eq!(size_align("bool"), (1, 1));
        assert_eq!(size_align("u16"), (2, 2));
        assert_eq!(size_align("f32"), (4, 4));
        assert_eq!(size_align("i64"), (8, 8));
        assert_eq!(size_align("complex64"), (8, 4));
        assert_eq!(size_align("complex128"), (16, 8));
    }

    #[test]
    fn integer_signs_and_widths() {
        assert!(Integer::I32.is_signed());
        assert!(!Integer::U32.is_signed());
        assert_eq!(Integer::U64.bits(), 64);
        assert_eq!(Complex::with_part(Float::F32), Complex::C64);
    }
}
