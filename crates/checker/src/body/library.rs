//! The classes of the standard library's own Baryon source (decision 0038) that the checker
//! names itself: the error a built-in raises, such as `OSError` from `open` (decision 0036) and
//! `ValueError` from `int(text)` (decision 0037), and the library functions a program calls as
//! methods of a library class, such as a FITS `Header`'s `header["EXPTIME"]` (decision 0044).

use super::Body;
use super::calls::Arguments;
use super::exclusive::Passed;
use database::{File, LIBRARY_ROOT};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Callee, Declaration, Item, Key, Mode, Nominal, Type, TypeKind};

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

    /// The function `name` that the library's file `module` defines at its top level, such as
    /// `loadtxt` in `<std>/arrays.byn` (decision 0043): its file and its name's node, which
    /// together name its item. `None` when the package has no such file or function. Costs one
    /// pass over the file's top-level statements, as [`Self::library_class_in`] does.
    pub(super) fn library_function(&self, module: &str, name: &str) -> Option<(File, NodeId)> {
        let file = database::modules(self.db, self.lowerer.package).library(module)?;
        let tree = self.db.parse(file).tree();
        let text = file.source(self.db).text();
        let function = tree
            .children_with(tree.root(), Role::Body)
            .filter(|&statement| tree.kind(statement) == SyntaxKind::FunctionDefinition)
            .filter_map(|function| tree.child(function, Role::Name))
            .find(|&function| tree.text(function, text) == name)?;
        Some((file, function))
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
        self.raises_library_errors(node, value, &[name])
    }

    /// `Raises[value, A | B | ...]`, with the library's error classes `names` in that order, for
    /// a built-in call `node` that can fail with any of them. The order is the runtime's: the
    /// error a call writes carries the position of its class in `names` as its tag, so a
    /// subscripted `NetCDF` read lists `OSError`, `IndexError` and `ValueError` as the runtime
    /// numbers them 0, 1 and 2. A missing class is reported at `node`, as above.
    pub(super) fn raises_library_errors(
        &mut self,
        node: NodeId,
        value: Type<'db>,
        names: &[&str],
    ) -> Type<'db> {
        let mut errors = Vec::with_capacity(names.len());
        for name in names {
            let Some(error) = self.library_class(name) else {
                let message = format!("`{name}` is missing from the standard library");
                return self.lowerer.error(
                    node,
                    &message,
                    "cannot be typed",
                    Some("the compiler adds the library's files to every package (decision 0038)"),
                );
            };
            errors.push(error);
        }
        Type::new(self.db, TypeKind::Fallible { value, errors })
    }

    /// The call `node` of the library function `function` of the standard module `module`'s
    /// file, which a program writes as a method of the library class `class`: the value
    /// `receiver` of type `base` is its first argument, then `arguments`. The checker checks it
    /// as a call of that function and records it at `node`, so lowering lowers it as any call
    /// of a library function: `header["EXPTIME"]` of a FITS `Header` (decision 0044), or
    /// `ds.getncattr("title")` of a `NetCDF` `Dataset` (decision 0047).
    pub(super) fn library_method(
        &mut self,
        node: NodeId,
        (module, class, function): (&str, &str, &str),
        (receiver, base): (NodeId, Type<'db>),
        arguments: &Arguments,
    ) -> Type<'db> {
        let signature = self
            .library_function(module, function)
            .map(|(file, name)| Item::new(self.db, self.lowerer.package, file, name))
            .and_then(|item| match self.db.declaration(item).declaration.clone() {
                Declaration::Function(signature) => Some((item, signature)),
                _ => None,
            });
        let Some((item, signature)) = signature else {
            self.untyped(arguments);
            let message = format!("`{module}.{function}` is missing from the standard library");
            return self.lowerer.error(
                node,
                &message,
                "cannot be typed",
                Some("the compiler adds the library's files to every package (decision 0038)"),
            );
        };
        let mode = signature
            .parameters
            .first()
            .map_or(Mode::Borrow, |parameter| parameter.mode);
        let receiver = Passed {
            node: receiver,
            mode,
            ty: base,
        };
        let ty = self.check_call(
            node,
            class,
            &signature,
            Some(receiver),
            Callee::Function(item),
            arguments,
        );
        debug_assert!(self.typed.calls.contains_key(&Key(node)));
        ty
    }
}
