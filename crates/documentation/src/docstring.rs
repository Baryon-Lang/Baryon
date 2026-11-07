//! Turning the source text of a documentation string into the text it documents with.
//!
//! A documentation string is written in the source as a Python string literal, quotes, prefix,
//! escapes and indentation included. [`text`] undoes all of that, so the generator works with
//! the words the author meant: the literal's value, with the indentation it inherits from the
//! code around it removed, as Python's `inspect.cleandoc` does.

/// The value of `literal`, the source text of a string literal, cleaned for display: the
/// literal's value with the shared indentation of its lines removed and blank lines trimmed
/// from both ends. `None` when the text is not a well-formed string literal, which a tree from a
/// successful parse never holds.
///
/// Several literals written side by side, `"a" "b"`, are one value, as in Python. Escapes are
/// decoded unless the literal is raw (`r"..."`); an escape Python does not define is kept as
/// written, as Python keeps it.
pub(crate) fn text(literal: &str) -> Option<String> {
    Some(clean(&value(literal)?))
}

/// The value of `literal`: every quoted part joined, with escapes decoded. `None` when the
/// text is not one or more string literals separated by whitespace.
fn value(literal: &str) -> Option<String> {
    let mut value = String::new();
    let mut rest = literal.trim_start();
    while !rest.is_empty() {
        let prefix_length = rest
            .find(['"', '\''])
            .filter(|&length| rest[..length].chars().all(|letter| "rRuU".contains(letter)))?;
        let raw = rest[..prefix_length].contains(['r', 'R']);
        rest = &rest[prefix_length..];
        let quote = if rest.starts_with("\"\"\"") || rest.starts_with("'''") {
            &rest[..3]
        } else {
            &rest[..1]
        };
        rest = &rest[quote.len()..];
        let end = closing_quote(rest, quote)?;
        let body = &rest[..end];
        if raw {
            value.push_str(body);
        } else {
            unescape(body, &mut value);
        }
        rest = rest[end + quote.len()..].trim_start();
    }
    Some(value)
}

/// The byte offset in `body` of the `quote` that closes the literal, skipping escaped
/// characters. `None` when the literal is never closed.
fn closing_quote(body: &str, quote: &str) -> Option<usize> {
    let mut characters = body.char_indices();
    while let Some((offset, character)) = characters.next() {
        if character == '\\' {
            characters.next();
        } else if body[offset..].starts_with(quote) {
            return Some(offset);
        }
    }
    None
}

/// Appends `body`, the text between a literal's quotes, to `value` with its escapes decoded.
///
/// Only the escapes that appear in prose are decoded: a backslash before a newline, a
/// backslash, either quote, `n` or `t`. Any other escape, `\x41` or `\N{...}` for instance, is
/// kept as written; a documentation string that needs one is rare enough to show it raw.
fn unescape(body: &str, value: &mut String) {
    let mut characters = body.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            value.push(character);
            continue;
        }
        match characters.next() {
            Some('\n') => {}
            Some('n') => value.push('\n'),
            Some('t') => value.push('\t'),
            Some(escaped @ ('\\' | '"' | '\'')) => value.push(escaped),
            Some(other) => {
                value.push('\\');
                value.push(other);
            }
            None => value.push('\\'),
        }
    }
}

/// `value` with the indentation shared by every line after the first removed, trailing
/// whitespace removed from each line, and blank lines removed from the start and end.
///
/// The first line is skipped when finding the shared indentation because it starts right after
/// the opening quotes, so it has none. This is the rule of Python's `inspect.cleandoc` and
/// PEP 257, so documentation reads the same here as in any Python tool. A tab counts as one
/// column, so a documentation string indented with a mix of tabs and spaces keeps some of it.
fn clean(value: &str) -> String {
    let lines: Vec<&str> = value.lines().map(str::trim_end).collect();
    let indentation = lines
        .iter()
        .skip(1)
        .filter(|line| !line.is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    let cleaned: Vec<&str> = lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            if index == 0 {
                line.trim_start()
            } else {
                line.get(indentation..).unwrap_or_default()
            }
        })
        .collect();
    let first = cleaned.iter().position(|line| !line.is_empty());
    let last = cleaned.iter().rposition(|line| !line.is_empty());
    match (first, last) {
        (Some(first), Some(last)) => cleaned[first..=last].join("\n"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triple_quoted_text_loses_its_indentation() {
        let literal = "\"\"\"Advance the atoms one step.\n\n    Uses velocity Verlet.\n      \
                       Indented more.\n    \"\"\"";
        assert_eq!(
            text(literal).unwrap(),
            "Advance the atoms one step.\n\nUses velocity Verlet.\n  Indented more.",
        );
    }

    #[test]
    fn a_text_starting_on_the_second_line_keeps_no_blank_line() {
        let literal = "'''\n    Kinetic energy.\n    '''";
        assert_eq!(text(literal).unwrap(), "Kinetic energy.");
    }

    #[test]
    fn single_quotes_and_escapes_are_decoded() {
        assert_eq!(
            text(r#""Say \"hi\"\tthen\nleave.""#).unwrap(),
            "Say \"hi\"\tthen\nleave."
        );
        assert_eq!(text(r"'it\'s \\ fine'").unwrap(), r"it's \ fine");
        assert_eq!(text(r#""\x41 stays""#).unwrap(), r"\x41 stays");
    }

    #[test]
    fn raw_literals_keep_backslashes() {
        assert_eq!(
            text(r#"r"Solves \(Ax = b\).""#).unwrap(),
            r"Solves \(Ax = b\)."
        );
    }

    #[test]
    fn literals_side_by_side_are_one_value() {
        assert_eq!(
            text("\"First, \"\n    'then second.'").unwrap(),
            "First, then second."
        );
    }

    #[test]
    fn a_line_continuation_joins_lines() {
        assert_eq!(text("\"one \\\ntwo\"").unwrap(), "one two");
    }

    #[test]
    fn a_quote_of_the_other_kind_does_not_close_the_literal() {
        assert_eq!(
            text("\"\"\"Uses \"quotes\" and '''.\"\"\"").unwrap(),
            "Uses \"quotes\" and '''."
        );
    }

    #[test]
    fn malformed_text_is_not_a_literal() {
        assert_eq!(text("\"never closed"), None);
        assert_eq!(text("f\"formatted\""), None);
        assert_eq!(text("name"), None);
    }

    #[test]
    fn an_empty_literal_is_empty_text() {
        assert_eq!(text("\"\"\"\n    \n\"\"\"").unwrap(), "");
    }
}
