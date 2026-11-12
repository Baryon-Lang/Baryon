//! Rename: every place a variable or a top-level definition is named, so the editor can change
//! them all at once (roadmap 2.4.2).
//!
//! The places come from the resolver's [`Resolution`](database::Resolution) of each file, which
//! records what every name refers to, so a rename never touches a different thing that happens
//! to have the same name. What a resolution cannot see is refused rather than half done: a
//! parameter, which a call may name as a keyword argument, and a field, method or enum member,
//! which is reached through values whose class only the checker knows.

use super::{Cursor, Target};
use crate::Session;
use database::{BUILTINS, Db, Definition, File, ModuleName};
use source::Span;
use syntax::{NodeId, SyntaxKind, SyntaxTree};

impl Session {
    /// Every span to replace with `new_name` to rename what the name at byte `offset` of `file`
    /// refers to: its definition and each use, across the package's files for a top-level
    /// definition, in source order within each file.
    ///
    /// A local variable is renamed within its function; a function, class, constant or type
    /// alias at the top of a module everywhere it is named, imports included.
    ///
    /// ```
    /// use driver::Session;
    ///
    /// let mut session = Session::new();
    /// let text = "def main():\n    total = 1.0\n    total = total * 2.0\n    print(total)\n";
    /// let file = session.add_file("main.byn", text)?;
    /// let at = u32::try_from(text.rfind("total").unwrap()).unwrap();
    /// let spans = session.rename(file, at, "product")?;
    /// assert_eq!(spans.len(), 4);
    /// let named = |span: &source::Span| &text[span.start() as usize..span.end() as usize];
    /// assert!(spans.iter().all(|span| named(span) == "total"));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    ///
    /// A message for the editor to show when the new name is not a Python name or is a keyword,
    /// when the cursor is not on a name, or when the name is one that cannot be renamed yet: a
    /// parameter, a field, a method, an enum member, a module, or a name the language or the
    /// standard library provides; and when `new_name` is taken anywhere the rename reaches, which
    /// would join two things into one or hide one behind the other: a builtin such as `print`,
    /// any name the variable's function uses, or any name in a module that the definition is
    /// defined or named in.
    pub fn rename(&self, file: File, offset: u32, new_name: &str) -> Result<Vec<Span>, String> {
        if new_name.contains('.') || ModuleName::from_dotted(new_name).is_none() {
            return Err(format!(
                "`{new_name}` is not a name a variable or definition can have"
            ));
        }
        if BUILTINS.contains(&new_name) {
            return Err(format!(
                "`{new_name}` is already a name the language provides"
            ));
        }
        let cursor = self.cursor(file, offset);
        let (_, target) = cursor
            .target()
            .ok_or_else(|| "there is no name here to rename".to_owned())?;
        match target {
            Target::Local(binding) => {
                if cursor.is_parameter(binding) {
                    return Err(
                        "renaming a parameter is not supported yet: a call may name it".to_owned(),
                    );
                }
                let function = cursor.function_of(binding);
                if names(cursor.tree, cursor.text, function).any(|name| name == new_name) {
                    return Err(format!("`{new_name}` is already a name in this function"));
                }
                let wanted = Definition::Local(binding);
                let mut spans = vec![cursor.tree.span(binding)];
                spans.extend(self.uses(file, &wanted));
                Ok(sorted(spans))
            }
            Target::Item(item) => {
                let (defined_in, name) = (item.file(cursor.db), item.name(cursor.db));
                if self.library.contains(&defined_in) {
                    return Err(
                        "this name is part of the language or its standard library".to_owned()
                    );
                }
                let tree = cursor.db.parse(defined_in).tree();
                let text = defined_in.source(cursor.db).text();
                if !super::outline::definitions(tree, text, tree.root())
                    .iter()
                    .any(|definition| definition.name == name)
                {
                    return Err("renaming a method is not supported yet".to_owned());
                }
                let wanted = Definition::Item(defined_in, name);
                let mut spans = vec![tree.span(name)];
                // The standard library never names the package's definitions.
                for &other in &self.files {
                    let uses = self.uses(other, &wanted);
                    if other == defined_in || !uses.is_empty() {
                        let tree = cursor.db.parse(other).tree();
                        let source = other.source(cursor.db);
                        if names(tree, source.text(), tree.root()).any(|name| name == new_name) {
                            return Err(format!(
                                "`{new_name}` is already a name in `{}`",
                                source.name()
                            ));
                        }
                    }
                    spans.extend(uses);
                }
                Ok(sorted(spans))
            }
            Target::Member { .. } | Target::Attribute(..) => {
                Err("renaming a field, method or enum member is not supported yet".to_owned())
            }
            Target::Module(_) => Err("renaming a module is not supported yet".to_owned()),
            Target::Standard(_) | Target::Builtin(_) => {
                Err("this name is part of the language or its standard library".to_owned())
            }
        }
    }

    /// The span of each name in `file` that the resolver found refers to `wanted`.
    fn uses(&self, file: File, wanted: &Definition) -> Vec<Span> {
        let tree = self.database.parse(file).tree();
        self.database
            .resolve(self.package, file)
            .references()
            .iter()
            .filter(|(_, definition)| definition == wanted)
            .map(|&(name, _)| tree.span(name))
            .collect()
    }
}

impl Cursor<'_> {
    /// Whether the `Name` node `binding` is a parameter's name.
    fn is_parameter(&self, binding: NodeId) -> bool {
        self.parent(binding).is_some_and(|parent| {
            matches!(
                self.tree.kind(parent),
                SyntaxKind::Parameter | SyntaxKind::StarParameter | SyntaxKind::DoubleStarParameter
            )
        })
    }
}

impl Cursor<'_> {
    /// The function definition `node` is inside, or the module's root when it is in none: the
    /// scope a local variable belongs to.
    fn function_of(&self, node: NodeId) -> NodeId {
        let mut current = node;
        while let Some(parent) = self.parent(current) {
            if self.tree.kind(parent) == SyntaxKind::FunctionDefinition {
                return parent;
            }
            current = parent;
        }
        self.tree.root()
    }
}

/// The text of every name under `node` in `tree`, whose file's text is `text`: each variable,
/// definition, import and attribute named there. A rename refuses a new name among them, which
/// is stricter than it must be for an attribute, and never misses a clash.
fn names<'a>(tree: &'a SyntaxTree, text: &'a str, node: NodeId) -> impl Iterator<Item = &'a str> {
    let mut pending = vec![node];
    std::iter::from_fn(move || {
        while let Some(current) = pending.pop() {
            pending.extend(tree.children(current));
            if tree.kind(current) == SyntaxKind::Name {
                return Some(tree.text(current, text));
            }
        }
        None
    })
}

/// `spans` in file and source order, each once.
fn sorted(mut spans: Vec<Span>) -> Vec<Span> {
    spans.sort_by_key(|span| (span.file().index(), span.start()));
    spans.dedup();
    spans
}

#[cfg(test)]
mod tests {
    use crate::Session;
    use source::Sources;

    /// The file name and the start of each span `rename` gives for the last `name` in the
    /// file `at` of `files`, renaming it to `new_name`.
    fn renamed(
        files: &[(&str, &str)],
        at: usize,
        name: &str,
        new_name: &str,
    ) -> Result<Vec<(String, u32)>, String> {
        let mut session = Session::new();
        let ids: Vec<_> = files
            .iter()
            .map(|&(path, text)| session.add_file(path, text).unwrap())
            .collect();
        let offset = u32::try_from(files[at].1.rfind(name).unwrap()).unwrap();
        let spans = session.rename(ids[at], offset, new_name)?;
        Ok(spans
            .iter()
            .map(|span| (session.file(span.file()).name().to_owned(), span.start()))
            .collect())
    }

    /// A variable is renamed where it is assigned and read in its own function, never in
    /// another function that has a variable of the same name.
    #[test]
    fn a_variable_is_renamed_within_its_function() {
        let text = "def f() -> f64:\n    x = 1.0\n    return x\n\n\
                    def g() -> f64:\n    x = 2.0\n    x = x + 1.0\n    return x\n";
        let starts = |name: &str| {
            text.match_indices(name)
                .map(|(start, _)| u32::try_from(start).unwrap())
                .filter(|&start| start > 50)
                .map(|start| ("main.byn".to_owned(), start))
                .collect::<Vec<_>>()
        };
        assert_eq!(renamed(&[("main.byn", text)], 0, "x", "y"), Ok(starts("x")));
    }

    /// A top-level function is renamed at its definition and wherever another module names
    /// it, through an import or through the module's name.
    #[test]
    fn a_function_is_renamed_across_modules() {
        let units = "def gamma(v: f64) -> f64:\n    return v\n";
        let main = "import physics.units\nfrom physics.units import gamma\n\n\
                    def main():\n    print(gamma(1.0))\n    print(physics.units.gamma(2.0))\n";
        let found = renamed(
            &[("physics/units.byn", units), ("main.byn", main)],
            1,
            "gamma",
            "lorentz",
        )
        .unwrap();
        let mut expected = vec![("physics/units.byn".to_owned(), 4)];
        expected.extend(
            main.match_indices("gamma")
                .map(|(start, _)| ("main.byn".to_owned(), u32::try_from(start).unwrap())),
        );
        assert_eq!(found, expected);
    }

    /// What the resolver cannot follow is refused: a parameter, a field, a standard library
    /// function, and a name that is not a valid new name.
    #[test]
    fn what_cannot_be_renamed_whole_is_refused() {
        let text = "class Body(Struct):\n    mass: f64\n\n\
                    def weigh(b: Body, g: f64) -> f64:\n    return b.mass * g\n";
        let files = [("main.byn", text)];
        assert_eq!(
            renamed(&files, 0, "g", "gravity"),
            Err("renaming a parameter is not supported yet: a call may name it".to_owned())
        );
        assert_eq!(
            renamed(&files, 0, "mass", "m"),
            Err("renaming a field, method or enum member is not supported yet".to_owned())
        );
        assert_eq!(
            renamed(&files, 0, "weigh", "class"),
            Err("`class` is not a name a variable or definition can have".to_owned())
        );
        assert_eq!(
            renamed(
                &[(
                    "main.byn",
                    "import linalg\n\ndef main():\n    print(linalg.det)\n"
                )],
                0,
                "det",
                "determinant",
            ),
            Err("this name is part of the language or its standard library".to_owned())
        );
        assert_eq!(
            renamed(&files, 0, "weigh", "two words"),
            Err("`two words` is not a name a variable or definition can have".to_owned())
        );
    }

    /// A new name already taken where the rename reaches is refused: joining two variables, or
    /// hiding a builtin, an import or another definition, would change what the program means.
    #[test]
    fn a_name_already_taken_is_refused() {
        let main = "import math\n\ndef step(x: f64) -> f64:\n    alpha = 1.0\n    beta = 2.0\n    \
                    return alpha + beta + math.sqrt(x)\n\n\
                    def other() -> f64:\n    gamma = 3.0\n    return gamma\n";
        let files = [("main.byn", main)];
        assert_eq!(
            renamed(&files, 0, "alpha", "beta"),
            Err("`beta` is already a name in this function".to_owned())
        );
        assert_eq!(
            renamed(&files, 0, "alpha", "math"),
            Err("`math` is already a name in this function".to_owned())
        );
        assert_eq!(
            renamed(&files, 0, "alpha", "print"),
            Err("`print` is already a name the language provides".to_owned())
        );
        assert_eq!(
            renamed(&files, 0, "gamma", "alpha").map(|spans| spans.len()),
            Ok(2),
            "a name used only in another function is free"
        );
        assert_eq!(
            renamed(&files, 0, "other", "step"),
            Err("`step` is already a name in `main.byn`".to_owned())
        );
        let units = "def gamma(v: f64) -> f64:\n    return v\n";
        let user = "from units import gamma\n\ndef main():\n    lorentz = 2.0\n    \
                    print(gamma(lorentz))\n";
        assert_eq!(
            renamed(
                &[("units.byn", units), ("main.byn", user)],
                0,
                "gamma",
                "lorentz"
            ),
            Err("`lorentz` is already a name in `main.byn`".to_owned())
        );
    }
}
