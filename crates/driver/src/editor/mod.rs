//! What an editor asks about a place in the code: what the name under the cursor is
//! ([`Session::hover`]), where it is defined ([`Session::definition`]), and what could be typed
//! there ([`Session::complete`]); for a whole file, the inferred types of its variables
//! ([`Session::inlay_hints`]); and every place a name is used, to rename it
//! ([`Session::rename`]).
//!
//! Each answer is read from the queries `baryon check` already runs, so it costs a lookup in
//! cached results after the first check: the parser's tree for where the cursor is, the
//! resolver's [`Resolution`] for what a name refers to, the checker's declarations for what a
//! definition declares, its typed bodies (roadmap 1.9.2) for the type of a local variable or an
//! expression, and the `documentation` crate for documentation strings. The language server,
//! `baryon lsp`, translates the answers into the protocol (decision 0019).
//!
//! The answers are best effort: code being typed is often broken, and a question about it gets
//! what the parts that still parse and check can tell, or nothing. They never fail.
//!
//! # Example
//!
//! ```
//! use driver::Session;
//! use source::Sources;
//!
//! let mut session = Session::new();
//! let text = "def half(x: f64) -> f64:\n    \"\"\"Half of `x`.\"\"\"\n    return x / 2\n\
//!             \n\ndef main():\n    print(half(3.0))\n";
//! let file = session.add_file("main.byn", text)?;
//! let call = u32::try_from(text.rfind("half").unwrap()).unwrap();
//!
//! let hover = session.hover(file, call).unwrap();
//! assert_eq!(hover.signature, "def half(x: f64) -> f64");
//! assert_eq!(hover.documentation.as_deref(), Some("Half of `x`."));
//!
//! let definition = session.definition(file, call).unwrap();
//! assert_eq!(session.file(definition.file()).name(), "main.byn");
//! assert_eq!(definition.start(), 4);
//! # Ok::<(), source::SourceError>(())
//! ```

mod complete;
mod hints;
mod hover;
mod outline;
mod rename;

pub use complete::{Completion, CompletionKind};
pub use hints::InlayHint;
pub use hover::Hover;

use crate::{Database, Session};
use database::{Db, Definition, File, ModuleName, Package, Resolution};
use source::Span;
use syntax::{NodeId, Role, SyntaxKind, SyntaxTree};
use types::{Item, Type, TypeDb, TypeKind};

impl Session {
    /// Where the name under the cursor at byte `offset` of `file` is defined: the name of a
    /// definition, of a variable's first assignment or parameter, or of a field, method or enum
    /// member, in whichever module defines it. A module imported from the package goes to the
    /// start of its file. `None` when the cursor is not on a name, or on one the language or
    /// the standard library provides, which has no source.
    ///
    /// A cursor just after a name counts as on it, as editors place it after a double click.
    ///
    /// ```
    /// use driver::Session;
    /// use source::Sources;
    ///
    /// let mut session = Session::new();
    /// session.add_file("physics/units.byn", "C: Final = 299792458.0\n")?;
    /// let text = "import physics.units\n\ndef main():\n    print(physics.units.C)\n";
    /// let main = session.add_file("main.byn", text)?;
    ///
    /// let use_of_c = u32::try_from(text.rfind('C').unwrap()).unwrap();
    /// let found = session.definition(main, use_of_c).unwrap();
    /// assert_eq!(session.file(found.file()).name(), "physics/units.byn");
    /// assert_eq!((found.start(), found.end()), (0, 1));
    /// # Ok::<(), source::SourceError>(())
    /// ```
    #[must_use]
    pub fn definition(&self, file: File, offset: u32) -> Option<Span> {
        let cursor = Cursor::new(&self.database, self.package, file, offset);
        let (_, target) = cursor.target()?;
        target.definition(&cursor)
    }

    /// The context the questions about `file` share.
    fn cursor(&self, file: File, offset: u32) -> Cursor<'_> {
        Cursor::new(&self.database, self.package, file, offset)
    }
}

/// One place in one file, with the cached query results every question about it reads.
struct Cursor<'db> {
    /// The database, through the trait the checker's queries are read with.
    db: &'db dyn TypeDb,
    /// The package the file belongs to.
    package: Package,
    /// The file the cursor is in.
    file: File,
    /// The file's syntax tree.
    tree: &'db SyntaxTree,
    /// The file's text.
    text: &'db str,
    /// What each name in the file refers to.
    resolution: &'db Resolution,
    /// The byte offset of the cursor.
    offset: u32,
    /// The function or method the cursor is in, if any: its definition statement and item.
    function: Option<(NodeId, Item<'db>)>,
    /// The class the cursor is in, if any: its definition statement.
    class: Option<NodeId>,
}

/// What a name refers to, in the terms the questions answer in.
enum Target<'db> {
    /// A local variable or parameter, by the `Name` node of its first binding in the cursor's
    /// file.
    Local(NodeId),
    /// A top-level definition or a method.
    Item(Item<'db>),
    /// A field, method or enum member, by name, of the class `class`.
    Member {
        /// The class.
        class: Item<'db>,
        /// The member's name.
        name: String,
    },
    /// The attribute of a value that is not a class's member, such as `shape` in `a.shape`: its
    /// name and its type.
    Attribute(String, Type<'db>),
    /// A module or package of the program.
    Module(ModuleName),
    /// A standard library module or one of its members, written as the program writes it, such
    /// as `math.sqrt`.
    Standard(String),
    /// A name the language provides.
    Builtin(&'static str),
}

impl<'db> Cursor<'db> {
    /// The cursor at byte `offset` of `file`.
    fn new(database: &'db Database, package: Package, file: File, offset: u32) -> Self {
        let db: &'db dyn TypeDb = database;
        let tree = db.parse(file).tree();
        let text = file.source(db).text();
        let mut cursor = Self {
            db,
            package,
            file,
            tree,
            text,
            resolution: db.resolve(package, file),
            offset: offset.min(u32::try_from(text.len()).unwrap_or(u32::MAX)),
            function: None,
            class: None,
        };
        cursor.enclose();
        cursor
    }

    /// Finds the class and the function the cursor is in. A function's tree ends at its last
    /// statement, so a cursor on an indented line after it, where the next statement is being
    /// typed, is still in it.
    fn enclose(&mut self) {
        let indented = self.text[..self.offset as usize]
            .rsplit('\n')
            .next()
            .is_some_and(|line| line.starts_with([' ', '\t']));
        let inside = |statement: NodeId| {
            let node = self.tree.node(statement);
            node.start() <= self.offset && (self.offset <= node.end() || indented)
        };
        let last_before = |parent: NodeId| {
            self.tree
                .children_with(parent, Role::Body)
                .take_while(|&statement| self.tree.node(statement).start() <= self.offset)
                .last()
                .filter(|&statement| inside(statement))
        };
        let Some(mut statement) = last_before(self.tree.root()) else {
            return;
        };
        if self.tree.kind(statement) == SyntaxKind::ClassDefinition {
            self.class = Some(statement);
            match last_before(statement) {
                Some(method) => statement = method,
                None => return,
            }
        }
        if self.tree.kind(statement) == SyntaxKind::FunctionDefinition
            && let Some(name) = self.tree.child(statement, Role::Name)
        {
            let item = Item::new(self.db, self.package, self.file, name);
            self.function = Some((statement, item));
        }
    }

    /// The `Name` node under the cursor: the one the cursor is inside, or else the one that
    /// ends where the cursor is. An empty name, which the parser leaves after `x.`, counts.
    ///
    /// Cost: a walk down the nodes whose span reaches the cursor.
    fn name(&self) -> Option<NodeId> {
        let mut inside = None;
        let mut ending = None;
        let mut pending = vec![self.tree.root()];
        while let Some(node) = pending.pop() {
            let span = self.tree.node(node);
            if span.start() > self.offset || span.end() < self.offset {
                continue;
            }
            if self.tree.kind(node) == SyntaxKind::Name {
                if self.offset < span.end() {
                    inside = Some(node);
                } else {
                    ending = Some(node);
                }
            }
            pending.extend(self.tree.children(node));
        }
        inside.or(ending)
    }

    /// The `Attribute` node whose attribute is `name`, as `x.y` is for `y`.
    fn attribute_of(&self, name: NodeId) -> Option<NodeId> {
        if self.tree.node(name).role() != Role::Attribute {
            return None;
        }
        let parent = self.parent(name)?;
        (self.tree.kind(parent) == SyntaxKind::Attribute).then_some(parent)
    }

    /// The node that has `node` as a direct child. In preorder, that child of each ancestor is
    /// its last child that does not come after `node`.
    fn parent(&self, node: NodeId) -> Option<NodeId> {
        let mut parent = self.tree.root();
        loop {
            let child = self
                .tree
                .children(parent)
                .take_while(|child| child.index() <= node.index())
                .last()?;
            if child == node {
                return Some(parent);
            }
            parent = child;
        }
    }

    /// The name under the cursor, with its span and what it refers to.
    fn target(&self) -> Option<(Span, Target<'db>)> {
        let name = self.name()?;
        let span = self.tree.span(name);
        // A definition's own name first: the resolver binds a field like a local variable.
        let target = match self.definition_name(name) {
            Some(target) => target,
            None => match self.resolution.definition(name) {
                Some(definition) => self.resolved(definition),
                None => self.member(name)?,
            },
        };
        Some((span, target))
    }

    /// A resolved name's target.
    fn resolved(&self, definition: &Definition) -> Target<'db> {
        match definition {
            Definition::Local(binding) => Target::Local(*binding),
            Definition::Item(file, name) => {
                Target::Item(Item::new(self.db, self.package, *file, *name))
            }
            Definition::Module(module) => Target::Module(module.clone()),
            Definition::Standard { module, member } => Target::Standard(match member {
                Some(member) => format!("{module}.{member}"),
                None => (*module).to_owned(),
            }),
            Definition::Builtin(builtin) => Target::Builtin(builtin),
        }
    }

    /// The target of `name` when it is the attribute in `x.name`: a member of `x`'s class, or
    /// an attribute with the type the checker found.
    fn member(&self, name: NodeId) -> Option<Target<'db>> {
        let attribute = self.attribute_of(name)?;
        let base = self.tree.child(attribute, Role::Value)?;
        let wanted = self.tree.text(name, self.text).to_owned();
        if let Some(class) = self.class_of(base) {
            return Some(Target::Member {
                class,
                name: wanted,
            });
        }
        let ty = self.expression(attribute)?;
        Some(Target::Attribute(wanted, ty))
    }

    /// The target of a name that is itself a definition: the name after `def` or `class`, or
    /// the target of a constant, alias, field or enum member.
    fn definition_name(&self, name: NodeId) -> Option<Target<'db>> {
        let item = |name| Item::new(self.db, self.package, self.file, name);
        let top_level = outline::definitions(self.tree, self.text, self.tree.root());
        if top_level.iter().any(|definition| definition.name == name) {
            return Some(Target::Item(item(name)));
        }
        let class = self.class?;
        let member = outline::definitions(self.tree, self.text, class)
            .into_iter()
            .find(|definition| definition.name == name)?;
        Some(match member.kind {
            outline::Kind::Function => Target::Item(item(name)),
            outline::Kind::Class | outline::Kind::Assignment => Target::Member {
                class: item(self.tree.child(class, Role::Name)?),
                name: member.text,
            },
        })
    }

    /// The class an expression's value belongs to, or that it names: a struct or enum value's
    /// class, or the class a name like `Phase` in `Phase.GAS` refers to.
    fn class_of(&self, expression: NodeId) -> Option<Item<'db>> {
        if let Some(ty) = self.expression(expression)
            && let TypeKind::Struct(nominal) | TypeKind::Enum(nominal) = ty.kind(self.db)
        {
            return Some(Item::new(self.db, self.package, nominal.file, nominal.name));
        }
        let name = match self.tree.kind(expression) {
            SyntaxKind::Name => expression,
            SyntaxKind::Attribute => self.tree.child(expression, Role::Attribute)?,
            _ => return None,
        };
        match self.resolution.definition(name)? {
            Definition::Item(file, class) => {
                let tree = self.db.parse(*file).tree();
                let statement = outline::statement_of(tree, *class)?;
                (tree.kind(statement) == SyntaxKind::ClassDefinition)
                    .then(|| Item::new(self.db, self.package, *file, *class))
            }
            _ => None,
        }
    }

    /// The type the checker found for the expression `node` of the cursor's function.
    fn expression(&self, node: NodeId) -> Option<Type<'db>> {
        let (_, function) = self.function?;
        self.db
            .body(function)
            .expression(node)
            .filter(|ty| !matches!(ty.kind(self.db), TypeKind::Unknown))
    }

    /// The declared type of the local variable or parameter first bound at `binding`.
    fn local(&self, binding: NodeId) -> Option<Type<'db>> {
        let (_, function) = self.function?;
        let body = self.db.body(function);
        body.local(binding)
            .or_else(|| body.expression(binding))
            .filter(|ty| !matches!(ty.kind(self.db), TypeKind::Unknown))
    }
}

impl<'db> Target<'db> {
    /// Where the target is defined.
    fn definition(&self, cursor: &Cursor<'db>) -> Option<Span> {
        let db = cursor.db;
        match self {
            Target::Local(binding) => Some(cursor.tree.span(*binding)),
            Target::Item(item) => Some(db.parse(item.file(db)).tree().span(item.name(db))),
            Target::Member { class, name } => {
                let file = class.file(db);
                let tree = db.parse(file).tree();
                let statement = outline::statement_of(tree, class.name(db))?;
                outline::definitions(tree, file.source(db).text(), statement)
                    .into_iter()
                    .find(|member| member.text == *name)
                    .map(|member| tree.span(member.name))
            }
            Target::Module(module) => {
                let file = database::modules(db, cursor.package).get(module)?;
                Some(Span::new(file.id(db), 0, 0))
            }
            Target::Attribute(..) | Target::Standard(_) | Target::Builtin(_) => None,
        }
    }
}

/// Whether a name is public: not starting with `_`, as `baryon doc` and imports decide it.
fn is_public(name: &str) -> bool {
    !name.starts_with('_')
}

/// The module name of `file`, written with dots, or its file name when it has none.
fn module_name(db: &dyn Db, file: File) -> String {
    database::module_name(db, file)
        .as_ref()
        .map_or_else(|_| file.source(db).name().to_owned(), ToString::to_string)
}

#[cfg(test)]
mod tests;
