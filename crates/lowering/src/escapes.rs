//! The text a string literal writes: its quotes taken off and its backslash escapes read as
//! Python reads them, so `"a\tb"` is `a`, a tab, `b`. Lowering stores that text in a
//! [`ConstantValue::Str`](mir::ConstantValue::Str) or an f-string's literal piece.
//!
//! Read: `\\`, `\'`, `\"`, `\n`, `\t`, `\r`, `\a`, `\b`, `\f`, `\v`, octal `\ooo`, `\xhh`,
//! `\uXXXX`, `\UXXXXXXXX`, and a backslash before a line end, which joins the lines. As in
//! Python, a backslash before any other character stays as written: `"\d"` is `\d`. A raw
//! string, `r"..."`, keeps every backslash. Not read yet: `\N{name}`, which needs the Unicode
//! name table, bytes `b"..."`, and literals written in several parts, `"a" "b"`. Each gives
//! `None`, which lowering reports as not lowered.

/// The text of the string literal `written`, as the source spells it: an optional `r` or `u`
/// prefix, one or three quotes, the body, the same quotes. `None` for what this slice does not
/// read (see the module documentation).
pub(crate) fn string(written: &str) -> Option<String> {
    let (raw, rest) = match written.chars().next()? {
        'r' | 'R' => (true, &written[1..]),
        'u' | 'U' => (false, &written[1..]),
        _ => (false, written),
    };
    let quote = rest
        .chars()
        .next()
        .filter(|&quote| quote == '"' || quote == '\'')?;
    let triple: String = [quote; 3].iter().collect();
    let delimiter = if rest.len() >= 6 && rest.starts_with(&triple) {
        triple.as_str()
    } else {
        &rest[..1]
    };
    let body = rest.strip_prefix(delimiter)?.strip_suffix(delimiter)?;
    decode(body, raw, Some(delimiter), false)
}

/// The text of a piece of literal text in an f-string: its escapes read unless the f-string is
/// `raw`, and `{{` and `}}` read as one brace. `None` for `\N{name}`.
pub(crate) fn formatted(written: &str, raw: bool) -> Option<String> {
    decode(written, raw, None, true)
}

/// Reads `body`. A `delimiter` that appears unescaped inside means the literal has several
/// parts, `"a" "b"`, and gives `None`. With `braces`, `{{` and `}}` are one brace each.
fn decode(body: &str, raw: bool, delimiter: Option<&str>, braces: bool) -> Option<String> {
    let mut text = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(next) = rest.chars().next() {
        if delimiter.is_some_and(|delimiter| rest.starts_with(delimiter)) {
            return None;
        }
        if braces && (rest.starts_with("{{") || rest.starts_with("}}")) {
            text.push(next);
            rest = &rest[2..];
            continue;
        }
        rest = &rest[next.len_utf8()..];
        if next != '\\' {
            text.push(next);
            continue;
        }
        let escaped = rest.chars().next()?;
        rest = &rest[escaped.len_utf8()..];
        if raw {
            // A raw string keeps the backslash, and the character after it never ends the
            // string, so `r"\""` is a backslash and a quote.
            text.push('\\');
            text.push(escaped);
            continue;
        }
        match escaped {
            '\n' => {}
            '\r' => rest = rest.strip_prefix('\n').unwrap_or(rest),
            '\\' | '\'' | '"' => text.push(escaped),
            'a' => text.push('\u{7}'),
            'b' => text.push('\u{8}'),
            'f' => text.push('\u{c}'),
            'n' => text.push('\n'),
            'r' => text.push('\r'),
            't' => text.push('\t'),
            'v' => text.push('\u{b}'),
            '0'..='7' => {
                let digits = 1 + rest
                    .chars()
                    .take(2)
                    .take_while(|digit| ('0'..='7').contains(digit))
                    .count();
                let start = body.len() - rest.len() - 1;
                let value = u32::from_str_radix(&body[start..start + digits], 8).ok()?;
                text.push(char::from_u32(value)?);
                rest = &rest[digits - 1..];
            }
            'x' | 'u' | 'U' => {
                let digits = match escaped {
                    'x' => 2,
                    'u' => 4,
                    _ => 8,
                };
                let hex = rest
                    .get(..digits)
                    .filter(|hex| hex.chars().all(|digit| digit.is_ascii_hexdigit()))?;
                let value = u32::from_str_radix(hex, 16).ok()?;
                text.push(char::from_u32(value)?);
                rest = &rest[digits..];
            }
            'N' => return None,
            _ => {
                text.push('\\');
                text.push(escaped);
            }
        }
    }
    Some(text)
}
