//! `cargo xtask check-links`: every relative link in the repository's Markdown files must point
//! at a file or directory that exists.
//!
//! Broken links are the most common way documentation rots, so CI catches them. Links with a
//! URL scheme (`https://`, `mailto:`) are not fetched: CI must not depend on the network.
//! Anchors after `#` are not checked; only the file part of a link is.

use crate::{Outcome, workspace_root};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Directories never scanned: version-control data and build output.
const SKIPPED_DIRECTORIES: [&str; 2] = [".git", "target"];

/// A link in a Markdown file, with where it was found.
#[derive(Debug, PartialEq, Eq)]
struct Link<'a> {
    /// The 1-based line the link is on.
    line: usize,
    /// The link target as written, without any `#anchor`.
    target: &'a str,
}

/// Checks every Markdown file in the repository and reports each broken link.
pub(crate) fn check() -> Outcome {
    let root = workspace_root();
    let mut files = Vec::new();
    collect_markdown(&root, &mut files)?;
    files.sort();

    let mut broken = 0;
    for file in &files {
        let text = fs::read_to_string(file)?;
        let directory = file.parent().unwrap_or(&root);
        for link in relative_links(&text) {
            if !directory.join(link.target).exists() {
                let shown = file.strip_prefix(&root).unwrap_or(file).display();
                eprintln!(
                    "links: {shown}:{}: `{}` does not exist",
                    link.line, link.target
                );
                broken += 1;
            }
        }
    }
    if broken == 0 {
        println!(
            "links: every relative link in {} Markdown files resolves",
            files.len()
        );
        Ok(())
    } else {
        Err(format!("{broken} broken documentation link(s)").into())
    }
}

/// Adds every `.md` file under `directory` to `files`, skipping [`SKIPPED_DIRECTORIES`].
fn collect_markdown(directory: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            let skipped = SKIPPED_DIRECTORIES
                .iter()
                .any(|name| entry.file_name() == *name);
            if !skipped {
                collect_markdown(&path, files)?;
            }
        } else if path.extension().is_some_and(|extension| extension == "md") {
            files.push(path);
        }
    }
    Ok(())
}

/// Every relative inline link `[text](target)` outside code blocks and inline code.
fn relative_links(text: &str) -> Vec<Link<'_>> {
    let mut links = Vec::new();
    let mut in_code_block = false;
    for (index, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }
        // Splitting on backticks puts inline code at odd positions; only prose is scanned.
        for prose in line.split('`').step_by(2) {
            let mut rest = prose;
            while let Some(start) = rest.find("](") {
                let after = &rest[start + 2..];
                let Some(end) = after.find(')') else {
                    break;
                };
                let target = after[..end].split_whitespace().next().unwrap_or_default();
                let file_part = target.split('#').next().unwrap_or_default();
                if is_relative(file_part) {
                    links.push(Link {
                        line: index + 1,
                        target: file_part,
                    });
                }
                rest = &after[end + 1..];
            }
        }
    }
    links
}

/// Whether a link target names a local file, rather than a URL or nothing at all.
fn is_relative(target: &str) -> bool {
    !target.is_empty() && !target.contains("://") && !target.starts_with("mailto:")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn targets(text: &str) -> Vec<&str> {
        relative_links(text)
            .into_iter()
            .map(|link| link.target)
            .collect()
    }

    #[test]
    fn finds_relative_links_and_their_lines() {
        let links = relative_links("Intro\nSee [rules](CONTRIBUTING.md) and [plan](docs/plan.md).");
        assert_eq!(
            links,
            [
                Link {
                    line: 2,
                    target: "CONTRIBUTING.md"
                },
                Link {
                    line: 2,
                    target: "docs/plan.md"
                },
            ]
        );
    }

    #[test]
    fn skips_urls_and_pure_anchors() {
        let text = "[a](https://example.com) [b](mailto:x@example.com) [c](#layout)";
        assert!(targets(text).is_empty());
    }

    #[test]
    fn drops_anchors_and_titles_from_targets() {
        let text = r#"[a](README.md#layout) [b](docs/x.md "Title")"#;
        assert_eq!(targets(text), ["README.md", "docs/x.md"]);
    }

    #[test]
    fn ignores_link_syntax_inside_inline_code() {
        let text = "Generics: `def f[T](x: T)` and [rules](CONTRIBUTING.md)";
        assert_eq!(targets(text), ["CONTRIBUTING.md"]);
    }

    #[test]
    fn ignores_links_inside_fenced_code() {
        let text = "```markdown\n[a](missing.md)\n```\n[b](present.md)";
        assert_eq!(targets(text), ["present.md"]);
    }
}
