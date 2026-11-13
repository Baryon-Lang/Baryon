//! Run Test lenses: a "Run test" link the editor shows above each test, which runs that test
//! alone with `baryon test -k` (roadmap 2.4.3).

use crate::Session;
use crate::compile::functions::is_test;
use database::{Db, File};
use source::Span;
use syntax::{Role, SyntaxKind};

/// One test of a file, as the editor shows a lens for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestLens {
    /// The span of the test function's name, which the lens sits above.
    pub span: Span,
    /// The test's full name, `module.test_name`, as `baryon test` reports it and `-k` matches it.
    pub name: String,
}

impl Session {
    /// A lens for each test of `file`: each top-level function decorated `@test`, in source
    /// order. A file whose path names no module, which `baryon test` could not build, has none.
    ///
    /// Cost: one walk of the file's top-level statements, on the parsed tree.
    ///
    /// ```
    /// use driver::Session;
    ///
    /// let mut session = Session::new();
    /// let text = "@test\ndef test_flux():\n    assert 2 + 2 == 4\n\ndef flux() -> f64:\n    return 1.0\n";
    /// let file = session.add_file("heat.byn", text)?;
    /// let lenses = session.test_lenses(file);
    /// assert_eq!(lenses.len(), 1);
    /// assert_eq!(lenses[0].name, "heat.test_flux");
    /// # Ok::<(), source::SourceError>(())
    /// ```
    #[must_use]
    pub fn test_lenses(&self, file: File) -> Vec<TestLens> {
        let db: &dyn Db = &self.database;
        let Ok(module) = database::module_name(db, file) else {
            return Vec::new();
        };
        let tree = db.parse(file).tree();
        let text = file.source(db).text();
        tree.children_with(tree.root(), Role::Body)
            .filter(|&statement| {
                tree.kind(statement) == SyntaxKind::FunctionDefinition
                    && is_test(tree, text, statement)
            })
            .filter_map(|statement| tree.child(statement, Role::Name))
            .map(|name| TestLens {
                span: tree.span(name),
                name: format!("{module}.{}", tree.text(name, text)),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::Session;

    /// Only top-level functions decorated `@test` get a lens, named with their module; a method
    /// decorated `@test`, which `baryon test` rejects, and an ordinary function get none.
    #[test]
    fn each_top_level_test_gets_a_lens() {
        let text = "@test\ndef test_one():\n    pass\n\n\
                    def helper():\n    pass\n\n\
                    class Grid(Struct):\n    n: i64\n\n    @test\n    def test_size(self):\n        pass\n\n\
                    @test\ndef test_two():\n    pass\n";
        let mut session = Session::new();
        let file = session.add_file("physics/heat.byn", text).unwrap();
        let names: Vec<String> = session
            .test_lenses(file)
            .into_iter()
            .map(|lens| lens.name)
            .collect();
        assert_eq!(names, ["physics.heat.test_one", "physics.heat.test_two"]);
    }
}
