//! Tests for the Learn site: how links are rewritten, how pages are ordered, and that every
//! page in `docs/` has a place in the summary.

use super::*;

/// What `target`, written in a page of `docs/<directory>` served from the same place,
/// becomes on the site.
fn served(directory: &str, target: &str) -> Option<String> {
    let root = workspace_root();
    let docs = root.join("docs");
    let written = docs.join(directory);
    let places = Places {
        repository: &root,
        docs: &docs,
        written: &written,
        served: &written,
    };
    served_target(target, &places).unwrap()
}

#[test]
fn keeps_links_to_pages_and_images_inside_docs() {
    assert_eq!(served("guide", "language-tour.md#2-records"), None);
    assert_eq!(served("guide", "../brand/baryon-logo.svg"), None);
    assert_eq!(served("guide", "https://example.com/x.md"), None);
    assert_eq!(served("guide", "#an-anchor"), None);
}

#[test]
fn points_links_that_leave_the_site_at_github() {
    let github = "https://github.com/Baryon-Lang/Baryon";
    assert_eq!(
        served("guide", "../../ROADMAP.md#ideas"),
        Some(format!("{github}/blob/main/ROADMAP.md#ideas"))
    );
    assert_eq!(
        served("guide", "../../editors/vscode"),
        Some(format!("{github}/tree/main/editors/vscode"))
    );
    assert_eq!(
        served("guide", "../examples/heat_equation.byn"),
        Some(format!(
            "{github}/blob/main/docs/examples/heat_equation.byn"
        ))
    );
    assert_eq!(
        served("guide", "/CHANGELOG.md"),
        Some(format!("{github}/blob/main/CHANGELOG.md"))
    );
}

#[test]
fn points_readme_links_at_the_index_mdbook_writes() {
    assert_eq!(
        served("guide", "../brand/").as_deref(),
        Some("../brand/index.md")
    );
    assert_eq!(
        served("guide", "../reference#top").as_deref(),
        Some("../reference/index.md#top")
    );
    assert_eq!(
        served("guide", "README.md#tests").as_deref(),
        Some("index.md#tests")
    );
}

#[test]
fn follows_the_two_moved_pages() {
    assert_eq!(
        served("guide", "../README.md").as_deref(),
        Some("../documentation.md")
    );
    assert_eq!(
        served("reference", "../guide/learn.md").as_deref(),
        Some("../index.md")
    );
}

#[test]
fn rewrites_links_from_where_a_moved_page_is_served() {
    let docs = workspace_root().join("docs");
    let page = docs.join(HOME_PAGE);
    let text = "[guide](README.md), [tour](language-tour.md), [logo](../brand/baryon-logo.svg)";
    let expected = "[guide](guide/index.md), [tour](guide/language-tour.md), \
                    [logo](brand/baryon-logo.svg)";
    let rewritten = rewrite_links(text, &docs, &page, &docs.join("README.md")).unwrap();
    assert_eq!(rewritten, expected);
}

#[test]
fn reports_a_link_to_a_missing_file() {
    let root = workspace_root();
    let docs = root.join("docs");
    let places = Places {
        repository: &root,
        docs: &docs,
        written: &docs,
        served: &docs,
    };
    let result = served_target("missing.md", &places);
    assert_eq!(result, Err(String::from("`missing.md` does not exist")));
}

#[test]
fn rewrites_prose_links_but_not_code() {
    let docs = workspace_root().join("docs");
    let page = docs.join("guide/vscode.md");
    let text = "See [the roadmap](../../ROADMAP.md) and `[x](../../ROADMAP.md)`.\n\
                ```markdown\n[y](../../ROADMAP.md)\n```\n";
    let expected = "See [the roadmap](https://github.com/Baryon-Lang/Baryon/blob/main/\
                    ROADMAP.md) and `[x](../../ROADMAP.md)`.\n\
                    ```markdown\n[y](../../ROADMAP.md)\n```\n";
    assert_eq!(rewrite_links(text, &docs, &page, &page).unwrap(), expected);
}

#[test]
fn normalizes_parent_components_by_name() {
    assert_eq!(
        normalize(Path::new("/repo/docs/guide/../../ROADMAP.md")),
        Path::new("/repo/ROADMAP.md")
    );
}

#[test]
fn writes_relative_paths_between_directories() {
    let directory = Path::new("/docs/guide");
    assert_eq!(relative(directory, Path::new("/docs/guide/a.md")), "a.md");
    assert_eq!(
        relative(directory, Path::new("/docs/index.md")),
        "../index.md"
    );
}

#[test]
fn orders_pages_as_the_index_links_them() {
    let docs = workspace_root().join("docs");
    let pages = ordered_pages(&docs.join("guide"), &[]).unwrap();
    let names: Vec<_> = pages
        .iter()
        .map(|page| page.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names[..2], ["README.md", "language-tour.md"]);
}

#[test]
fn stages_every_page_with_a_place_in_the_summary() {
    let docs = workspace_root().join("docs");
    let source = std::env::temp_dir().join(format!("learn-site-{}", std::process::id()));
    stage(&docs, &source).unwrap();
    let text = fs::read_to_string(source.join("SUMMARY.md")).unwrap();
    fs::remove_dir_all(&source).unwrap();
    assert!(text.starts_with("# Summary\n\n[Learn Baryon](README.md)\n\n# Learn\n\n"));
    assert!(text.contains("\n- [Beginner's guide](guide/README.md)\n"));
    assert!(text.contains("\n- [Decision records](decisions/README.md)\n"));
    assert!(text.contains("\n- [Documentation](documentation.md)\n"));
}
