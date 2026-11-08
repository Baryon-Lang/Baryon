//! Format specifications: the part of an f-string interpolation after the colon, as in
//! `f"{energy:10.4f}"`.
//!
//! The language takes Python's format specification mini-language (decision 0004), so a format
//! that works in Python works here and writes the same text:
//!
//! ```text
//! [[fill]align][sign][#][0][width][grouping][.precision][type]
//! ```
//!
//! A specification is parsed and checked against the value's type at compile time (decision
//! 0022), where Python would raise `ValueError` at run time; the runtime then gets its parts and
//! never sees a bad one. Two parts of Python's language are not supported yet, and are rejected
//! with a message saying so: the `z` flag and the `n` and `c` presentation types.
//!
//! # Example
//!
//! ```
//! use types::{Align, FormatClass, FormatSpec, Presentation};
//!
//! let spec = FormatSpec::parse(">10.4f").unwrap();
//! assert_eq!(spec.align, Some(Align::Right));
//! assert_eq!((spec.width, spec.precision), (Some(10), Some(4)));
//! assert_eq!(spec.presentation, Presentation::Fixed);
//! assert!(spec.check(FormatClass::Float).is_ok());
//! assert!(spec.check(FormatClass::Text).is_err());
//! ```

use std::fmt;

/// A parsed format specification. [`FormatSpec::default`] is the empty specification, which
/// writes a value as `print` does.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FormatSpec {
    /// The character that pads the value to `width`: a space unless the specification names one.
    pub fill: char,
    /// Where the value sits in `width`, if the specification says; otherwise numbers go right
    /// and text left, as in Python.
    pub align: Option<Align>,
    /// Which numbers get a sign.
    pub sign: Sign,
    /// `#`: the alternate form, such as `0x` before a hexadecimal number.
    pub alternate: bool,
    /// `0` before the width: pad a number with zeros after its sign.
    pub zero: bool,
    /// The least number of characters to write.
    pub width: Option<u32>,
    /// The separator between groups of digits: `,` or `_`.
    pub grouping: Option<char>,
    /// Digits after the point for `f`, `e` and `%`, significant digits for `g`, or the most
    /// characters of a `str` to write.
    pub precision: Option<u32>,
    /// How the value is written.
    pub presentation: Presentation,
}

impl Default for FormatSpec {
    fn default() -> Self {
        Self {
            fill: ' ',
            align: None,
            sign: Sign::Negative,
            alternate: false,
            zero: false,
            width: None,
            grouping: None,
            precision: None,
            presentation: Presentation::Default,
        }
    }
}

impl fmt::Display for FormatSpec {
    /// Writes the specification as the source would, so parsing the text gives it back:
    /// `*^+#012,.3f`. The default specification writes nothing.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(align) = self.align {
            let symbol = match align {
                Align::Left => '<',
                Align::Right => '>',
                Align::Center => '^',
                Align::AfterSign => '=',
            };
            write!(formatter, "{}{symbol}", self.fill)?;
        }
        match self.sign {
            Sign::Negative => {}
            Sign::Always => formatter.write_str("+")?,
            Sign::Space => formatter.write_str(" ")?,
        }
        if self.alternate {
            formatter.write_str("#")?;
        }
        if self.zero {
            formatter.write_str("0")?;
        }
        if let Some(width) = self.width {
            write!(formatter, "{width}")?;
        }
        if let Some(separator) = self.grouping {
            write!(formatter, "{separator}")?;
        }
        if let Some(precision) = self.precision {
            write!(formatter, ".{precision}")?;
        }
        if let Some(letter) = self.presentation.letter() {
            write!(formatter, "{letter}")?;
        }
        Ok(())
    }
}

/// Where a value sits in its width.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Align {
    /// `<`: at the start, padding after.
    Left,
    /// `>`: at the end, padding before.
    Right,
    /// `^`: in the middle, any odd padding character after.
    Center,
    /// `=`: padding between a number's sign and its digits, as in `-000042`.
    AfterSign,
}

/// Which numbers are written with a sign.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sign {
    /// `-`, the default: only negative numbers.
    Negative,
    /// `+`: every number, `+` before a positive one.
    Always,
    /// A space: a space before a positive number, so columns line up with negative ones.
    Space,
}

/// How a value is written: the type letter at the end of a specification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Presentation {
    /// No letter: as `print` writes the value, or as `g` with a precision for a float.
    Default,
    /// `s`: text as it is.
    Text,
    /// `d`: an integer in decimal.
    Decimal,
    /// `b`: an integer in binary.
    Binary,
    /// `o`: an integer in octal.
    Octal,
    /// `x`: an integer in hexadecimal, lowercase.
    Hex,
    /// `X`: an integer in hexadecimal, uppercase.
    HexUpper,
    /// `e`: scientific notation, `1.500000e+03`.
    Exponent,
    /// `E`: scientific notation with `E`.
    ExponentUpper,
    /// `f`: fixed point, `1500.000000`.
    Fixed,
    /// `F`: fixed point, with `INF` and `NAN` in capitals.
    FixedUpper,
    /// `g`: fixed point or scientific notation, whichever is shorter, without trailing zeros.
    General,
    /// `G`: as `g`, with `E`, `INF` and `NAN` in capitals.
    GeneralUpper,
    /// `%`: the number times 100 in fixed point, then `%`.
    Percent,
}

impl Presentation {
    /// The letter that names it, or `None` for [`Default`](Self::Default).
    ///
    /// ```
    /// assert_eq!(types::Presentation::Fixed.letter(), Some('f'));
    /// ```
    #[must_use]
    pub fn letter(self) -> Option<char> {
        Some(match self {
            Self::Default => return None,
            Self::Text => 's',
            Self::Decimal => 'd',
            Self::Binary => 'b',
            Self::Octal => 'o',
            Self::Hex => 'x',
            Self::HexUpper => 'X',
            Self::Exponent => 'e',
            Self::ExponentUpper => 'E',
            Self::Fixed => 'f',
            Self::FixedUpper => 'F',
            Self::General => 'g',
            Self::GeneralUpper => 'G',
            Self::Percent => '%',
        })
    }

    /// The presentation `letter` names, if any.
    fn from_letter(letter: char) -> Option<Self> {
        Some(match letter {
            's' => Self::Text,
            'd' => Self::Decimal,
            'b' => Self::Binary,
            'o' => Self::Octal,
            'x' => Self::Hex,
            'X' => Self::HexUpper,
            'e' => Self::Exponent,
            'E' => Self::ExponentUpper,
            'f' => Self::Fixed,
            'F' => Self::FixedUpper,
            'g' => Self::General,
            'G' => Self::GeneralUpper,
            '%' => Self::Percent,
            _ => return None,
        })
    }

    /// Whether it writes an integer as an integer: `d`, `b`, `o`, `x` or `X`.
    fn is_integer(self) -> bool {
        matches!(
            self,
            Self::Decimal | Self::Binary | Self::Octal | Self::Hex | Self::HexUpper
        )
    }
}

/// The kind of value a specification formats, which decides what it may contain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FormatClass {
    /// An integer of any width.
    Integer,
    /// A float of any width.
    Float,
    /// A `bool`, written `True` or `False` by the empty specification and as the integer 1 or 0
    /// by any other, as in Python.
    Bool,
    /// A `str`.
    Text,
}

impl FormatClass {
    /// The name of the class in a message.
    fn name(self) -> &'static str {
        match self {
            Self::Integer => "an integer",
            Self::Float => "a float",
            Self::Bool => "a `bool`",
            Self::Text => "a `str`",
        }
    }
}

/// Why a format specification is rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatError {
    /// The text is not a specification: something is left over after its last part.
    Invalid(String),
    /// A part of Python's language this compiler does not support yet, such as `n`.
    Unsupported(&'static str),
    /// A part the value's kind does not allow, such as a sign on a `str`.
    NotFor {
        /// The part, as a message names it: "a sign", "format code `d`".
        part: String,
        /// The kind of value.
        class: FormatClass,
    },
}

impl fmt::Display for FormatError {
    /// Writes the message, which says what to change.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(text) => write!(formatter, "`{text}` is not a format specification"),
            Self::Unsupported(part) => write!(formatter, "{part} is not supported yet"),
            Self::NotFor { part, class } => {
                write!(formatter, "{part} cannot format {}", class.name())
            }
        }
    }
}

impl FormatSpec {
    /// Parses `text`, the specification after the colon, such as `>10.4f`.
    ///
    /// # Errors
    ///
    /// [`FormatError::Invalid`] when `text` is not a specification, and
    /// [`FormatError::Unsupported`] for the `z` flag and the `n` and `c` types.
    pub fn parse(text: &str) -> Result<Self, FormatError> {
        let mut spec = Self::default();
        let characters: Vec<char> = text.chars().collect();
        let mut at = 0;
        let align = |character: char| match character {
            '<' => Some(Align::Left),
            '>' => Some(Align::Right),
            '^' => Some(Align::Center),
            '=' => Some(Align::AfterSign),
            _ => None,
        };
        if let Some(found) = characters.get(1).copied().and_then(align) {
            spec.fill = characters[0];
            spec.align = Some(found);
            at = 2;
        } else if let Some(found) = characters.first().copied().and_then(align) {
            spec.align = Some(found);
            at = 1;
        }
        if let Some(&symbol @ ('+' | '-' | ' ')) = characters.get(at) {
            spec.sign = match symbol {
                '+' => Sign::Always,
                ' ' => Sign::Space,
                _ => Sign::Negative,
            };
            at += 1;
        }
        if characters.get(at) == Some(&'z') {
            return Err(FormatError::Unsupported("the `z` flag"));
        }
        if characters.get(at) == Some(&'#') {
            spec.alternate = true;
            at += 1;
        }
        if characters.get(at) == Some(&'0') {
            spec.zero = true;
            at += 1;
        }
        let invalid = || FormatError::Invalid(text.to_owned());
        spec.width = number(&characters, &mut at).map_err(|()| invalid())?;
        if let Some(&separator @ (',' | '_')) = characters.get(at) {
            spec.grouping = Some(separator);
            at += 1;
        }
        if characters.get(at) == Some(&'.') {
            at += 1;
            spec.precision = Some(
                number(&characters, &mut at)
                    .map_err(|()| invalid())?
                    .ok_or_else(invalid)?,
            );
        }
        if let Some(&letter) = characters.get(at) {
            spec.presentation = match letter {
                'n' => return Err(FormatError::Unsupported("format code `n`")),
                'c' => return Err(FormatError::Unsupported("format code `c`")),
                _ => Presentation::from_letter(letter).ok_or_else(invalid)?,
            };
            at += 1;
        }
        if at == characters.len() {
            Ok(spec)
        } else {
            Err(invalid())
        }
    }

    /// Checks that the specification can format a value of `class`, with the rules Python
    /// applies when it formats one.
    ///
    /// # Errors
    ///
    /// [`FormatError::NotFor`] naming the first part `class` does not allow.
    pub fn check(&self, class: FormatClass) -> Result<(), FormatError> {
        let not_for = |part: String| Err(FormatError::NotFor { part, class });
        let code = |presentation: Presentation| {
            format!(
                "format code `{}`",
                presentation.letter().unwrap_or_default()
            )
        };
        let presentation = self.presentation;
        match class {
            FormatClass::Text => {
                if !matches!(presentation, Presentation::Default | Presentation::Text) {
                    return not_for(code(presentation));
                }
                if self.sign != Sign::Negative {
                    return not_for("a sign".to_owned());
                }
                if self.alternate {
                    return not_for("`#`".to_owned());
                }
                if let Some(separator) = self.grouping {
                    return not_for(format!("the grouping `{separator}`"));
                }
                if self.align == Some(Align::AfterSign) {
                    return not_for("`=` alignment".to_owned());
                }
            }
            FormatClass::Integer | FormatClass::Bool => {
                if presentation == Presentation::Text {
                    return not_for(code(presentation));
                }
                if presentation.is_integer() || presentation == Presentation::Default {
                    if self.precision.is_some() {
                        return not_for("a precision".to_owned());
                    }
                    if self.grouping == Some(',')
                        && presentation.is_integer()
                        && presentation != Presentation::Decimal
                    {
                        return not_for(format!("the grouping `,` with {}", code(presentation)));
                    }
                }
            }
            FormatClass::Float => {
                if presentation.is_integer() || presentation == Presentation::Text {
                    return not_for(code(presentation));
                }
            }
        }
        Ok(())
    }
}

/// The decimal number at `at` in `characters`, if there is one, moving `at` past it.
///
/// # Errors
///
/// `Err` when it does not fit in a `u32`.
fn number(characters: &[char], at: &mut usize) -> Result<Option<u32>, ()> {
    let start = *at;
    let mut value: u32 = 0;
    while let Some(digit) = characters
        .get(*at)
        .and_then(|character| character.to_digit(10))
    {
        value = value
            .checked_mul(10)
            .and_then(|value| value.checked_add(digit))
            .ok_or(())?;
        *at += 1;
    }
    Ok((*at > start).then_some(value))
}

#[cfg(test)]
mod tests;
