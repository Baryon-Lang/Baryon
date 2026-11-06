//! Tests of type display and layout, over a small database.

use crate::{Declared, Layout, LayoutError, Nominal, Primitive, Shape, Type, TypeKind, layout};
use database::{Db, File, Package, ParsedFile, Resolution};
use source::{FileId, SourceFile};
use syntax::{NodeId, Role, SyntaxKind, TreeBuilder};

/// The `Name` node of a one-name file.
fn name_node(database: &Database, file: File) -> NodeId {
    let tree = database.parse(file).tree();
    let statement = tree.children(tree.root()).next().unwrap();
    tree.children(statement).next().unwrap()
}

/// A database whose files each hold one name, such as `Particle`, standing for a class.
#[salsa::db]
#[derive(Clone, Default)]
struct Database {
    /// Salsa's inputs and caches.
    storage: salsa::Storage<Self>,
}

#[salsa::db]
impl salsa::Database for Database {}

#[salsa::db]
impl Db for Database {
    fn parse(&self, file: File) -> &ParsedFile {
        one_name(self, file)
    }

    fn resolve(&self, _: Package, _: File) -> &Resolution {
        unreachable!("type tests never resolve names")
    }
}

/// The tree of a file that is one name: a module holding an expression statement.
#[salsa::tracked(returns(ref), no_eq)]
fn one_name(db: &dyn Db, file: File) -> ParsedFile {
    let end = u32::try_from(file.source(db).text().len()).unwrap();
    let mut builder = TreeBuilder::new(file.id(db));
    builder.open(SyntaxKind::Module, Role::Root, 0, end);
    builder.open(SyntaxKind::ExpressionStatement, Role::Body, 0, end);
    builder.leaf(SyntaxKind::Name, Role::Value, 0, end);
    builder.close();
    builder.close();
    ParsedFile::new(builder.finish(), Vec::new())
}

/// A class type named `name`, declared in a file of its own.
fn class<'db>(database: &'db Database, name: &str, enumeration: bool) -> Type<'db> {
    let file = File::new(
        database,
        FileId::from_index(0),
        SourceFile::new("a.npl", name).unwrap(),
    );
    let nominal = Nominal {
        file,
        name: name_node(database, file),
        arguments: Vec::new(),
    };
    let kind = if enumeration {
        TypeKind::Enum(nominal)
    } else {
        TypeKind::Struct(nominal)
    };
    Type::new(database, kind)
}

fn primitive<'db>(database: &'db Database, name: &str) -> Type<'db> {
    Type::new(
        database,
        TypeKind::Primitive(Primitive::from_name(name).unwrap()),
    )
}

fn fixed<'db>(database: &'db Database, element: Type<'db>, lengths: Vec<u64>) -> Type<'db> {
    Type::new(
        database,
        TypeKind::Fixed {
            element,
            shape: Shape::new(lengths).unwrap(),
        },
    )
}

#[test]
fn types_display_as_programs_spell_them() {
    let database = Database::default();
    let f64 = primitive(&database, "f64");
    let int = primitive(&database, "int");
    let shown = |kind| Type::new(&database, kind).display(&database).to_string();
    assert_eq!(
        shown(TypeKind::Array {
            element: f64,
            rank: 3
        }),
        "Array[f64, 3]"
    );
    assert_eq!(
        fixed(&database, f64, vec![3, 3])
            .display(&database)
            .to_string(),
        "Fixed[f64, (3, 3)]"
    );
    assert_eq!(shown(TypeKind::List(int)), "list[i64]");
    assert_eq!(shown(TypeKind::Tuple(vec![int, f64])), "tuple[i64, f64]");
    assert_eq!(shown(TypeKind::Optional(f64)), "f64 | None");
    assert_eq!(
        shown(TypeKind::Callable {
            parameters: vec![f64, int],
            returns: f64
        }),
        "Callable[[f64, i64], f64]"
    );
    let error = class(&database, "SolverError", false);
    assert_eq!(
        shown(TypeKind::Fallible {
            value: f64,
            errors: vec![error]
        }),
        "Raises[f64, SolverError]"
    );
    assert_eq!(
        class(&database, "Particle", false)
            .display(&database)
            .to_string(),
        "Particle"
    );
}

#[test]
fn equal_types_are_one_id() {
    let database = Database::default();
    let f64 = primitive(&database, "f64");
    assert_eq!(f64, primitive(&database, "float"));
    assert_ne!(f64, primitive(&database, "f32"));
    assert_eq!(
        fixed(&database, f64, vec![3]),
        fixed(&database, f64, vec![3])
    );
}

/// The layout of `ty`, where the class `Boundary` is an enum of 3 variants and every other class
/// is a struct of a `u8` then an `f64`.
fn layout_of<'db>(database: &'db Database, ty: Type<'db>) -> Result<(u64, u64), LayoutError> {
    let byte = primitive(database, "u8");
    let double = primitive(database, "f64");
    let declared = |nominal: &Nominal<'db>| {
        if nominal.file.source(database).text() == "Boundary" {
            Declared::Enum { variants: 3 }
        } else {
            Declared::Struct(vec![byte, double])
        }
    };
    layout(database, ty, &declared).map(|layout| (layout.size(), layout.align()))
}

#[test]
fn layouts_follow_the_documented_rules() {
    let database = Database::default();
    let f64 = primitive(&database, "f64");
    let f32 = primitive(&database, "f32");
    let byte = primitive(&database, "u8");
    let of = |kind| layout_of(&database, Type::new(&database, kind));
    assert_eq!(
        layout_of(&database, primitive(&database, "complex128")),
        Ok((16, 8))
    );
    assert_eq!(
        layout_of(&database, fixed(&database, f32, vec![3, 3])),
        Ok((36, 4))
    );
    assert_eq!(
        of(TypeKind::Array {
            element: f64,
            rank: 3
        }),
        Ok((56, 8))
    );
    assert_eq!(of(TypeKind::List(f64)), Ok((24, 8)));
    assert_eq!(of(TypeKind::Str), Ok((16, 8)));
    assert_eq!(of(TypeKind::Tuple(vec![byte, f64, byte])), Ok((24, 8)));
    assert_eq!(of(TypeKind::Optional(f64)), Ok((16, 8)));
    assert_eq!(of(TypeKind::None), Ok((0, 1)));
    assert_eq!(
        of(TypeKind::Callable {
            parameters: vec![f64],
            returns: f64
        }),
        Ok((0, 1))
    );
    let particle = class(&database, "Particle", false);
    assert_eq!(layout_of(&database, particle), Ok((16, 8)));
    assert_eq!(
        of(TypeKind::Fallible {
            value: f32,
            errors: vec![particle]
        }),
        Ok((24, 8))
    );
    assert_eq!(
        layout_of(&database, class(&database, "Boundary", true)),
        Ok((1, 1))
    );
}

#[test]
fn types_without_a_layout_say_why() {
    let database = Database::default();
    let file = File::new(
        &database,
        FileId::from_index(0),
        SourceFile::new("a.npl", "T").unwrap(),
    );
    let name = name_node(&database, file);
    let parameter = Type::new(&database, TypeKind::Parameter { file, name });
    assert_eq!(layout_of(&database, parameter), Err(LayoutError::Generic));
    let unknown = Type::new(&database, TypeKind::Unknown);
    assert_eq!(
        layout_of(&database, Type::new(&database, TypeKind::List(unknown))),
        Ok((24, 8))
    );
    assert_eq!(
        layout_of(
            &database,
            Type::new(&database, TypeKind::Tuple(vec![unknown]))
        ),
        Err(LayoutError::Unknown)
    );
    let huge = fixed(&database, primitive(&database, "f64"), vec![u64::MAX / 2]);
    assert_eq!(layout_of(&database, huge), Err(LayoutError::TooLarge));
}

#[test]
fn a_struct_that_contains_itself_has_no_layout() {
    let database = Database::default();
    let node = class(&database, "Node", false);
    let declared = |_: &Nominal<'_>| Declared::Struct(vec![node]);
    assert_eq!(
        layout(&database, node, &declared),
        Err(LayoutError::Recursive)
    );
}

#[test]
fn records_pad_between_and_after_fields() {
    let byte = Layout::new(1, 1).unwrap();
    let double = Layout::new(8, 8).unwrap();
    let record = Layout::record([double, byte]).unwrap();
    assert_eq!(record.offsets(), [0, 8]);
    assert_eq!(record.layout().size(), 16);
    assert_eq!(Layout::record([]).unwrap().layout(), Layout::EMPTY);
}
