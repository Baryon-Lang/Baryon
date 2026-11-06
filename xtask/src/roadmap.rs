//! `cargo xtask check-roadmap`: keeps `ROADMAP.md`, the project's source of truth, valid and
//! current.
//!
//! It runs two checks:
//!
//! 1. **Every row is well formed.** Each item has a unique id, a known status, an owner once
//!    work starts, a link once it is done, and dependencies that exist.
//! 2. **Every change updates the roadmap.** When the working tree differs from where it branched
//!    off `origin/main`, `ROADMAP.md` must be among the changed files. Contributors mark items
//!    `In progress` and `Done` as they work, so the roadmap never falls behind the code.

use crate::{Outcome, workspace_root};
use std::collections::HashSet;
use std::fs;
use std::process::Command;

/// The roadmap's path, relative to the repository root.
const ROADMAP: &str = "ROADMAP.md";

/// The branch every change is compared against.
const BASE: &str = "origin/main";

/// The header every work-item table uses. Rows of tables with any other header are ignored.
const HEADER: [&str; 7] = [
    "ID",
    "Item",
    "Weeks",
    "Depends on",
    "Status",
    "Owner",
    "Link",
];

/// What an empty cell holds. Writing it explicitly tells readers the cell was not forgotten.
const EMPTY: &str = "—";

/// Where an item stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    /// Proposed; not yet accepted into a phase.
    Idea,
    /// Accepted and waiting for an owner.
    Planned,
    /// Someone is working on it.
    InProgress,
    /// Started, but waiting on something named in the description.
    Blocked,
    /// Finished and merged.
    Done,
    /// Decided against.
    Dropped,
}

impl Status {
    /// The status as written in the roadmap, or `None` if the text is not a status.
    fn parse(text: &str) -> Option<Self> {
        match text {
            "Idea" => Some(Self::Idea),
            "Planned" => Some(Self::Planned),
            "In progress" => Some(Self::InProgress),
            "Blocked" => Some(Self::Blocked),
            "Done" => Some(Self::Done),
            "Dropped" => Some(Self::Dropped),
            _ => None,
        }
    }

    /// Whether an item in this status must name its owner.
    fn needs_owner(self) -> bool {
        matches!(self, Self::InProgress | Self::Blocked | Self::Done)
    }
}

/// Checks the roadmap's rows, then checks that the current change updates it.
pub(crate) fn check() -> Outcome {
    let root = workspace_root();
    let text = fs::read_to_string(root.join(ROADMAP))?;
    let problems = problems(&text);
    for problem in &problems {
        eprintln!("roadmap: {ROADMAP}:{problem}");
    }
    if !problems.is_empty() {
        return Err(format!("{} roadmap problem(s)", problems.len()).into());
    }

    match changed_files()? {
        None => println!("roadmap: `{BASE}` not found; skipped the check that {ROADMAP} changed"),
        Some(files) if files.is_empty() || files.iter().any(|file| file == ROADMAP) => {}
        Some(files) => {
            return Err(format!(
                "this change touches {} file(s) but not {ROADMAP}; mark the item you are \
                 working on as described in its \"How to use it\" section",
                files.len()
            )
            .into());
        }
    }
    println!("roadmap: every item is well formed and the roadmap is current");
    Ok(())
}

/// Every problem in the roadmap's work-item tables, in line order, each prefixed with its
/// 1-based line number.
fn problems(text: &str) -> Vec<String> {
    let rows = item_rows(text);
    let mut problems: Vec<(usize, String)> = Vec::new();
    let mut ids = HashSet::new();
    for (line, cells) in &rows {
        if cells.len() != HEADER.len() {
            problems.push((
                *line,
                format!("row has {} cells; expected {}", cells.len(), HEADER.len()),
            ));
        } else if !ids.insert(cells[0]) {
            problems.push((*line, format!("id `{}` is used more than once", cells[0])));
        }
    }
    for (line, cells) in rows.iter().filter(|(_, cells)| cells.len() == HEADER.len()) {
        let [id, _, _, depends_on, status, owner, link] = cells[..] else {
            continue;
        };
        if let Some(column) = cells.iter().position(|cell| cell.is_empty()) {
            problems.push((
                *line,
                format!(
                    "item `{id}` has an empty {} cell; write `{EMPTY}`",
                    HEADER[column]
                ),
            ));
        }
        match Status::parse(status) {
            None => problems.push((*line, format!("item `{id}` has unknown status `{status}`"))),
            Some(status) => {
                if status.needs_owner() && owner == EMPTY {
                    problems.push((*line, format!("item `{id}` has started but has no owner")));
                }
                if status == Status::Done && link == EMPTY {
                    problems.push((
                        *line,
                        format!("item `{id}` is done but links to no pull request"),
                    ));
                }
            }
        }
        let dependencies = depends_on.split(',').map(str::trim);
        for dependency in dependencies.filter(|&dependency| dependency != EMPTY) {
            if !ids.contains(dependency) {
                problems.push((
                    *line,
                    format!("item `{id}` depends on unknown item `{dependency}`"),
                ));
            }
        }
    }
    problems.sort_by_key(|&(line, _)| line);
    problems
        .into_iter()
        .map(|(line, message)| format!("{line}: {message}"))
        .collect()
}

/// The cells of every row in a table whose header is [`HEADER`], with 1-based line numbers.
fn item_rows(text: &str) -> Vec<(usize, Vec<&str>)> {
    let mut rows = Vec::new();
    let mut in_item_table = false;
    let mut lines = text.lines().enumerate();
    while let Some((index, line)) = lines.next() {
        let Some(cells) = table_cells(line) else {
            in_item_table = false;
            continue;
        };
        if cells == HEADER {
            in_item_table = true;
            lines.next();
        } else if in_item_table {
            rows.push((index + 1, cells));
        }
    }
    rows
}

/// The trimmed cells of a Markdown table row, or `None` if the line is not one.
fn table_cells(line: &str) -> Option<Vec<&str>> {
    let inner = line.trim().strip_prefix('|')?.strip_suffix('|')?;
    Some(inner.split('|').map(str::trim).collect())
}

/// Files changed since the working tree branched off [`BASE`], or `None` if [`BASE`] does not
/// exist locally. Uncommitted changes and new, untracked files count.
fn changed_files() -> Result<Option<Vec<String>>, Box<dyn std::error::Error>> {
    let Some(merge_base) = git(&["merge-base", BASE, "HEAD"])? else {
        return Ok(None);
    };
    let changed = git(&["diff", "--name-only", merge_base.trim()])?
        .ok_or("`git diff` failed against the merge base")?;
    let untracked =
        git(&["ls-files", "--others", "--exclude-standard"])?.ok_or("`git ls-files` failed")?;
    Ok(Some(
        changed
            .lines()
            .chain(untracked.lines())
            .map(str::to_owned)
            .collect(),
    ))
}

/// Runs `git` in the repository root and returns its output, or `None` if it fails.
fn git(arguments: &[&str]) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(workspace_root())
        .output()?;
    Ok(output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wraps item rows in a table with the work-item header.
    fn table(rows: &str) -> String {
        format!(
            "# Roadmap\n\n| ID | Item | Weeks | Depends on | Status | Owner | Link |\n\
             | --- | --- | --- | --- | --- | --- | --- |\n{rows}"
        )
    }

    #[test]
    fn accepts_well_formed_items() {
        let text = table(
            "| 0.1 | Spec | 3 | — | Done | Collins | [#1](https://example.com/1) |\n\
             | 1.1 | Lexer | 1 | 0.1 | In progress | Claude | — |\n\
             | I.1 | Idea | — | 0.1, 1.1 | Idea | — | — |\n",
        );
        assert_eq!(problems(&text), Vec::<String>::new());
    }

    #[test]
    fn ignores_tables_with_other_headers() {
        let text = "| Status | Meaning |\n| --- | --- |\n| `Idea` | Proposed |\n";
        assert_eq!(problems(text), Vec::<String>::new());
    }

    #[test]
    fn reports_unknown_status_and_duplicate_ids() {
        let text = table(
            "| 1.1 | Lexer | 1 | — | Started | — | — |\n\
             | 1.1 | Parser | 3 | — | Planned | — | — |\n",
        );
        assert_eq!(
            problems(&text),
            [
                "5: item `1.1` has unknown status `Started`",
                "6: id `1.1` is used more than once",
            ]
        );
    }

    #[test]
    fn requires_owner_once_started_and_link_once_done() {
        let text = table(
            "| 1.1 | Lexer | 1 | — | In progress | — | — |\n\
             | 1.2 | Parser | 3 | — | Done | Claude | — |\n",
        );
        assert_eq!(
            problems(&text),
            [
                "5: item `1.1` has started but has no owner",
                "6: item `1.2` is done but links to no pull request",
            ]
        );
    }

    #[test]
    fn reports_empty_cells_and_unknown_dependencies() {
        let text = table("| 1.1 | Lexer |  | 0.9 | Planned | — | — |\n");
        assert_eq!(
            problems(&text),
            [
                "5: item `1.1` has an empty Weeks cell; write `—`",
                "5: item `1.1` depends on unknown item `0.9`",
            ]
        );
    }

    #[test]
    fn reports_rows_with_the_wrong_number_of_cells() {
        let text = table("| 1.1 | Lexer | Planned |\n");
        assert_eq!(problems(&text), ["5: row has 3 cells; expected 7"]);
    }
}
