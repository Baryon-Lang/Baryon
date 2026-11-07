//! The documentation generator: turns the documentation strings in a package's source into
//! Markdown pages, one per module, behind `baryon doc`.
//!
//! # Where it sits
//!
//! It needs only the parser's output. It reads a module's [`SyntaxTree`] and source text and
//! returns a [`ModuleDocumentation`]: the module's own documentation and every public item in
//! it with its signature and documentation. That value renders as a Markdown page with
//! [`ModuleDocumentation::markdown`], and a package's modules render as one index page with
//! [`index`]. It is an implementation crate that reads only the `syntax` interface (decision
//! 0001), so it runs on any file that parses, whether or not it type-checks. The driver finds
//! the files, names the modules and writes the pages.
//!
//! # What it documents
//!
//! The documentation positions are the language reference's, in its [syntax chapter]: a string
//! literal first in a module, class or function documents it, and a string literal right after
//! a constant, type alias or field documents that. A module's items are its `Final` constants,
//! type aliases, functions and classes; a class's are its fields and methods. Each is listed in
//! source order with its signature as written, so the page reads in the order the author chose.
//! A name starting with `_` is private to its module and left out; special methods such as
//! `__add__` are kept. An item with no documentation string is still listed, with its signature.
//!
//! Documentation strings are cleaned as Python's `inspect.cleandoc` cleans them (indentation
//! and surrounding blank lines removed) and are otherwise copied into the page as written, so
//! they are Markdown: backticks for code, `-` for lists. Their first line is the summary the
//! index shows.
//!
//! [syntax chapter]: https://github.com/csimpson1964/New-Programming-Language/blob/main/docs/reference/syntax.md
//!
//! # Example
//!
//! Trees normally come from the parser. Built by hand, a module with one documented constant
//! documents like this:
//!
//! ```
//! use source::SourceMap;
//! use syntax::{Role, SyntaxKind, TreeBuilder};
//!
//! let text = "\"\"\"Heat flow.\"\"\"\nALPHA: Final = 0.1\n\"\"\"Diffusivity.\"\"\"\n";
//! let mut sources = SourceMap::new();
//! let file = sources.add("heat.byn", text)?;
//!
//! let mut builder = TreeBuilder::new(file);
//! builder.open(SyntaxKind::Module, Role::Root, 0, 55);
//! builder.open(SyntaxKind::ExpressionStatement, Role::Body, 0, 16);
//! builder.leaf(SyntaxKind::StringLiteral, Role::Value, 0, 16);
//! builder.close();
//! builder.open(SyntaxKind::AnnotatedAssign, Role::Body, 17, 35);
//! builder.leaf(SyntaxKind::Name, Role::Target, 17, 22);
//! builder.leaf(SyntaxKind::Name, Role::Annotation, 24, 29);
//! builder.leaf(SyntaxKind::Float, Role::Value, 32, 35);
//! builder.close();
//! builder.open(SyntaxKind::ExpressionStatement, Role::Body, 36, 54);
//! builder.leaf(SyntaxKind::StringLiteral, Role::Value, 36, 54);
//! builder.close();
//! builder.close();
//! let tree = builder.finish();
//!
//! let module = documentation::document(&tree, text, "heat");
//! assert_eq!(module.summary(), Some("Heat flow."));
//! assert_eq!(module.items()[0].name(), "ALPHA");
//! assert_eq!(
//!     module.markdown(),
//!     "# `heat`\n\nHeat flow.\n\n## `ALPHA`\n\n```python\nALPHA: Final = 0.1\n```\n\nDiffusivity.\n",
//! );
//! # Ok::<(), source::SourceError>(())
//! ```

mod docstring;
mod extract;
mod markdown;

use syntax::SyntaxTree;

/// Reads the documentation of the module `name` from `tree`, parsed from `source`.
///
/// `name` is the module's dotted name, such as `physics.nbody`, which the page is titled and
/// named after. `source` must be the text the tree was parsed from. A tree with syntax errors
/// documents what it can; the driver documents only files that parse cleanly.
///
/// Cost: one pass over the module's top-level statements and its classes' bodies. Function
/// bodies are never entered past their first statement.
///
/// See the crate documentation for an example.
#[must_use]
pub fn document(tree: &SyntaxTree, source: &str, name: &str) -> ModuleDocumentation {
    extract::Extractor::new(tree, source).module(name)
}

/// The index page of a package: a Markdown table with one row per module, in the order given,
/// linking the module's page by its [`file_name`](ModuleDocumentation::file_name) and showing
/// its [`summary`](ModuleDocumentation::summary).
///
/// ```
/// use source::SourceMap;
/// use syntax::{Role, SyntaxKind, TreeBuilder};
///
/// let text = "\"Integrators.\"\n";
/// let mut sources = SourceMap::new();
/// let mut builder = TreeBuilder::new(sources.add("rk4.byn", text)?);
/// builder.open(SyntaxKind::Module, Role::Root, 0, 15);
/// builder.open(SyntaxKind::ExpressionStatement, Role::Body, 0, 14);
/// builder.leaf(SyntaxKind::StringLiteral, Role::Value, 0, 14);
/// builder.close();
/// builder.close();
/// let module = documentation::document(&builder.finish(), text, "ode.rk4");
///
/// assert_eq!(
///     documentation::index(&[module]),
///     "# Modules\n\n| Module | Summary |\n| --- | --- |\n| [`ode.rk4`](ode.rk4.md) | Integrators. |\n",
/// );
/// # Ok::<(), source::SourceError>(())
/// ```
#[must_use]
pub fn index(modules: &[ModuleDocumentation]) -> String {
    let mut page = String::new();
    markdown::index_page(modules, &mut page);
    page
}

/// The documentation of one module: its own documentation string and its public items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleDocumentation {
    /// The module's dotted name, such as `physics.nbody`.
    name: String,
    /// The module's documentation string, cleaned; `None` when it has none.
    text: Option<String>,
    /// The module's public constants, type aliases, functions and classes, in source order.
    items: Vec<Item>,
}

impl ModuleDocumentation {
    /// The module's dotted name, such as `physics.nbody`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The module's documentation, cleaned of its source indentation; `None` when the module
    /// has no documentation string.
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    /// The first line of the module's documentation, its one-line summary; `None` when it has
    /// none.
    #[must_use]
    pub fn summary(&self) -> Option<&str> {
        self.text()?.lines().next().filter(|line| !line.is_empty())
    }

    /// The module's public constants, type aliases, functions and classes, in source order.
    #[must_use]
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// The name of the module's page: its dotted name with `.md` added, such as
    /// `physics.nbody.md`. Every page sits in one directory, so pages link to each other by
    /// file name alone.
    #[must_use]
    pub fn file_name(&self) -> String {
        format!("{}.md", self.name)
    }

    /// The module's page in Markdown.
    ///
    /// The page is titled with the module's name, then gives its documentation. Each item
    /// follows in source order under a second-level heading naming it, with its signature as a
    /// `python` code block (the syntax is Python's, so every highlighter colors it) and then its
    /// documentation. A class's fields and methods follow the class under third-level headings
    /// such as `Atoms.positions`.
    ///
    /// See the crate documentation for an example.
    #[must_use]
    pub fn markdown(&self) -> String {
        let mut page = String::new();
        markdown::module_page(self, &mut page);
        page
    }
}

/// One documented item: a constant, type alias, function or class, or a field or method of a
/// class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// The name it is declared with.
    name: String,
    /// Its declaration as written, on one line; a definition's decorators come first, a line
    /// each.
    signature: String,
    /// Its documentation string, cleaned; `None` when it has none.
    text: Option<String>,
    /// A class's public fields and methods, in source order; empty for anything else.
    members: Vec<Item>,
}

impl Item {
    /// The name it is declared with.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Its declaration as written, joined onto one line: `def step(atoms: InOut[Atoms]) -> None`
    /// for a function, `class Atoms(Struct)` for a class, the whole statement for a constant,
    /// type alias or field. Comments, the closing colon and the trailing comma of a parameter
    /// list written one parameter per line are left out. A definition's decorators come first,
    /// each on its own line.
    #[must_use]
    pub fn signature(&self) -> &str {
        &self.signature
    }

    /// Its documentation, cleaned of its source indentation; `None` when it has none.
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    /// A class's public fields and methods, in source order; empty for anything else.
    #[must_use]
    pub fn members(&self) -> &[Item] {
        &self.members
    }
}
