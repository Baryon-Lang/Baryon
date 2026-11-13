//! [`Written`]: definitions a [`Module`] wrote, taken out of it so another module can take
//! them in (roadmap 2.2.2). A compiler that keeps each function's text between builds writes
//! each function in a module of its own and gathers the ones it kept or wrote into the
//! program's module; a definition numbers its own constants, so its text is the same in any
//! module.

use crate::module::{Definition, Module};
use std::collections::BTreeMap;

impl Module<'_, '_> {
    /// The definitions written so far, apart from the module, to be added to another with
    /// [`absorb`](Self::absorb). A compiler that keeps each function's definition between builds
    /// writes each into a module of its own, and gathers the ones it kept or wrote into the
    /// program's module (roadmap 2.2.2): a definition numbers its own constants, so its text
    /// is the same in any module.
    ///
    /// ```
    /// # #[salsa::db]
    /// # #[derive(Clone, Default)]
    /// # struct Database { storage: salsa::Storage<Self> }
    /// # #[salsa::db]
    /// # impl salsa::Database for Database {}
    /// # #[salsa::db]
    /// # impl database::Db for Database {
    /// #     fn parse(&self, _: database::File) -> &database::ParsedFile { unreachable!() }
    /// #     fn resolve(&self, _: database::Package, _: database::File) -> &database::Resolution {
    /// #         unreachable!()
    /// #     }
    /// # }
    /// let database = Database::default();
    /// let sources = source::SourceMap::new();
    /// let declared = |_: &types::Nominal<'_>| types::Declared::Struct(Vec::new());
    /// let symbol = |_: &types::Item<'_>, _: &[types::Type<'_>]| String::new();
    /// let names = |_: &types::Nominal<'_>| llvm_backend::ClassNames::default();
    /// let mut apart =
    ///     llvm_backend::Module::new(&database, &sources, &declared, &names, &symbol);
    /// apart.entry("main.main", &[])?;
    /// let written = apart.take();
    /// let mut module =
    ///     llvm_backend::Module::new(&database, &sources, &declared, &names, &symbol);
    /// module.absorb(&written);
    /// assert!(module.finish().starts_with("define i32 @main()"));
    /// # Ok::<(), llvm_backend::Unsupported>(())
    /// ```
    #[must_use]
    pub fn take(self) -> Written {
        Written {
            first_file: self.constants.first_file(),
            definitions: self.definitions,
            declarations: self.declarations,
        }
    }

    /// Adds the definitions in `written`, which another module wrote and
    /// [`take`](Self::take) took, after the ones this module holds, as if this module had
    /// written them. Both modules must be for the same program and settings.
    ///
    /// Cost: a copy of the definitions' text.
    pub fn absorb(&mut self, written: &Written) {
        self.definitions.extend(written.definitions.iter().cloned());
        self.declarations.extend(
            written
                .declarations
                .iter()
                .map(|(symbol, declaration)| (symbol.clone(), declaration.clone())),
        );
        self.constants.adopt(written.first_file);
    }
}

/// Definitions a [`Module`] wrote, taken out of it with [`Module::take`] so another module can
/// [`absorb`](Module::absorb) them. Equal when every definition's text, constants and calls
/// are, which is what lets a query system keep one between builds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Written {
    /// The definitions, in the order they were written.
    definitions: Vec<Definition>,
    /// The declaration of each symbol they call, by symbol.
    declarations: BTreeMap<String, String>,
    /// The first function's file, which the compile unit names.
    first_file: Option<source::FileId>,
}
