//! `cargo xtask learn-site [<output>]`: builds the Learn site, the HTML version of `docs/` that
//! the `learn.yml` workflow publishes on GitHub Pages.
//!
//! The Markdown in `docs/` stays the one source: GitHub renders it in the repository, and this
//! command renders the same files as a website with a sidebar and search. It needs `mdbook`
//! ([`MDBOOK_VERSION`]) on the `PATH`, and works in four steps:
//!
//! 1. Copy `docs/` into a staging directory, `<output>/docs`, so the repository is never
//!    touched and the edit link on each page names its file in the repository.
//! 2. Rewrite each link the site cannot serve: a link that leaves `docs/`, such as
//!    `../../ROADMAP.md`, or names a file the site would show as raw bytes, such as an example's
//!    `.byn` file, points at the file on GitHub instead. mdBook turns each `README.md` into an
//!    `index.html` but leaves links to it alone, so a link to a `README.md`, or to a directory
//!    that has one, points at `index.md`, which mdBook links as `index.html`.
//! 3. Write the table of contents, `SUMMARY.md`, from [`PARTS`]: each directory's pages in the
//!    order its `README.md` first links them, then the rest by name. A Markdown file no part
//!    covers stops the build, since the site would have no page for it.
//! 4. Run `mdbook build`, which writes the site to `<output>/html`.
//!
//! Two pages move. The site opens on [`HOME_PAGE`], so it is served as the top `README.md`,
//! which mdBook makes the site's `index.html`, and `docs/README.md`, the map of all
//! documentation, is served as [`DOCUMENTATION_MAP`]. Links to both follow them.
//!
//! The default output is `target/learn-site`.

use crate::{Outcome, collect_files, workspace_root};
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

/// The `mdbook` release the site is built with, and the one `learn.yml` installs.
const MDBOOK_VERSION: &str = "0.5.4";

/// Where files that leave the site are linked to: the repository's `main` branch on GitHub.
const REPOSITORY: &str = "https://github.com/Baryon-Lang/Baryon";

/// The site's first page, relative to `docs/`: what Baryon is and where to start.
const HOME_PAGE: &str = "guide/learn.md";

/// What `docs/README.md`, the map of all documentation, is called on the site. mdBook makes a
/// directory's `README.md` its `index.html`, which at the top would push out [`HOME_PAGE`], so
/// the map is renamed and every link to it rewritten. `build` points both moved pages' edit
/// links back at their files in `docs/`.
const DOCUMENTATION_MAP: &str = "documentation.md";

/// File types the site serves itself because a page shows them, such as the logo.
const IMAGE_EXTENSIONS: [&str; 4] = ["svg", "png", "jpg", "gif"];

/// One part of the site's sidebar.
struct Part {
    /// The part's heading in the sidebar.
    title: &'static str,
    /// The directories under `docs/` whose pages the part lists, in order; `""` is `docs/`
    /// itself, without its subdirectories.
    directories: &'static [&'static str],
    /// Whether each directory's pages sit under its `README.md` in the sidebar, rather than
    /// beside it.
    nested: bool,
}

/// The sidebar, top to bottom. The guide is read in order, so its pages sit side by side;
/// the reference and the decision records are looked up, so their pages fold under an index.
const PARTS: &[Part] = &[
    Part {
        title: "Learn",
        directories: &["guide"],
        nested: false,
    },
    Part {
        title: "Reference",
        directories: &["reference", "examples"],
        nested: true,
    },
    Part {
        title: "Decisions",
        directories: &["decisions"],
        nested: true,
    },
    Part {
        title: "The project",
        directories: &["", "design", "brand"],
        nested: false,
    },
];

/// Builds the site into the directory named by `output`, or `target/learn-site`.
pub(crate) fn build(output: Option<&str>) -> Outcome {
    let root = workspace_root();
    let output = output.map_or_else(|| root.join("target/learn-site"), PathBuf::from);
    if output.exists() {
        fs::remove_dir_all(&output)?;
    }
    stage(&root.join("docs"), &output.join("docs"))?;
    fs::write(output.join("book.toml"), BOOK)?;

    let status = Command::new("mdbook")
        .arg("build")
        .arg(&output)
        .status()
        .map_err(|error| {
            format!(
                "could not run mdbook ({error}); install mdbook {MDBOOK_VERSION} from \
                 https://github.com/rust-lang/mdBook/releases"
            )
        })?;
    if !status.success() {
        return Err(format!("mdbook build failed ({status})").into());
    }
    // The two moved pages get edit links to where they are served; point them home.
    let html = output.join("html");
    for (page, served, written) in [
        ("index.html", "README.md", HOME_PAGE),
        ("documentation.html", DOCUMENTATION_MAP, "README.md"),
    ] {
        let text = fs::read_to_string(html.join(page))?;
        let edit = |path: &str| format!("/edit/main/docs/{path}\"");
        fs::write(html.join(page), text.replace(&edit(served), &edit(written)))?;
    }
    println!("learn-site: wrote {}", html.display());
    Ok(())
}

/// Copies `docs` into `source` with every page's links rewritten, the home page and the
/// documentation map moved to where the site serves them, and `SUMMARY.md` written.
///
/// # Errors
///
/// Fails when a file cannot be read or written, a link names a missing file, or a page has no
/// place in the summary.
fn stage(docs: &Path, source: &Path) -> Outcome {
    copy_directory(docs, source)?;
    let mut originals = Vec::new();
    collect_files(docs, "md", &mut originals)?;
    let mut staged = Vec::new();
    for original in &originals {
        let served = served_file(docs, original);
        let text = fs::read_to_string(original)?;
        staged.push((
            served.clone(),
            rewrite_links(&text, docs, original, &served)?,
        ));
    }
    fs::remove_file(source.join("README.md"))?;
    fs::remove_file(source.join(HOME_PAGE))?;
    for (served, text) in staged {
        fs::write(source.join(served.strip_prefix(docs)?), text)?;
    }
    fs::write(source.join("SUMMARY.md"), summary(source)?)?;
    Ok(())
}

/// Where the site serves the page `path` of `docs`, as a path in `docs`: the home page becomes
/// the top `README.md`, which mdBook makes the site's `index.html`, and the documentation map
/// that held that place becomes [`DOCUMENTATION_MAP`]. Every other page stays where it is.
fn served_file(docs: &Path, path: &Path) -> PathBuf {
    if path == docs.join("README.md") {
        docs.join(DOCUMENTATION_MAP)
    } else if path == docs.join(HOME_PAGE) {
        docs.join("README.md")
    } else {
        path.to_path_buf()
    }
}

/// The mdBook configuration. Edit links open the page's Markdown in `docs/` on GitHub, and
/// `create-missing = false` makes a summary entry with no file an error instead of a blank page.
const BOOK: &str = r#"[book]
title = "Learn Baryon"
language = "en"
src = "docs"

[build]
build-dir = "html"
create-missing = false

[output.html]
site-url = "/Baryon/"
git-repository-url = "https://github.com/Baryon-Lang/Baryon"
edit-url-template = "https://github.com/Baryon-Lang/Baryon/edit/main/{path}"

[output.html.fold]
enable = true
level = 0
"#;

/// Copies every file under `from` to the same place under `to`, creating directories as needed.
///
/// # Errors
///
/// Fails when a file cannot be read or written.
fn copy_directory(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// The page `text`, from the file `page` in `docs`, served from `served`, with every link the
/// site cannot serve rewritten as the module documentation describes. Links in code blocks and
/// inline code are left alone, as `check-links` leaves them.
///
/// # Errors
///
/// Fails when a link names a file that does not exist, which `check-links` also reports.
fn rewrite_links(text: &str, docs: &Path, page: &Path, served: &Path) -> Result<String, String> {
    let places = Places {
        repository: docs.parent().unwrap_or(docs),
        docs,
        written: page.parent().unwrap_or(docs),
        served: served.parent().unwrap_or(docs),
    };
    let mut rewritten = String::with_capacity(text.len());
    let mut open_fence: Option<&str> = None;
    for line in text.split_inclusive('\n') {
        let fence = ["```", "~~~"]
            .into_iter()
            .find(|fence| line.trim_start().starts_with(fence));
        match (open_fence, fence) {
            (None, Some(opened)) => open_fence = Some(opened),
            (Some(open), Some(closed)) if open == closed => open_fence = None,
            _ => {}
        }
        if open_fence.is_some() || fence.is_some() {
            rewritten.push_str(line);
            continue;
        }
        // Splitting on backticks puts inline code at odd positions; only prose is rewritten.
        for (index, piece) in line.split('`').enumerate() {
            if index > 0 {
                rewritten.push('`');
            }
            if index % 2 == 1 {
                rewritten.push_str(piece);
                continue;
            }
            let mut rest = piece;
            while let Some(start) = rest.find("](") {
                let after = &rest[start + 2..];
                let Some(end) = after.find(')') else {
                    break;
                };
                rewritten.push_str(&rest[..start + 2]);
                let target = &after[..end];
                let resolved = served_target(target, &places)
                    .map_err(|error| format!("{}: {error}", page.display()))?;
                rewritten.push_str(resolved.as_deref().unwrap_or(target));
                rest = &after[end..];
            }
            rewritten.push_str(rest);
        }
    }
    Ok(rewritten)
}

/// The directories a link is resolved and rewritten against.
struct Places<'a> {
    /// The repository root, which a link starting with `/` is relative to.
    repository: &'a Path,
    /// The repository's `docs/` directory: the site's content.
    docs: &'a Path,
    /// The directory of the page's file in `docs`, which its links are written relative to.
    written: &'a Path,
    /// The directory the site serves the page from, which rewritten links are relative to.
    served: &'a Path,
}

/// What the site should link to instead of `target`, or `None` when the site serves `target`
/// as written.
///
/// # Errors
///
/// Fails when the target names a file that does not exist.
fn served_target(target: &str, places: &Places<'_>) -> Result<Option<String>, String> {
    let (file, anchor) = target
        .split_once('#')
        .map_or((target, ""), |(file, anchor)| (file, anchor));
    if file.is_empty() || file.contains("://") || file.starts_with("mailto:") {
        return Ok(None);
    }
    let path = match file.strip_prefix('/') {
        Some(from_root) => places.repository.join(from_root),
        None => places.written.join(file),
    };
    let path = normalize(&path);
    if !path.exists() {
        return Err(format!("`{target}` does not exist"));
    }
    let anchor = if anchor.is_empty() {
        String::new()
    } else {
        format!("#{anchor}")
    };
    let inside = path.starts_with(places.docs);
    let as_written = !(path.is_dir() || file.starts_with('/')) && places.written == places.served;
    let index = path.join("README.md");
    let path = if inside && path.is_dir() && index.is_file() {
        index
    } else {
        path
    };
    let extension = path.extension().and_then(|extension| extension.to_str());
    if inside && path.is_file() && extension == Some("md") {
        // mdBook writes each `README.md` as `index.html`, and links `index.md` there.
        let served = served_file(places.docs, &path);
        let linked = if served.ends_with("README.md") {
            served.with_file_name("index.md")
        } else {
            served
        };
        if as_written && linked == path {
            return Ok(None);
        }
        return Ok(Some(format!(
            "{}{anchor}",
            relative(places.served, &linked)
        )));
    }
    if inside && path.is_file() && extension.is_some_and(|found| IMAGE_EXTENSIONS.contains(&found))
    {
        return Ok((!as_written).then(|| format!("{}{anchor}", relative(places.served, &path))));
    }
    let kind = if path.is_dir() { "tree" } else { "blob" };
    let relative = path.strip_prefix(places.repository).unwrap_or(&path);
    let relative = relative.to_string_lossy().replace('\\', "/");
    Ok(Some(format!("{REPOSITORY}/{kind}/main/{relative}{anchor}")))
}

/// `path` with every `.` and `..` component resolved by name, without asking the file system,
/// so a link through a missing directory is reported rather than followed.
fn normalize(path: &Path) -> PathBuf {
    let mut normal = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normal.pop();
            }
            other => normal.push(other),
        }
    }
    normal
}

/// The path that leads from `directory` to `path`, both absolute and normalized, written with
/// `/` as a link is.
fn relative(directory: &Path, path: &Path) -> String {
    let from: Vec<_> = directory.components().collect();
    let to: Vec<_> = path.components().collect();
    let shared = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = vec![String::from(".."); from.len() - shared];
    parts.extend(
        to[shared..]
            .iter()
            .map(|part| part.as_os_str().to_string_lossy().into_owned()),
    );
    parts.join("/")
}

/// The table of contents for the staged `source` directory, in mdBook's `SUMMARY.md` form.
///
/// # Errors
///
/// Fails when a Markdown file is in no part of [`PARTS`], or a page has no `# ` title.
fn summary(source: &Path) -> Result<String, String> {
    let mut text = String::from("# Summary\n\n");
    let home = source.join("README.md");
    let mut listed = vec![home.clone()];
    let _ = writeln!(text, "[{}](README.md)", title(&home)?);
    for part in PARTS {
        let _ = write!(text, "\n# {}\n\n", part.title);
        for directory in part.directories {
            let pages = ordered_pages(&source.join(directory), &listed)?;
            for (index, page) in pages.iter().enumerate() {
                let indent = if part.nested && index > 0 { "    " } else { "" };
                let relative = page
                    .strip_prefix(source)
                    .map_err(|error| error.to_string())?;
                let relative = relative.to_string_lossy().replace('\\', "/");
                let _ = writeln!(text, "{indent}- [{}]({relative})", title(page)?);
            }
            listed.extend(pages);
        }
    }
    let mut everything = Vec::new();
    collect_files(source, "md", &mut everything).map_err(|error| error.to_string())?;
    everything.retain(|page| !listed.contains(page) && !page.ends_with("SUMMARY.md"));
    everything.sort();
    if let Some(page) = everything.first() {
        return Err(format!(
            "{} is in no part of the Learn site; add its directory to PARTS in \
             xtask/src/learn_site.rs",
            page.display()
        ));
    }
    Ok(text)
}

/// The Markdown pages directly in `directory`, except those in `listed`: `README.md` first,
/// then the pages its links name in the order they first appear, then the rest by name.
///
/// # Errors
///
/// Fails when the directory cannot be read.
fn ordered_pages(directory: &Path, listed: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut remaining: Vec<PathBuf> = fs::read_dir(directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|found| found == "md"))
        .filter(|path| !listed.contains(path))
        .collect();
    remaining.sort();
    let mut pages = Vec::new();
    let index = directory.join("README.md");
    if let Some(position) = remaining.iter().position(|page| *page == index) {
        pages.push(remaining.remove(position));
        let text = fs::read_to_string(&index).map_err(|error| error.to_string())?;
        for target in link_targets(&text) {
            let linked = directory.join(target);
            if let Some(position) = remaining.iter().position(|page| *page == linked) {
                pages.push(remaining.remove(position));
            }
        }
    }
    pages.extend(remaining);
    Ok(pages)
}

/// The file part of every inline link `[text](target)` in `text`, in order of appearance.
fn link_targets(text: &str) -> Vec<&str> {
    let mut targets = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("](") {
        let after = &rest[start + 2..];
        let Some(end) = after.find(')') else {
            break;
        };
        targets.push(after[..end].split('#').next().unwrap_or_default());
        rest = &after[end..];
    }
    targets
}

/// The text of the first `# ` heading in the Markdown file `page`.
///
/// # Errors
///
/// Fails when the page cannot be read or has no such heading.
fn title(page: &Path) -> Result<String, String> {
    let text = fs::read_to_string(page).map_err(|error| format!("{}: {error}", page.display()))?;
    text.lines()
        .find_map(|line| line.strip_prefix("# "))
        .map(|heading| heading.trim().replace('[', "\\[").replace(']', "\\]"))
        .ok_or_else(|| format!("{} has no `# ` title", page.display()))
}

#[cfg(test)]
mod tests;
