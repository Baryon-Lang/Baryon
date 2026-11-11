//! The classes of the standard library's own Baryon source (decision 0038) that the checker
//! names itself: the error a built-in raises, such as `OSError` from `open` (decision 0036) and
//! `ValueError` from `int(text)` (decision 0037).

use super::Body;
use database::LIBRARY_ROOT;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Nominal, Type, TypeKind};

impl<'db> Body<'db> {
    /// The class `name` that `<std>/builtins.byn` defines at its top level, such as `OSError`,
    /// when the package has that file.
    pub(super) fn library_class(&self, name: &str) -> Option<Type<'db>> {
        self.library_class_in("builtins", name)
    }

    /// The class `name` that the library's file for the standard module `module` defines at its
    /// top level, such as `Path` in `<std>/pathlib.byn` or `CompletedProcess` in
    /// `<std>/subprocess.byn`, when the package has that file; `builtins` names the file of the
    /// names every module can use. It reads the file's syntax tree, which the database has
    /// already parsed for the package, so the cost is one pass over its few top-level
    /// statements.
    pub(super) fn library_class_in(&self, module: &str, name: &str) -> Option<Type<'db>> {
        let file = database::modules(self.db, self.lowerer.package).library(module)?;
        debug_assert!(file.source(self.db).name().starts_with(LIBRARY_ROOT));
        let tree = self.db.parse(file).tree();
        let text = file.source(self.db).text();
        let class = tree
            .children_with(tree.root(), Role::Body)
            .filter(|&statement| tree.kind(statement) == SyntaxKind::ClassDefinition)
            .filter_map(|class| tree.child(class, Role::Name))
            .find(|&class| tree.text(class, text) == name)?;
        Some(Type::new(
            self.db,
            TypeKind::Struct(Nominal {
                file,
                name: class,
                arguments: Vec::new(),
            }),
        ))
    }

    /// `Raises[value, E]`, with `E` the library's error class `name`, for a built-in call `node`
    /// that can fail with it. The class is missing only when the library's files were left out
    /// of the package, which the driver never does, so that is reported at `node`.
    pub(super) fn raises_library_error(
        &mut self,
        node: NodeId,
        value: Type<'db>,
        name: &str,
    ) -> Type<'db> {
        let Some(error) = self.library_class(name) else {
            let message = format!("`{name}` is missing from the standard library");
            return self.lowerer.error(
                node,
                &message,
                "cannot be typed",
                Some("the compiler adds the library's files to every package (decision 0038)"),
            );
        };
        Type::new(
            self.db,
            TypeKind::Fallible {
                value,
                errors: vec![error],
            },
        )
    }
}
