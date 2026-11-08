//! The mid-level IR of a function, for tests and for debugging the compiler: lowering wired to
//! the session's database.

use crate::Session;
use database::Db;
use diagnostics::Diagnostic;
use syntax::{Role, SyntaxKind};
use types::Item;

impl Session {
    /// The mid-level IR of the top-level function `name`, as lowering builds it (roadmap 1.9.3),
    /// in the IR's text form. `None` when no file defines a function of
    /// that name. Call it on a package that [`check`](Self::check)s without errors.
    ///
    /// ```
    /// use driver::Session;
    ///
    /// let mut session = Session::new();
    /// session.add_file("main.byn", "def half(x: f64) -> f64:\n    return x / 2\n")?;
    /// assert!(session.check().is_empty());
    /// let text = session.mir("half").unwrap().unwrap();
    /// assert!(text.contains("_0 = Divide(copy _1, 2.0_f64);"), "{text}");
    /// # Ok::<(), source::SourceError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Fails with one error per construct lowering does not handle yet, and with no errors
    /// when the function has type errors.
    #[must_use]
    pub fn mir(&self, name: &str) -> Option<Result<String, Vec<Diagnostic>>> {
        let database = &self.database;
        self.files.iter().find_map(|&file| {
            let tree = database.parse(file).tree();
            let text = file.source(database).text();
            let function = tree
                .children_with(tree.root(), Role::Body)
                .filter(|&statement| tree.kind(statement) == SyntaxKind::FunctionDefinition)
                .filter_map(|statement| tree.child(statement, Role::Name))
                .find(|&found| tree.text(found, text) == name)?;
            let item = Item::new(database, self.package, file, function);
            Some(lowering::lower(database, item).map(|body| body.display(database).to_string()))
        })
    }
}
