//! `cargo xtask check-conflicts`: no file may hold a merge-conflict marker left behind by an
//! unfinished merge.
//!
//! When `git` cannot merge two changes to the same lines, it writes both versions into the file
//! between marker lines and leaves the person merging to choose. A marker that survives the merge
//! is easy to miss in review, and in Markdown no other check notices it: a merge into #76 left
//! markers in `README.md` and every other check passed.
//!
//! A marker is a line that starts with seven `<`, `|` or `>` characters followed by a space or
//! the end of the line, or a line of exactly seven `=`. These are the shapes `git` writes, in both
//! its default and its `diff3` conflict styles. The check reads every file `git` tracks plus new
//! files it does not ignore, so it also catches a marker before the file is committed. Files that
//! are not UTF-8 text, such as images, are skipped.

use crate::{Outcome, workspace_root};
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

/// The characters `git` repeats to open (`<`), separate the common ancestor (`|`) and close
/// (`>`) a conflict. Each marker is the character seven times, then a space and a label.
const LABELLED_MARKERS: [char; 3] = ['<', '|', '>'];

/// The character of the marker between the two versions: seven of it, alone on its line.
const SEPARATOR: char = '=';

/// How many times a marker repeats its character.
const MARKER_LENGTH: usize = 7;

/// Checks every tracked and new file and reports each line that holds a conflict marker.
pub(crate) fn check() -> Outcome {
    let root = workspace_root();
    let files = files_to_check(&root)?;

    let mut found = 0;
    for file in &files {
        let text = match fs::read_to_string(root.join(file)) {
            Ok(text) => text,
            // A tracked file deleted in the working tree has nothing to check, and a file that
            // is not UTF-8 is not text a merge would write markers into.
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::InvalidData
                ) =>
            {
                continue;
            }
            Err(error) => return Err(format!("could not read `{file}`: {error}").into()),
        };
        for line in marker_lines(&text) {
            eprintln!("conflicts: {file}:{line}: leftover merge-conflict marker");
            found += 1;
        }
    }
    if found == 0 {
        println!(
            "conflicts: no merge-conflict markers in {} files",
            files.len()
        );
        Ok(())
    } else {
        Err(format!("{found} merge-conflict marker(s); finish the merge").into())
    }
}

/// Every file `git` tracks plus every new file it does not ignore, relative to `root`.
///
/// # Errors
///
/// Fails if `git` cannot run or does not succeed, for example outside a repository.
fn files_to_check(root: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(root)
        .output()
        .map_err(|error| format!("could not run `git`, which lists the files: {error}"))?;
    if !output.status.success() {
        return Err("`git ls-files` failed".into());
    }
    let mut files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .split('\0')
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}

/// The 1-based numbers of the lines in `text` that are conflict markers.
fn marker_lines(text: &str) -> Vec<usize> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| is_marker(line.trim_end_matches('\r')))
        .map(|(index, _)| index + 1)
        .collect()
}

/// Whether `line` is a conflict marker in one of the shapes `git` writes.
fn is_marker(line: &str) -> bool {
    let separator = SEPARATOR.to_string().repeat(MARKER_LENGTH);
    line == separator
        || LABELLED_MARKERS.iter().any(|&character| {
            line.strip_prefix(&character.to_string().repeat(MARKER_LENGTH))
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A marker built at run time, so this file never holds one and passes its own check.
    fn marker(character: char, label: &str) -> String {
        format!("{}{label}", character.to_string().repeat(MARKER_LENGTH))
    }

    #[test]
    fn finds_every_marker_of_a_diff3_conflict() {
        let text = [
            "# Title",
            &marker('<', " HEAD"),
            "ours",
            &marker('|', " merged common ancestors"),
            "base",
            &marker('=', ""),
            "theirs",
            &marker('>', " origin/main"),
            "after",
        ]
        .join("\n");
        assert_eq!(marker_lines(&text), [2, 4, 6, 8]);
    }

    #[test]
    fn finds_markers_without_a_label_and_with_windows_line_ends() {
        let text = format!("{}\r\nours\r\n{}\r\n", marker('<', ""), marker('>', ""));
        assert_eq!(marker_lines(&text), [1, 3]);
    }

    #[test]
    fn ignores_lines_that_only_look_like_markers() {
        let text = [
            // A Markdown heading underline is usually shorter or longer than seven.
            "Heading",
            "===",
            &marker('=', "="),
            // Eight characters, or seven with text right after them, are not markers.
            &marker('<', "<"),
            &marker('>', ">text"),
            // A marker indented or inside a line is text, not a conflict.
            &format!("    {}", marker('<', " HEAD")),
            &format!("see {}", marker('>', " main")),
        ]
        .join("\n");
        assert!(marker_lines(&text).is_empty());
    }
}
