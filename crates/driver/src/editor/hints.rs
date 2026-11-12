//! Inlay hints: the type the checker inferred for each variable that has no annotation, shown
//! after its first assignment, so the reader sees `energy: f64` where the code says `energy = …`
//! (roadmap 2.4).

use crate::Session;
use database::File;
use syntax::{NodeId, Role, SyntaxKind, SyntaxTree};
use types::{Item, TypeDb, TypeKind};

/// One hint: text the editor shows at a place in the code without it being part of the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlayHint {
    /// The byte offset the hint is shown at: the end of the variable's name.
    pub offset: u32,
    /// What is shown, such as `: f64`.
    pub label: String,
}

impl Session {
    /// The type hints for `file`: for each variable of each function and method, at the end of
    /// its first binding, the type the checker gave it, unless the code already says it. A
    /// parameter and a variable written `x: T = …` carry their annotation and get none; a
    /// variable whose type is unknown, as in code too broken to check, gets none.
    ///
    /// Cost: one walk of each function's tree, reading the typed bodies `baryon check` caches.
    ///
    /// ```
    /// use driver::Session;
    ///
    /// let mut session = Session::new();
    /// let text = "def kinetic(m: f64, v: f64) -> f64:\n    energy = 0.5 * m * v * v\n    \
    ///             return energy\n";
    /// let file = session.add_file("main.byn", text)?;
    /// let hints = session.inlay_hints(file);
    /// assert_eq!(hints.len(), 1);
    /// assert_eq!(hints[0].label, ": f64");
    /// assert_eq!(&text[..hints[0].offset as usize], "def kinetic(m: f64, v: f64) -> f64:\n    energy");
    /// # Ok::<(), source::SourceError>(())
    /// ```
    #[must_use]
    pub fn inlay_hints(&self, file: File) -> Vec<InlayHint> {
        let db: &dyn TypeDb = &self.database;
        let tree = db.parse(file).tree();
        let mut hints = Vec::new();
        for statement in tree.children_with(tree.root(), Role::Body) {
            match tree.kind(statement) {
                SyntaxKind::FunctionDefinition => {
                    self.function_hints(db, file, tree, statement, &mut hints);
                }
                SyntaxKind::ClassDefinition => {
                    for method in tree.children_with(statement, Role::Body) {
                        if tree.kind(method) == SyntaxKind::FunctionDefinition {
                            self.function_hints(db, file, tree, method, &mut hints);
                        }
                    }
                }
                _ => {}
            }
        }
        hints.sort_by_key(|hint| hint.offset);
        hints
    }

    /// Adds to `hints` those of the function defined by the statement `function`.
    fn function_hints(
        &self,
        db: &dyn TypeDb,
        file: File,
        tree: &SyntaxTree,
        function: NodeId,
        hints: &mut Vec<InlayHint>,
    ) {
        let Some(name) = tree.child(function, Role::Name) else {
            return;
        };
        let body = db.body(Item::new(db, self.package, file, name));
        let mut pending: Vec<NodeId> = tree.children_with(function, Role::Body).collect();
        while let Some(node) = pending.pop() {
            if tree.kind(node) == SyntaxKind::Name
                && let Some(ty) = body.local(node)
                && !matches!(ty.kind(db), TypeKind::Unknown)
            {
                hints.push(InlayHint {
                    offset: tree.node(node).end(),
                    label: format!(": {}", ty.display(db)),
                });
            }
            // An annotated assignment's target already says its type.
            let annotated = tree.kind(node) == SyntaxKind::AnnotatedAssign;
            pending.extend(
                tree.children(node)
                    .filter(|&child| !(annotated && tree.node(child).role() == Role::Target)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Session;

    /// Each hint of `text` as the name it follows and its label.
    fn hints(text: &str) -> Vec<(String, String)> {
        let mut session = Session::new();
        let file = session.add_file("main.byn", text).unwrap();
        session
            .inlay_hints(file)
            .into_iter()
            .map(|hint| {
                let before = &text[..hint.offset as usize];
                let start = before
                    .rfind(|character: char| !(character.is_alphanumeric() || character == '_'))
                    .map_or(0, |start| start + 1);
                (before[start..].to_owned(), hint.label)
            })
            .collect()
    }

    /// Owned strings from string slices.
    fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
        expected
            .iter()
            .map(|(name, label)| ((*name).to_owned(), (*label).to_owned()))
            .collect()
    }

    /// A variable gets one hint, at its first assignment; a loop variable and each name a
    /// tuple is unpacked into get theirs; parameters and annotated variables get none.
    #[test]
    fn only_inferred_variables_get_a_hint_once() {
        let text = "def sums(n: i64) -> f64:\n    \
                    total = 0.0\n    \
                    count: i64 = 0\n    \
                    for i in range(n):\n        \
                        total = total + 1.0\n        \
                        count += 1\n    \
                    low, high = 1, 2.5\n    \
                    return total + high\n";
        assert_eq!(
            hints(text),
            pairs(&[
                ("total", ": f64"),
                ("i", ": i64"),
                ("low", ": i64"),
                ("high", ": f64"),
            ])
        );
    }

    /// A method's variables get hints like a function's.
    #[test]
    fn methods_get_hints() {
        let text = "class Body(Struct):\n    \
                    mass: f64\n\n    \
                    def doubled(self) -> f64:\n        \
                        twice = self.mass * 2.0\n        \
                        return twice\n";
        assert_eq!(hints(text), pairs(&[("twice", ": f64")]));
    }

    /// Code too broken to check gives no hint rather than a wrong one.
    #[test]
    fn an_unknown_type_gives_no_hint() {
        assert_eq!(hints("def f():\n    x = undefined_name\n"), Vec::new());
    }
}
