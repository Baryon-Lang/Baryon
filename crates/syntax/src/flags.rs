//! Yes-or-no facts about a node that its kind alone does not say, such as `async def`.

use std::fmt;
use std::ops::BitOr;

/// A set of single-bit facts about a node. Most nodes have none.
///
/// ```
/// use syntax::Flags;
///
/// let flags = Flags::ASYNC | Flags::PARENTHESIZED;
/// assert!(flags.contains(Flags::ASYNC));
/// assert!(!flags.contains(Flags::STAR));
/// assert_eq!(flags.to_string(), "async parenthesized");
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Flags(
    /// One bit per fact, as named by the associated constants.
    u8,
);

impl Flags {
    /// No facts set.
    pub const NONE: Self = Self(0);
    /// `async def`, `async for`, `async with`, or an `async for` comprehension clause.
    pub const ASYNC: Self = Self(1);
    /// `try` with `except*` handlers.
    pub const STAR: Self = Self(1 << 1);
    /// A tuple or generator written inside its own parentheses.
    pub const PARENTHESIZED: Self = Self(1 << 2);
    /// An f-string interpolation written `{x=}`, which prints its own source text.
    pub const DEBUG: Self = Self(1 << 3);
    /// An f-string interpolation converted with `!s`.
    pub const CONVERT_STR: Self = Self(1 << 4);
    /// An f-string interpolation converted with `!r`.
    pub const CONVERT_REPR: Self = Self(1 << 5);
    /// An f-string interpolation converted with `!a`.
    pub const CONVERT_ASCII: Self = Self(1 << 6);

    /// Every flag with its name, in the order [`Display`](fmt::Display) prints them.
    const NAMES: [(Self, &'static str); 7] = [
        (Self::ASYNC, "async"),
        (Self::STAR, "star"),
        (Self::PARENTHESIZED, "parenthesized"),
        (Self::DEBUG, "debug"),
        (Self::CONVERT_STR, "!s"),
        (Self::CONVERT_REPR, "!r"),
        (Self::CONVERT_ASCII, "!a"),
    ];

    /// Whether every fact in `other` is set in `self`.
    #[must_use]
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether no fact is set.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl BitOr for Flags {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Prints the set flags' names separated by spaces, or nothing if none is set.
impl fmt::Display for Flags {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut names = Self::NAMES
            .iter()
            .filter(|(flag, _)| self.contains(*flag))
            .map(|(_, name)| name);
        if let Some(first) = names.next() {
            formatter.write_str(first)?;
        }
        for name in names {
            write!(formatter, " {name}")?;
        }
        Ok(())
    }
}
