//! What each name in a file refers to: the result of name resolution, read through
//! [`Db::resolve`](crate::Db::resolve).
//!
//! Name resolution runs after parsing. For every name a file reads, such as `x` in `y = x + 1`,
//! it finds the definition the name refers to: a local variable or parameter, a module-level
//! function, class or constant of this or another module, an imported module, a standard
//! library name, or a name the language itself provides ([`BUILTINS`]). The type checker, the
//! language server's "go to definition" and "rename" all start from this map.

use crate::{File, ModuleName};
use diagnostics::Diagnostic;
use syntax::NodeId;

/// The names every module can use without an import: the names the language adds and the
/// Python built-ins it keeps (language reference, "Syntax", "Names the language adds"), the
/// standard traits (language reference, "Types", "Traits"), and the names decision 0031 adds:
/// `Protocol`, the base of a user trait, `Embed`, which embeds a struct in another, and `Dyn`,
/// reserved for trait objects (roadmap 3.5).
pub const BUILTINS: [&str; 56] = [
    "f64",
    "f32",
    "i64",
    "i32",
    "i16",
    "i8",
    "u64",
    "u32",
    "u16",
    "u8",
    "complex64",
    "complex128",
    "Final",
    "Struct",
    "Copy",
    "Array",
    "Fixed",
    "InOut",
    "Owned",
    "Self",
    "Raises",
    "prange",
    "simd",
    "unsafe",
    "test",
    "bench",
    "layout",
    "Callable",
    "print",
    "len",
    "range",
    "round",
    "abs",
    "min",
    "max",
    "sum",
    "enumerate",
    "zip",
    "sorted",
    "reversed",
    "int",
    "float",
    "complex",
    "bool",
    "str",
    "list",
    "dict",
    "tuple",
    "Exception",
    "Enum",
    "Number",
    "Float",
    "VectorSpace",
    "Protocol",
    "Embed",
    "Dyn",
];

/// The standard library's modules. Until the standard library is written (roadmap 1.13),
/// importing one of these is accepted and its members are not checked.
pub const STANDARD_MODULES: [&str; 2] = ["math", "random"];

/// What a name refers to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Definition {
    /// A name bound inside a function, a comprehension or a class body: a parameter, a
    /// variable, a type parameter or a field. The id is the `Name` node of its first binding in
    /// the same file, so every use of one variable carries the same id.
    Local(NodeId),
    /// A function, class, constant or type alias defined at the top level of a module. The id
    /// is the `Name` node of the definition, in the given file.
    Item(File, NodeId),
    /// A module or package of the program, named by an import, such as `physics` after
    /// `import physics.nbody`.
    Module(ModuleName),
    /// A standard library module, or one of its members when `member` is set. Not checked until
    /// the standard library exists (roadmap 1.13).
    Standard {
        /// The module, one of [`STANDARD_MODULES`].
        module: &'static str,
        /// The member, such as `sqrt` in `math.sqrt`.
        member: Option<String>,
    },
    /// A name the language provides, one of [`BUILTINS`].
    Builtin(&'static str),
}

/// The names of one file, resolved: the definition each name refers to, and the errors found.
///
/// ```
/// use database::{Definition, Resolution};
/// use syntax::{Role, SyntaxKind, TreeBuilder};
/// use source::FileId;
///
/// // `print`: a module whose only name is node 2.
/// let mut builder = TreeBuilder::new(FileId::from_index(0));
/// builder.open(SyntaxKind::Module, Role::Root, 0, 5);
/// builder.open(SyntaxKind::ExpressionStatement, Role::Body, 0, 5);
/// builder.leaf(SyntaxKind::Name, Role::Value, 0, 5);
/// builder.close();
/// builder.close();
/// let tree = builder.finish();
/// let name = tree.children(tree.children(tree.root()).next().unwrap()).next().unwrap();
///
/// let resolution = Resolution::new(vec![(name, Definition::Builtin("print"))], Vec::new());
/// assert_eq!(resolution.definition(name), Some(&Definition::Builtin("print")));
/// assert_eq!(resolution.definition(tree.root()), None);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolution {
    /// Each resolved `Name` node and its definition, sorted by node.
    references: Vec<(NodeId, Definition)>,
    /// One error per name that refers to nothing, import that finds nothing, or definition
    /// that clashes with another, in source order.
    diagnostics: Vec<Diagnostic>,
}

impl Resolution {
    /// Bundles the resolved names, in any order, with the errors found resolving them.
    #[must_use]
    pub fn new(mut references: Vec<(NodeId, Definition)>, diagnostics: Vec<Diagnostic>) -> Self {
        references.sort_by_key(|&(node, _)| node);
        Self {
            references,
            diagnostics,
        }
    }

    /// What the `Name` node `name` refers to, or `None` if it is not a resolved name: a name
    /// that refers to nothing, an attribute such as `y` in `x.y` that only types can resolve,
    /// a keyword argument's name, or any other node.
    ///
    /// Cost: a binary search, `O(log n)` in the number of names in the file.
    #[must_use]
    pub fn definition(&self, name: NodeId) -> Option<&Definition> {
        self.references
            .binary_search_by_key(&name, |&(node, _)| node)
            .ok()
            .map(|index| &self.references[index].1)
    }

    /// Every resolved name and its definition, in source order.
    #[must_use]
    pub fn references(&self) -> &[(NodeId, Definition)] {
        &self.references
    }

    /// One error per name that refers to nothing, import that finds nothing, or definition that
    /// clashes with another, in source order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}
