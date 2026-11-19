//! The options of `drop_duplicates` and `value_counts` (decision 0045, roadmap 3.13.16):
//! `keep=`, which of the rows that repeat `drop_duplicates` keeps, and `normalize=`, `sort=`,
//! `ascending=` and `dropna=` of `value_counts`, with pandas's names and meanings.
//!
//! Each is written out: `keep` as `"first"`, `"last"` or `False`, and the others as `True`
//! or `False`. `normalize` changes the result's last column from `count` to `proportion`,
//! so the frame's type depends on it, and the others choose which steps the compiler
//! builds.

use super::Body;
use super::calls::Arguments;
use syntax::{NodeId, SyntaxKind};

impl Body<'_> {
    /// The arguments of the method `callee`, `drop_duplicates` or `value_counts`: the
    /// columns to compare, by position or as `subset=`, if they are written, and whether
    /// `normalize=True` is. The second is `None` after an option written wrongly, which is
    /// reported here. `None`, with nothing reported, when the call has an argument the
    /// method does not take, or one twice.
    pub(super) fn distinct_arguments(
        &mut self,
        callee: &str,
        arguments: &Arguments,
    ) -> Option<(Option<NodeId>, Option<bool>)> {
        let mut subset = match arguments.positional.as_slice() {
            [] => None,
            &[subset] => Some(subset),
            _ => return None,
        };
        let allowed: &[&str] = if callee == "value_counts" {
            &["normalize", "sort", "ascending", "dropna"]
        } else {
            &["keep"]
        };
        let mut options: Vec<(String, NodeId)> = Vec::new();
        for &(keyword, value) in &arguments.keywords {
            let name = self.tree.text(keyword, self.text).to_owned();
            if name == "subset" && subset.is_none() {
                subset = Some(value);
            } else if allowed.contains(&name.as_str())
                && options.iter().all(|(option, _)| *option != name)
            {
                options.push((name, value));
            } else {
                return None;
            }
        }
        let (mut right, mut normalize) = (true, false);
        for (name, value) in options {
            if name == "keep" {
                right &= self.kept_rows(value);
            } else if let Some(flag) = self.written_flag(&name, value) {
                normalize |= flag && name == "normalize";
            } else {
                right = false;
            }
        }
        Some((subset, right.then_some(normalize)))
    }

    /// Whether `keep`, the `keep=` of a `drop_duplicates`, is one of the three pandas takes:
    /// `"first"`, `"last"` or `False`. Anything else is reported.
    fn kept_rows(&mut self, keep: NodeId) -> bool {
        if self.tree.kind(keep) == SyntaxKind::False
            || matches!(self.literal_text(keep), Some("first" | "last"))
        {
            return true;
        }
        self.expression(keep, None);
        self.lowerer.error(
            keep,
            "`keep` is `\"first\"`, `\"last\"` or `False`, written out",
            "not a way to keep rows",
            Some(
                "`\"first\"` keeps the first of the rows that repeat, `\"last\"` the last, and \
                 `False` none of them",
            ),
        );
        false
    }

    /// The `bool` that `value`, the value of the option `name`, writes out: `True` or
    /// `False`. `None`, after reporting it, for anything else.
    pub(super) fn written_flag(&mut self, name: &str, value: NodeId) -> Option<bool> {
        match self.tree.kind(value) {
            SyntaxKind::True => Some(true),
            SyntaxKind::False => Some(false),
            _ => {
                self.expression(value, None);
                let message = format!("`{name}` is `True` or `False`, written out");
                self.lowerer.error(
                    value,
                    &message,
                    "not `True` or `False`",
                    Some(
                        "what the compiler builds depends on it, so it must be known when \
                         compiling",
                    ),
                );
                None
            }
        }
    }
}
