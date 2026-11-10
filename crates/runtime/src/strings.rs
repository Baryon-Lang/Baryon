//! The text processing behind `str`'s methods (roadmap 1.13.12): taking characters by position,
//! splitting, stripping, joining, searching, replacing and changing case, each as Python's `str`
//! method of the same name does it.
//!
//! These functions are safe Rust over `&str`. The exported functions in `abi::string_methods` and `abi::string_lists`
//! turn compiled code's pointers into `&str`s, call these, and hand the results back as owned
//! text. Positions are counted in characters, code points, as Python counts them, never in
//! bytes, so each function that takes or gives a position reads the text from its start: the
//! cost is linear in the length.

/// Whether `character` is whitespace as Python's `str.isspace`, `str.split()` and `str.strip()`
/// see it: Unicode's `White_Space` characters, which Rust's [`char::is_whitespace`] tests, and
/// the four ASCII separators `\x1c` to `\x1f`, which Python counts too.
pub(crate) fn is_python_whitespace(character: char) -> bool {
    character.is_whitespace() || ('\x1c'..='\x1f').contains(&character)
}

/// The characters of `text` at `start`, `start + step`, `start + 2 * step` and so on, `count` of
/// them, as one new string: Python's `text[i]` when `count` is 1, and a slice `text[i:j:k]`
/// otherwise. Positions are character positions. The caller has checked that every position
/// lies inside the text; one that does not is skipped, never read.
///
/// A step of 1 copies one run of bytes; any other step collects the characters first, so it
/// costs one pass and one array of the text's characters.
pub(crate) fn take(text: &str, start: i64, count: i64, step: i64) -> String {
    let (Ok(start), Ok(count)) = (usize::try_from(start), usize::try_from(count)) else {
        return String::new();
    };
    if step == 1 {
        return text.chars().skip(start).take(count).collect();
    }
    let characters: Vec<char> = text.chars().collect();
    let Ok(start) = i64::try_from(start) else {
        return String::new();
    };
    (0..count)
        .filter_map(|taken| {
            let position = i64::try_from(taken)
                .ok()?
                .checked_mul(step)?
                .checked_add(start)?;
            characters.get(usize::try_from(position).ok()?).copied()
        })
        .collect()
}

/// The pieces of `text` between each occurrence of `separator`, as Python's
/// `text.split(separator)`: two separators next to each other give an empty piece, and the empty
/// text gives one empty piece. `separator` is not empty; the caller panics first, where Python
/// raises `ValueError: empty separator`.
pub(crate) fn split<'a>(text: &'a str, separator: &str) -> Vec<&'a str> {
    text.split(separator).collect()
}

/// The words of `text`, as Python's `text.split()` with no separator: runs of whitespace
/// ([`is_python_whitespace`]) separate them, and whitespace at either end gives no empty word, so
/// text that is only whitespace has no words.
pub(crate) fn split_whitespace(text: &str) -> Vec<&str> {
    text.split(is_python_whitespace)
        .filter(|word| !word.is_empty())
        .collect()
}

/// Which ends of a string a strip removes characters from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    /// Both ends: `strip`.
    Both,
    /// The start: `lstrip`.
    Start,
    /// The end: `rstrip`.
    End,
}

impl Side {
    /// The side compiled code passes as `code`: 0 for both ends, 1 for the start and 2 for the
    /// end. Any other value strips both ends.
    pub(crate) fn from_code(code: i32) -> Self {
        match code {
            1 => Self::Start,
            2 => Self::End,
            _ => Self::Both,
        }
    }
}

/// `text` with the characters `strip` matches removed from `side`, as Python's `strip`,
/// `lstrip` and `rstrip`.
pub(crate) fn strip(text: &str, side: Side, strip: impl Fn(char) -> bool) -> &str {
    match side {
        Side::Both => text.trim_matches(strip),
        Side::Start => text.trim_start_matches(strip),
        Side::End => text.trim_end_matches(strip),
    }
}

/// The position of the first occurrence of `needle` in `text`, in characters, or -1 when it
/// does not occur, as Python's `text.find(needle)`. The empty string occurs at 0.
pub(crate) fn find(text: &str, needle: &str) -> i64 {
    text.find(needle).map_or(-1, |byte| {
        i64::try_from(text[..byte].chars().count()).unwrap_or(i64::MAX)
    })
}

#[cfg(test)]
mod tests {
    use super::{Side, find, is_python_whitespace, split, split_whitespace, strip, take};

    /// Indexing and slicing count characters, with any step, as Python's do.
    #[test]
    fn take_counts_characters() {
        assert_eq!(take("Ångström", 0, 1, 1), "Å");
        assert_eq!(take("Ångström", 5, 1, 1), "r");
        assert_eq!(take("Ångström", 1, 3, 1), "ngs");
        assert_eq!(take("abcdef", 0, 3, 2), "ace");
        assert_eq!(take("abcdef", 5, 6, -1), "fedcba");
        assert_eq!(take("abc", 0, 0, 1), "");
        assert_eq!(take("abc", 2, 5, 1), "c");
    }

    /// `split(sep)` keeps empty pieces; `split()` drops them and splits on any whitespace run.
    #[test]
    fn split_matches_python() {
        assert_eq!(split("a,,b", ","), ["a", "", "b"]);
        assert_eq!(split("", ","), [""]);
        assert_eq!(split("1.0 :: 2.0", " :: "), ["1.0", "2.0"]);
        assert_eq!(split_whitespace("  1.0\t2.0\n 3 "), ["1.0", "2.0", "3"]);
        assert!(split_whitespace(" \t\n").is_empty());
        assert!(split_whitespace("").is_empty());
        assert_eq!(split_whitespace("a\x1fb\u{3000}c"), ["a", "b", "c"]);
    }

    /// Stripping removes whitespace, or the given characters, from the chosen ends.
    #[test]
    fn strip_matches_python() {
        assert_eq!(strip("  x y \n", Side::Both, is_python_whitespace), "x y");
        assert_eq!(strip("  x ", Side::Start, is_python_whitespace), "x ");
        assert_eq!(strip("  x ", Side::End, is_python_whitespace), "  x");
        assert_eq!(strip("xxaxbxx", Side::Both, |c| "x".contains(c)), "axb");
        assert_eq!(Side::from_code(1), Side::Start);
        assert_eq!(Side::from_code(2), Side::End);
        assert_eq!(Side::from_code(0), Side::Both);
    }

    /// `find` gives a character position, 0 for the empty string and -1 when nothing matches.
    #[test]
    fn find_counts_characters() {
        assert_eq!(find("Ångström", "ö"), 6);
        assert_eq!(find("abc", ""), 0);
        assert_eq!(find("abc", "d"), -1);
        assert_eq!(find("abcabc", "c"), 2);
    }
}
