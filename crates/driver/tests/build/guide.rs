//! The beginner's guide in `docs/guide/` run as written (roadmap 0.8.3): every program it shows is
//! written to disk, every `baryon` command it shows is run, and what the command prints must be
//! what the guide says it prints. A guide whose examples stop working fails CI instead of
//! misleading a newcomer.
//!
//! A guide page marks its examples in two ways, both visible to the reader:
//!
//! - A line ``File `ball/main.byn`:`` followed by a ```` ```python ```` block writes the block to
//!   that path, relative to one scratch directory per page. A later block for the same path
//!   replaces the file, so a page can grow a program step by step.
//! - A ```` ```text ```` block whose first line is `$ baryon <command> <arguments>` runs that
//!   command in the scratch directory. The rest of the block is what it prints, standard output
//!   then standard error. The frames of a backtrace name the reader's own directory, so the guide
//!   writes them as one line `  ...`, and the run's frames are cut to that line before comparing.

use super::{BACKTRACE, BARYON, build_runtime};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// One example a guide page shows, in page order.
#[derive(Debug)]
enum Step {
    /// A file to write: its path relative to the scratch directory, and its text.
    File {
        /// Where the file goes, such as `ball/main.byn`.
        path: String,
        /// The file's text.
        text: String,
    },
    /// A command to run and what it must print.
    Command {
        /// The arguments after `baryon`, such as `["run", "ball"]`.
        arguments: Vec<String>,
        /// What the command prints, with backtrace frames written as `  ...`.
        expected: String,
        /// The line of the page the block starts on, to name it in a failure.
        line: usize,
    },
}

/// The steps of the guide page `text`, in order.
///
/// # Errors
///
/// Fails when a ``File `...`:`` line is not followed by a `python` block, or a block is not
/// closed: either is a mistake in the page that would otherwise be silently skipped.
fn steps(text: &str) -> Result<Vec<Step>, String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut steps = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let is_command = |next: Option<&&str>| next.is_some_and(|l| l.starts_with("$ baryon "));
        if let Some(path) = line
            .strip_prefix("File `")
            .and_then(|rest| rest.strip_suffix("`:"))
        {
            let fence = lines[index + 1..]
                .iter()
                .position(|line| !line.is_empty())
                .map(|offset| index + 1 + offset)
                .filter(|&fence| lines[fence] == "```python")
                .ok_or_else(|| format!("line {}: no python block follows", index + 1))?;
            let (text, end) = block(&lines, fence)?;
            steps.push(Step::File {
                path: path.to_owned(),
                text,
            });
            index = end + 1;
        } else if line == "```text" && is_command(lines.get(index + 1)) {
            let (body, end) = block(&lines, index)?;
            let (command, expected) = body.split_once('\n').unwrap_or((&body, ""));
            let arguments = command
                .trim_start_matches("$ baryon ")
                .split_whitespace()
                .map(str::to_owned)
                .collect();
            steps.push(Step::Command {
                arguments,
                expected: expected.to_owned(),
                line: index + 1,
            });
            index = end + 1;
        } else {
            index += 1;
        }
    }
    Ok(steps)
}

/// The text of the fenced block opening at `lines[fence]`, each line ending in `\n`, and the
/// index of its closing fence.
///
/// # Errors
///
/// Fails when the block is never closed.
fn block(lines: &[&str], fence: usize) -> Result<(String, usize), String> {
    let end = lines[fence + 1..]
        .iter()
        .position(|line| *line == "```")
        .map_or_else(
            || Err(format!("line {}: the block is not closed", fence + 1)),
            |offset| Ok(fence + 1 + offset),
        )?;
    let body = lines[fence + 1..end]
        .iter()
        .fold(String::new(), |mut body, line| {
            body.push_str(line);
            body.push('\n');
            body
        });
    Ok((body, end))
}

/// `text` with the frame lines after each backtrace header cut to one line `  ...`.
fn without_frames(text: &str) -> String {
    let mut kept = String::new();
    let mut in_backtrace = false;
    for line in text.split_inclusive('\n') {
        if in_backtrace && line.starts_with("  ") {
            continue;
        }
        in_backtrace = line == BACKTRACE;
        kept.push_str(line);
        if in_backtrace {
            kept.push_str("  ...\n");
        }
    }
    kept
}

/// Runs every step of the guide page at `page` in a fresh scratch directory, and returns each
/// command whose output differs from the page's: where it is on the page, the command, what it
/// printed and what the page says.
///
/// # Errors
///
/// Fails when the page cannot be read or is malformed, a file cannot be written, or `baryon`
/// cannot be started.
fn run_page(page: &Path) -> Result<Vec<String>, String> {
    let fail = |error: std::io::Error| format!("{}: {error}", page.display());
    let name = page.file_stem().unwrap_or_default().to_string_lossy();
    let scratch = std::env::temp_dir().join(format!("baryon-guide-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&scratch);
    let steps = steps(&fs::read_to_string(page).map_err(fail)?)?;
    if !steps
        .iter()
        .any(|step| matches!(step, Step::Command { .. }))
    {
        return Err(format!("{} runs no command", page.display()));
    }
    let mut mismatches = Vec::new();
    for step in steps {
        match step {
            Step::File { path, text } => {
                let path = scratch.join(path);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(fail)?;
                }
                fs::write(path, text).map_err(fail)?;
            }
            Step::Command {
                arguments,
                expected,
                line,
            } => {
                let output = Command::new(BARYON)
                    .args(&arguments)
                    .current_dir(&scratch)
                    .output()
                    .map_err(fail)?;
                let printed = without_frames(&format!(
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ));
                if printed != expected {
                    mismatches.push(format!(
                        "{}:{line}: `baryon {}` printed\n{printed}\nbut the guide says\n{expected}",
                        page.display(),
                        arguments.join(" ")
                    ));
                }
            }
        }
    }
    let _ = fs::remove_dir_all(&scratch);
    Ok(mismatches)
}

/// Every page of the guide runs as written.
#[test]
fn the_guide_runs_as_written() {
    build_runtime();
    let guide = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/guide");
    let mut pages: Vec<PathBuf> = fs::read_dir(&guide)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    pages.sort();
    assert!(!pages.is_empty(), "no guide pages in {}", guide.display());
    let mismatches: Vec<String> = pages
        .iter()
        .flat_map(|page| run_page(page).unwrap())
        .collect();
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n\n"));
}

/// The page format is read as documented: files, commands with their output, and other blocks
/// left alone.
#[test]
fn guide_steps_are_read_from_the_page() {
    let page = "\
Some prose.

File `hello/main.byn`:

```python
def main():
    print(\"hi\")
```

```sh
cargo build
```

```text
$ baryon run hello
hi
```
";
    let steps = steps(page).unwrap();
    assert_eq!(steps.len(), 2, "{steps:?}");
    assert!(matches!(
        &steps[0],
        Step::File { path, text } if path == "hello/main.byn" && text == "def main():\n    print(\"hi\")\n"
    ));
    assert!(matches!(
        &steps[1],
        Step::Command { arguments, expected, line: 14 } if arguments == &["run", "hello"] && expected == "hi\n"
    ));
}

/// Backtrace frames, which name the reader's directory, are cut to one line.
#[test]
fn backtrace_frames_are_cut() {
    let printed = format!(
        "panic: oops\n{BACKTRACE}  main.main at /home/ada/main.byn:2:5\n  main.f at /home/ada/main.byn:7:1\n\nafter\n"
    );
    assert_eq!(
        without_frames(&printed),
        format!("panic: oops\n{BACKTRACE}  ...\n\nafter\n")
    );
}
