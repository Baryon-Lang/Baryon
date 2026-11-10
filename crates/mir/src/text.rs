//! The methods of `str` a [`Builtin::Text`](crate::Builtin::Text) call performs (roadmap
//! 1.13.12): taking characters by position, splitting, stripping, joining, searching, replacing
//! and changing case, each as Python's method of the same name.

/// A method of `str`, called by [`Builtin::Text`](crate::Builtin::Text).
///
/// Every `str` argument is passed `Borrow`, or `Owned` for a literal, and is read, never
/// changed. A method that makes text gives a new `str` that owns its bytes (decision 0022), never
/// a view into an argument, so it outlives the text it came from. Positions count characters,
/// code points, as Python's do, so each method that takes or gives one reads the text from its
/// start, in time linear in its length.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub enum TextMethod {
    /// `text[i]` and `text[i:j:k]`: four arguments, the `str`, then the position of the first
    /// character taken, the number of characters taken and the step between them, each an `i64`
    /// passed `Owned`; the destination is a `str`. The stage that writes the call checks first
    /// that every position taken lies inside the text, as for a list slice; the call itself
    /// cannot fail.
    Take,
    /// `text.split(separator)`: two arguments, the text and the separator; the destination is a
    /// `list[str]` of the pieces between the separators, in a buffer with room for exactly them.
    /// Two separators next to each other give an empty piece. An empty separator panics with
    /// `empty separator`, Python's `ValueError` message.
    Split,
    /// `text.split()`: one argument; the destination is a `list[str]` of the words that runs of
    /// whitespace separate, with no empty word. The call cannot fail.
    SplitWhitespace,
    /// `text.strip()`, or `text.strip(characters)`: one or two arguments, the text and the
    /// characters to remove, whitespace when there is no second; the destination is a `str`
    /// without them at either end. The call cannot fail.
    Strip,
    /// `text.lstrip()`: as [`Strip`](Self::Strip), at the start only.
    StripStart,
    /// `text.rstrip()`: as [`Strip`](Self::Strip), at the end only.
    StripEnd,
    /// `separator.join(pieces)`: two arguments, the separator `str` and a `list[str]`; the
    /// destination is a `str`. The call cannot fail.
    Join,
    /// `text.startswith(prefix)`: two arguments; the destination is a `bool`. The call cannot
    /// fail.
    StartsWith,
    /// `text.endswith(suffix)`: two arguments; the destination is a `bool`. The call cannot fail.
    EndsWith,
    /// `text.find(needle)`: two arguments; the destination is an `i64`, the character position
    /// of the first occurrence, 0 for the empty string, or -1. The call cannot fail.
    Find,
    /// `text.replace(old, new)`: three arguments; the destination is a `str` with every
    /// occurrence of `old` replaced. An empty `old` matches between every two characters and at
    /// both ends, as in Python. The call cannot fail.
    Replace,
    /// `text.lower()`: one argument; the destination is a `str`, by Unicode's full case mapping.
    /// The call cannot fail.
    Lower,
    /// `text.upper()`: one argument; the destination is a `str`, by Unicode's full case mapping,
    /// so `"ß"` becomes `"SS"`. The call cannot fail.
    Upper,
}

impl TextMethod {
    /// The name the source calls it by.
    ///
    /// ```
    /// assert_eq!(mir::TextMethod::Take.name(), "str slice");
    /// assert_eq!(mir::TextMethod::SplitWhitespace.name(), "str.split");
    /// assert_eq!(mir::TextMethod::StripEnd.name(), "str.rstrip");
    /// ```
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Take => "str slice",
            Self::Split | Self::SplitWhitespace => "str.split",
            Self::Strip => "str.strip",
            Self::StripStart => "str.lstrip",
            Self::StripEnd => "str.rstrip",
            Self::Join => "str.join",
            Self::StartsWith => "str.startswith",
            Self::EndsWith => "str.endswith",
            Self::Find => "str.find",
            Self::Replace => "str.replace",
            Self::Lower => "str.lower",
            Self::Upper => "str.upper",
        }
    }
}
