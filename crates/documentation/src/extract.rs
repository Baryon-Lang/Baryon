//! Reading a module's documented items out of its syntax tree.
//!
//! The walk follows the documentation positions of the [syntax chapter] of the language
//! reference: a string first in a module, class or function documents it, and a string right
//! after a constant, type alias or field documents that.
//!
//! [syntax chapter]: https://github.com/Baryon-Lang/Baryon/blob/main/docs/reference/syntax.md

use crate::{Item, ModuleDocumentation, docstring};
use syntax::{NodeId, Role, SyntaxKind, SyntaxTree};

/// Reads the documentation of one module from its tree.
pub(crate) struct Extractor<'a> {
    /// The module's tree.
    tree: &'a SyntaxTree,
    /// The text the tree was parsed from.
    source: &'a str,
}

impl<'a> Extractor<'a> {
    /// An extractor over `tree`, parsed from `source`.
    pub(crate) fn new(tree: &'a SyntaxTree, source: &'a str) -> Self {
        Self { tree, source }
    }

    /// The documentation of the module, named `name`.
    pub(crate) fn module(&self, name: &str) -> ModuleDocumentation {
        let root = self.tree.root();
        ModuleDocumentation {
            name: name.to_owned(),
            text: self.leading_documentation(root),
            items: self.items(root, false),
        }
    }

    /// The public items in the body of `parent`, a module or a class, in source order. `in_class`
    /// says which: a class holds fields and methods, a module constants, type aliases, functions
    /// and classes.
    fn items(&self, parent: NodeId, in_class: bool) -> Vec<Item> {
        let statements: Vec<NodeId> = self.tree.children_with(parent, Role::Body).collect();
        let mut items = Vec::new();
        for (index, &statement) in statements.iter().enumerate() {
            let following = statements.get(index + 1).copied();
            let item = match self.tree.kind(statement) {
                SyntaxKind::FunctionDefinition => self.definition(statement),
                SyntaxKind::ClassDefinition if !in_class => self.definition(statement),
                SyntaxKind::AnnotatedAssign => self.assignment(statement, following),
                SyntaxKind::Assign if !in_class => self.assignment(statement, following),
                _ => None,
            };
            items.extend(item.filter(|item| is_public(&item.name)));
        }
        items
    }

    /// The item a function or class definition declares. A class's fields and methods are its
    /// members.
    fn definition(&self, id: NodeId) -> Option<Item> {
        let name = self.tree.child(id, Role::Name)?;
        let is_class = self.tree.kind(id) == SyntaxKind::ClassDefinition;
        Some(Item {
            name: self.tree.text(name, self.source).to_owned(),
            signature: self.header(id),
            text: self.leading_documentation(id),
            members: if is_class {
                self.items(id, true)
            } else {
                Vec::new()
            },
        })
    }

    /// The item a constant, type alias or field declares: one name assigned or annotated,
    /// documented by the string that follows it, if `following` is one. Assignments to anything
    /// but a single name declare nothing documentable.
    fn assignment(&self, id: NodeId, following: Option<NodeId>) -> Option<Item> {
        let mut targets = self.tree.children_with(id, Role::Target);
        let target = targets.next()?;
        if targets.next().is_some() || self.tree.kind(target) != SyntaxKind::Name {
            return None;
        }
        Some(Item {
            name: self.tree.text(target, self.source).to_owned(),
            signature: one_line(self.tree.text(id, self.source)),
            text: following.and_then(|following| self.documentation_string(following)),
            members: Vec::new(),
        })
    }

    /// The source of a definition's header on one line: its decorators, each on its own line,
    /// then `def` or `class` through the end of the parameters, return type or bases, without
    /// the closing colon.
    fn header(&self, id: NodeId) -> String {
        let mut lines: Vec<String> = self
            .tree
            .children_with(id, Role::Decorator)
            .map(|decorator| one_line(self.tree.text(decorator, self.source)))
            .collect();
        let after_decorators = self
            .tree
            .children_with(id, Role::Decorator)
            .last()
            .map_or(self.tree.node(id).start(), |last| {
                self.tree.node(last).end()
            });
        let header_end = self
            .tree
            .children(id)
            .filter(|&child| {
                matches!(
                    self.tree.node(child).role(),
                    Role::Name
                        | Role::TypeParameters
                        | Role::Parameters
                        | Role::Returns
                        | Role::Arguments
                )
            })
            .map(|child| self.tree.node(child).end())
            .max()
            .unwrap_or(after_decorators);
        let header = self
            .source
            .get(after_decorators as usize..header_end as usize)
            .unwrap_or_default();
        lines.push(one_line(header));
        lines.join("\n")
    }

    /// The text of the documentation string that opens the body of `id`, a module, class or
    /// function, if its first statement is one.
    fn leading_documentation(&self, id: NodeId) -> Option<String> {
        let first = self.tree.children_with(id, Role::Body).next()?;
        self.documentation_string(first)
    }

    /// The text of `statement` if it is a string literal on its own line.
    fn documentation_string(&self, statement: NodeId) -> Option<String> {
        if self.tree.kind(statement) != SyntaxKind::ExpressionStatement {
            return None;
        }
        let value = self.tree.child(statement, Role::Value)?;
        if self.tree.kind(value) != SyntaxKind::StringLiteral {
            return None;
        }
        docstring::text(self.tree.text(value, self.source))
    }
}

/// Whether a name is public: it does not start with `_`, the mark of a private name, unless it
/// is a special method such as `__add__`, which callers reach through an operator.
fn is_public(name: &str) -> bool {
    !name.starts_with('_') || (name.len() > 4 && name.starts_with("__") && name.ends_with("__"))
}

/// `text` on one line: its lines trimmed and joined with one space, except right after an
/// opening bracket or right before a closing one, where they join with none. A comma that ends
/// a line before a closing bracket is dropped, so a signature written one parameter per line,
/// with a trailing comma, reads as it would written on one line. Text within a line is kept as
/// written, so a string or a one-element tuple such as `(3,)` is never changed. Comments are
/// dropped.
fn one_line(text: &str) -> String {
    let mut line = String::with_capacity(text.len());
    let pieces = text.lines().map(|piece| without_comment(piece).trim());
    for piece in pieces.filter(|piece| !piece.is_empty()) {
        let closes = piece.starts_with([')', ']']);
        if closes && line.ends_with(',') {
            line.pop();
        }
        if !(line.is_empty() || closes || line.ends_with(['(', '['])) {
            line.push(' ');
        }
        line.push_str(piece);
    }
    line
}

/// `line` up to its `#` comment, if it has one. A `#` inside a string on the line is not a
/// comment. A string that spans lines is not tracked, which a signature or constant never needs.
fn without_comment(line: &str) -> &str {
    let mut quote = None;
    let mut characters = line.char_indices();
    while let Some((offset, character)) = characters.next() {
        match (quote, character) {
            (Some(_), '\\') => {
                characters.next();
            }
            (Some(open), _) if character == open => quote = None,
            (None, '"' | '\'') => quote = Some(character),
            (None, '#') => return &line[..offset],
            _ => {}
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_written_over_several_lines_read_as_one() {
        assert_eq!(
            one_line("def step(\n    atoms: InOut[Atoms],\n    dt: f64,\n) -> None"),
            "def step(atoms: InOut[Atoms], dt: f64) -> None",
        );
        assert_eq!(
            one_line("Vec3 = Fixed[\n    f64, (3,)\n]"),
            "Vec3 = Fixed[f64, (3,)]",
        );
        assert_eq!(
            one_line("def step(\n    dt: f64,  # seconds\n    label: str = \"#1\",\n)"),
            "def step(dt: f64, label: str = \"#1\")",
        );
        assert_eq!(one_line("SEP: Final = \",  )\""), "SEP: Final = \",  )\"");
    }

    #[test]
    fn private_names_are_hidden_but_special_methods_are_not() {
        assert!(is_public("energy"));
        assert!(!is_public("_scratch"));
        assert!(!is_public("__hidden"));
        assert!(is_public("__add__"));
        assert!(!is_public("____"));
    }
}
