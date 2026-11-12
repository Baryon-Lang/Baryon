//! What a documented [`Item`] tells its reader: its name, its signature as written, its
//! documentation string and, for a class, its members. The struct is declared beside
//! [`ModuleDocumentation`](crate::ModuleDocumentation) in the crate root; its accessors live
//! here, with an example each.

use crate::Item;

impl Item {
    /// The name it is declared with.
    ///
    /// ```
    /// # use source::SourceMap;
    /// # use syntax::{Role, SyntaxKind, TreeBuilder};
    /// let text = "class Atoms(Struct):\n    \"\"\"Particles.\"\"\"\n\n    n: i64\n    \"\"\"How many.\"\"\"\n";
    /// # let mut sources = SourceMap::new();
    /// # let mut builder = TreeBuilder::new(sources.add("atoms.byn", text)?);
    /// # builder.open(SyntaxKind::Module, Role::Root, 0, 74);
    /// # builder.open(SyntaxKind::ClassDefinition, Role::Body, 0, 73);
    /// # builder.leaf(SyntaxKind::Name, Role::Name, 6, 11);
    /// # builder.open(SyntaxKind::Arguments, Role::Arguments, 11, 19);
    /// # builder.leaf(SyntaxKind::Name, Role::Item, 12, 18);
    /// # builder.close();
    /// # builder.open(SyntaxKind::ExpressionStatement, Role::Body, 25, 41);
    /// # builder.leaf(SyntaxKind::StringLiteral, Role::Value, 25, 41);
    /// # builder.close();
    /// # builder.open(SyntaxKind::AnnotatedAssign, Role::Body, 47, 53);
    /// # builder.leaf(SyntaxKind::Name, Role::Target, 47, 48);
    /// # builder.leaf(SyntaxKind::Name, Role::Annotation, 50, 53);
    /// # builder.close();
    /// # builder.open(SyntaxKind::ExpressionStatement, Role::Body, 58, 73);
    /// # builder.leaf(SyntaxKind::StringLiteral, Role::Value, 58, 73);
    /// # builder.close();
    /// # builder.close();
    /// # builder.close();
    /// let module = documentation::document(&builder.finish(), text, "atoms");
    ///
    /// assert_eq!(module.items()[0].name(), "Atoms");
    /// assert_eq!(module.items()[0].members()[0].name(), "n");
    /// # Ok::<(), source::SourceError>(())
    /// ```
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Its declaration as written, joined onto one line: `def step(atoms: InOut[Atoms]) -> None`
    /// for a function, `class Atoms(Struct)` for a class, the whole statement for a constant,
    /// type alias or field. Comments, the closing colon and the trailing comma of a parameter
    /// list written one parameter per line are left out. A definition's decorators come first,
    /// each on its own line.
    ///
    /// ```
    /// # use source::SourceMap;
    /// # use syntax::{Role, SyntaxKind, TreeBuilder};
    /// let text = "\"\"\"Heat flow.\n\nExplicit finite differences.\"\"\"\nALPHA: Final = 0.1\n\"\"\"Diffusivity.\"\"\"\n";
    /// # let mut sources = SourceMap::new();
    /// # let mut builder = TreeBuilder::new(sources.add("physics/heat.byn", text)?);
    /// # builder.open(SyntaxKind::Module, Role::Root, 0, 85);
    /// # builder.open(SyntaxKind::ExpressionStatement, Role::Body, 0, 46);
    /// # builder.leaf(SyntaxKind::StringLiteral, Role::Value, 0, 46);
    /// # builder.close();
    /// # builder.open(SyntaxKind::AnnotatedAssign, Role::Body, 47, 65);
    /// # builder.leaf(SyntaxKind::Name, Role::Target, 47, 52);
    /// # builder.leaf(SyntaxKind::Name, Role::Annotation, 54, 59);
    /// # builder.leaf(SyntaxKind::Float, Role::Value, 62, 65);
    /// # builder.close();
    /// # builder.open(SyntaxKind::ExpressionStatement, Role::Body, 66, 84);
    /// # builder.leaf(SyntaxKind::StringLiteral, Role::Value, 66, 84);
    /// # builder.close();
    /// # builder.close();
    /// let module = documentation::document(&builder.finish(), text, "physics.heat");
    ///
    /// assert_eq!(module.items()[0].signature(), "ALPHA: Final = 0.1");
    /// # Ok::<(), source::SourceError>(())
    /// ```
    #[must_use]
    pub fn signature(&self) -> &str {
        &self.signature
    }

    /// Its documentation, cleaned of its source indentation; `None` when it has none.
    ///
    /// ```
    /// # use source::SourceMap;
    /// # use syntax::{Role, SyntaxKind, TreeBuilder};
    /// let text = "class Atoms(Struct):\n    \"\"\"Particles.\"\"\"\n\n    n: i64\n    \"\"\"How many.\"\"\"\n";
    /// # let mut sources = SourceMap::new();
    /// # let mut builder = TreeBuilder::new(sources.add("atoms.byn", text)?);
    /// # builder.open(SyntaxKind::Module, Role::Root, 0, 74);
    /// # builder.open(SyntaxKind::ClassDefinition, Role::Body, 0, 73);
    /// # builder.leaf(SyntaxKind::Name, Role::Name, 6, 11);
    /// # builder.open(SyntaxKind::Arguments, Role::Arguments, 11, 19);
    /// # builder.leaf(SyntaxKind::Name, Role::Item, 12, 18);
    /// # builder.close();
    /// # builder.open(SyntaxKind::ExpressionStatement, Role::Body, 25, 41);
    /// # builder.leaf(SyntaxKind::StringLiteral, Role::Value, 25, 41);
    /// # builder.close();
    /// # builder.open(SyntaxKind::AnnotatedAssign, Role::Body, 47, 53);
    /// # builder.leaf(SyntaxKind::Name, Role::Target, 47, 48);
    /// # builder.leaf(SyntaxKind::Name, Role::Annotation, 50, 53);
    /// # builder.close();
    /// # builder.open(SyntaxKind::ExpressionStatement, Role::Body, 58, 73);
    /// # builder.leaf(SyntaxKind::StringLiteral, Role::Value, 58, 73);
    /// # builder.close();
    /// # builder.close();
    /// # builder.close();
    /// let module = documentation::document(&builder.finish(), text, "atoms");
    ///
    /// // The documentation string inside the class body documents the class.
    /// assert_eq!(module.items()[0].text(), Some("Particles."));
    /// # Ok::<(), source::SourceError>(())
    /// ```
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    /// A class's public fields and methods, in source order; empty for anything else.
    ///
    /// ```
    /// # use source::SourceMap;
    /// # use syntax::{Role, SyntaxKind, TreeBuilder};
    /// let text = "class Atoms(Struct):\n    \"\"\"Particles.\"\"\"\n\n    n: i64\n    \"\"\"How many.\"\"\"\n";
    /// # let mut sources = SourceMap::new();
    /// # let mut builder = TreeBuilder::new(sources.add("atoms.byn", text)?);
    /// # builder.open(SyntaxKind::Module, Role::Root, 0, 74);
    /// # builder.open(SyntaxKind::ClassDefinition, Role::Body, 0, 73);
    /// # builder.leaf(SyntaxKind::Name, Role::Name, 6, 11);
    /// # builder.open(SyntaxKind::Arguments, Role::Arguments, 11, 19);
    /// # builder.leaf(SyntaxKind::Name, Role::Item, 12, 18);
    /// # builder.close();
    /// # builder.open(SyntaxKind::ExpressionStatement, Role::Body, 25, 41);
    /// # builder.leaf(SyntaxKind::StringLiteral, Role::Value, 25, 41);
    /// # builder.close();
    /// # builder.open(SyntaxKind::AnnotatedAssign, Role::Body, 47, 53);
    /// # builder.leaf(SyntaxKind::Name, Role::Target, 47, 48);
    /// # builder.leaf(SyntaxKind::Name, Role::Annotation, 50, 53);
    /// # builder.close();
    /// # builder.open(SyntaxKind::ExpressionStatement, Role::Body, 58, 73);
    /// # builder.leaf(SyntaxKind::StringLiteral, Role::Value, 58, 73);
    /// # builder.close();
    /// # builder.close();
    /// # builder.close();
    /// let module = documentation::document(&builder.finish(), text, "atoms");
    ///
    /// let atoms = &module.items()[0];
    /// assert_eq!(atoms.members().len(), 1);
    /// assert_eq!(atoms.members()[0].signature(), "n: i64");
    /// assert!(atoms.members()[0].members().is_empty());
    /// # Ok::<(), source::SourceError>(())
    /// ```
    #[must_use]
    pub fn members(&self) -> &[Item] {
        &self.members
    }
}
