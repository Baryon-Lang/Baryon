//! Writing documentation as Markdown pages.
//!
//! Markdown because it renders on GitHub, in editors and in every static site generator, and
//! because documentation strings are already written in it. The page layout is described on
//! [`ModuleDocumentation::markdown`] and [`index`](crate::index).

use crate::{Item, ModuleDocumentation};
use std::fmt::Write as _;

/// Appends the page of `module` to `page`.
pub(crate) fn module_page(module: &ModuleDocumentation, page: &mut String) {
    let _ = writeln!(page, "# `{}`", module.name);
    paragraph(module.text.as_deref(), page);
    for item in &module.items {
        item_section(item, "##", "", page);
        for member in &item.members {
            item_section(member, "###", &item.name, page);
        }
    }
}

/// Appends the section of one item to `page`: a heading at `level` naming it, qualified by
/// `owner` for a class member, its signature as a code block, then its documentation.
fn item_section(item: &Item, level: &str, owner: &str, page: &mut String) {
    let dot = if owner.is_empty() { "" } else { "." };
    let _ = writeln!(page, "\n{level} `{owner}{dot}{}`\n", item.name);
    let fence = fence_for(&item.signature);
    let _ = writeln!(page, "{fence}python\n{}\n{fence}", item.signature);
    paragraph(item.text.as_deref(), page);
}

/// Appends `text`, after a blank line, to `page` if there is any.
fn paragraph(text: Option<&str>, page: &mut String) {
    if let Some(text) = text.filter(|text| !text.is_empty()) {
        let _ = writeln!(page, "\n{text}");
    }
}

/// A code fence that `code` cannot close: three backticks, or one more than the longest run of
/// backticks in `code`.
fn fence_for(code: &str) -> String {
    let longest = code
        .split(|character| character != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    "`".repeat(longest.max(2) + 1)
}

/// Appends the package's index page to `page`: a table with one row per module, linking its
/// page and giving the first line of its documentation.
pub(crate) fn index_page(modules: &[ModuleDocumentation], page: &mut String) {
    page.push_str("# Modules\n\n| Module | Summary |\n| --- | --- |\n");
    for module in modules {
        let summary = module
            .summary()
            .map_or_else(|| "—".to_owned(), |line| line.replace('|', "\\|"));
        let _ = writeln!(
            page,
            "| [`{}`]({}) | {summary} |",
            module.name,
            module.file_name()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fences_are_longer_than_any_backtick_run_in_the_code() {
        assert_eq!(fence_for("def f() -> None"), "```");
        assert_eq!(fence_for("NOTE: Final = \"````\""), "`````");
    }
}
