//! The last steps of [`Ssa::finish`](super::Ssa::finish): removing each `phi` that turned out to
//! be one value, and writing the remaining `phi`s and the values into the function's text.

use super::{MARKER, PHI_PREFIX, Phi, Value, phi_name};
use std::collections::{BTreeMap, HashMap};

/// For each of `phis`, by number, the value it stands for when it is one value, or `None`
/// when it stays a `phi`: a `phi` whose operands, once each replaced `phi` is followed, are all
/// one value or itself. Repeats until no `phi` changes, since replacing one can make another
/// one value.
pub(super) fn trivial(phis: &[Phi<'_>]) -> Vec<Option<String>> {
    let mut replaced: Vec<Option<Value>> = vec![None; phis.len()];
    let follow = |replaced: &[Option<Value>], value: &Value| -> Value {
        let mut value = value.clone();
        while let Value::Phi(number) = value {
            match &replaced[number] {
                Some(next) => value = next.clone(),
                None => break,
            }
        }
        value
    };
    loop {
        let mut changed = false;
        for (number, phi) in phis.iter().enumerate().rev() {
            if replaced[number].is_some() {
                continue;
            }
            let mut only: Option<Value> = None;
            let mut several = false;
            for (value, _) in &phi.incoming {
                let value = follow(&replaced, value);
                if value == Value::Phi(number) {
                    continue;
                }
                match &only {
                    None => only = Some(value),
                    Some(existing) if *existing == value => {}
                    Some(_) => {
                        several = true;
                        break;
                    }
                }
            }
            if let (false, Some(value)) = (several, only) {
                replaced[number] = Some(value);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    (0..phis.len())
        .map(|number| {
            replaced[number]
                .as_ref()
                .map(|_| match follow(&replaced, &Value::Phi(number)) {
                    Value::Phi(other) => phi_name(phis[other].local, phis[other].block),
                    Value::Text(text) => text,
                })
        })
        .collect()
}

/// `code` with each block's marker replaced by its `phi`s in `phis`, and the name of each `phi`
/// that turned out to be one value replaced by that value, as `names` says.
///
/// Scans `code` once for each pattern, keeping the next place each occurs, rather than line by
/// line: the function's text is most of what the backend writes, and markers and `phi` names
/// are rare in it.
pub(super) fn rewrite(
    code: &str,
    phis: &BTreeMap<usize, String>,
    names: &HashMap<String, String>,
) -> String {
    // Each pattern is found by its one rare character, `;` or `$`, which a byte search finds
    // far faster than a search for the whole pattern, then checked whole.
    let next = |pattern: &str, from: usize| {
        let key = pattern.find([';', '$'])?;
        let character = char::from(pattern.as_bytes()[key]);
        let mut start = from + key;
        loop {
            let at = start + code.get(start..)?.find(character)?;
            // A byte before the key may belong to a longer character, so the slice is taken
            // with `get`, which gives nothing there rather than panicking.
            if at - key >= from
                && code
                    .get(at - key..)
                    .is_some_and(|rest| rest.starts_with(pattern))
            {
                return Some(at - key);
            }
            start = at + 1;
        }
    };
    let mut text = String::with_capacity(code.len());
    let mut done = 0;
    let mut marker = next(MARKER, 0);
    let mut name = next(PHI_PREFIX, 0);
    loop {
        let position = match (marker, name) {
            (None, None) => break,
            (Some(marker), Some(name)) => marker.min(name),
            (Some(position), None) | (None, Some(position)) => position,
        };
        text.push_str(&code[done..position]);
        if marker == Some(position) {
            // A marker line is `MARKER`, the block's number and a line end.
            let after = &code[position + MARKER.len()..];
            let end = after.find('\n').map_or(after.len(), |end| end + 1);
            let block_phis = after[..end]
                .trim_end()
                .parse()
                .ok()
                .and_then(|block: usize| phis.get(&block));
            if let Some(block_phis) = block_phis {
                text.push_str(block_phis);
            }
            done = position + MARKER.len() + end;
            marker = next(MARKER, done);
        } else {
            let rest = &code[position..];
            let end = rest[PHI_PREFIX.len()..]
                .find(|character: char| !(character.is_ascii_digit() || character == '.'))
                .map_or(rest.len(), |end| end + PHI_PREFIX.len());
            let found = &rest[..end];
            text.push_str(names.get(found).map_or(found, String::as_str));
            done = position + end;
            name = next(PHI_PREFIX, done);
        }
    }
    text.push_str(&code[done..]);
    text
}

#[cfg(test)]
mod tests {
    use super::rewrite;
    use std::collections::{BTreeMap, HashMap};

    /// A `$` whose fourth byte back is inside a character, as after `€€` in a text constant, is
    /// not the end of a `phi` name's prefix and is left as it is, while a real name is still
    /// replaced.
    #[test]
    fn a_dollar_after_a_wide_character_is_not_a_phi_name() {
        let code = "  %t0 = add i64 %ssa$1.2, 1 ; \u{20ac}\u{20ac}$ \u{3c8}$x\n";
        let names = HashMap::from([("%ssa$1.2".to_owned(), "%a1".to_owned())]);
        assert_eq!(
            rewrite(code, &BTreeMap::new(), &names),
            "  %t0 = add i64 %a1, 1 ; \u{20ac}\u{20ac}$ \u{3c8}$x\n"
        );
    }
}
