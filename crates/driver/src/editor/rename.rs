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
use database::{Db, Definition, File, ModuleName};
use source::Span;
use syntax::{NodeId, SyntaxKind};

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
    /// let spans = session.rename(file, at, "sum")?;
    /// assert_eq!(spans.len(), 4);
    /// assert!(spans.iter().all(|span| &text[span.start() as usize..span.end() as usize] == "total"));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Errors
    ///
    /// A message for the editor to show when the new name is not a Python name or is a keyword,
    /// when the cursor is not on a name, or when the name is one that cannot be renamed yet: a
    /// parameter, a field, a method, an enum member, a module, or a name the language or the
    /// standard library provides.
    pub fn rename(&self, file: File, offset: u32, new_name: &str) -> Result<Vec<Span>, String> {
        if new_name.contains('.') || ModuleName::from_dotted(new_name).is_none() {
            return Err(format!(
                "`{new_name}` is not a name a variable or definition can have"
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
                    spans.extend(self.uses(other, &wanted));
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
}
