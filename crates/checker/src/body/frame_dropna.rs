//! The arguments of `df.dropna()` (decision 0045, part 3, roadmap 3.13.6): `how=`, which
//! says whether one `None` drops a row or only a row of nothing but `None`, and `subset=`,
//! the columns looked at. Both are written as literals, since the result's type depends on
//! them.

use super::Body;
use super::calls::Arguments;
use syntax::{NodeId, SyntaxKind};
use types::{Column, Type};

impl<'db> Body<'db> {
    /// Which of `columns`, those of the frame of type `base`, hold no `None` after the call
    /// `function` of `dropna` with `arguments`. `subset=` is one column's name or a list of
    /// names, every column when left out. With `how="any"`, the default, a row with a
    /// `None` in any of them is dropped, so each of them holds no `None` afterwards. With
    /// `how="all"` only a row with `None` in all of them is, so no column changes its type.
    /// `None` after an error.
    pub(super) fn dropped_columns(
        &mut self,
        function: NodeId,
        base: Type<'db>,
        columns: &[Column<'db>],
        arguments: &Arguments,
    ) -> Option<Vec<bool>> {
        if !arguments.positional.is_empty() {
            self.untyped(arguments);
            self.lowerer.error(
                function,
                "`dropna` of a `DataFrame` takes `how=` and `subset=`, each by name",
                "wrong arguments",
                Some("for example `df.dropna(subset=[\"bid\", \"ask\"])`"),
            );
            return None;
        }
        let (mut all, mut subset) = (false, None);
        let mut complete = true;
        for &(name, value) in &arguments.keywords {
            match self.tree.text(name, self.text) {
                "how" => match self.drop_rule(value) {
                    Some(rule) => all = rule,
                    None => complete = false,
                },
                "subset" => {
                    let names = if self.tree.kind(value) == SyntaxKind::List {
                        self.column_names(value, base)
                    } else {
                        self.column_name(value, base).map(|name| vec![name])
                    };
                    complete &= names.is_some();
                    subset = names;
                }
                other => {
                    let message = format!("`dropna` of a `DataFrame` has no argument `{other}`");
                    self.expression(value, None);
                    self.lowerer.error(
                        name,
                        &message,
                        "not an argument that is built",
                        Some("`how=` and `subset=` are built; `thresh=` and `axis=` are not"),
                    );
                    complete = false;
                }
            }
        }
        complete.then(|| {
            columns
                .iter()
                .map(|column| {
                    !all && subset
                        .as_ref()
                        .is_none_or(|names| names.contains(&column.name))
                })
                .collect()
        })
    }

    /// Whether `how`, the value of `how=`, is `"all"`, when it is the literal `"any"` or
    /// `"all"`. `None`, after an error, for anything else.
    fn drop_rule(&mut self, how: NodeId) -> Option<bool> {
        match self.literal_text(how) {
            Some("any") => return Some(false),
            Some("all") => return Some(true),
            _ => {}
        }
        self.expression(how, None);
        self.lowerer.error(
            how,
            "`how` is `\"any\"` or `\"all\"`, written as a string literal",
            "not a rule for dropping rows",
            Some("the result's type depends on it, so it must be known when compiling"),
        );
        None
    }
}
