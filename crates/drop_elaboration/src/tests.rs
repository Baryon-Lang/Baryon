//! Tests of drop elaboration over bodies built by hand, compared by their full text form.

use crate::elaborate;
use database::{Db, File, Package, ParsedFile, Resolution};
use mir::{
    Argument, BlockId, Body, Callee, LocalKind, Operand, Place, Projection, Rvalue, Statement,
    StatementKind, Terminator, TerminatorKind,
};
use source::{FileId, SourceFile, Span};
use syntax::{NodeId, Role, SyntaxKind, TreeBuilder};
use types::{
    Declared, Float, Integer, Item, Mode, Nominal, Primitive, StandardType, Type, TypeKind,
};

/// A database whose files each hold one name, standing for a definition.
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
        unreachable!("drop elaboration never resolves names")
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

/// A file holding only `name`, and its `Name` node.
fn named(database: &Database, name: &str) -> (File, NodeId) {
    let file = File::new(
        database,
        FileId::from_index(0),
        SourceFile::new("a.byn", name).unwrap(),
    );
    let tree = database.parse(file).tree();
    let statement = tree.children(tree.root()).next().unwrap();
    (file, tree.children(statement).next().unwrap())
}

/// The span every test statement carries; the text form does not show spans.
fn span() -> Span {
    Span::new(FileId::from_index(0), 0, 1)
}

/// The types the tests use.
struct Types<'db> {
    /// `bool`.
    bool: Type<'db>,
    /// `i64`.
    i64: Type<'db>,
    /// `None`.
    none: Type<'db>,
    /// `Array[f64, 1]`.
    array: Type<'db>,
}

impl<'db> Types<'db> {
    /// Interns the types in `database`.
    fn new(database: &'db Database) -> Self {
        let primitive = |primitive| Type::new(database, TypeKind::Primitive(primitive));
        let f64 = primitive(Primitive::Float(Float::F64));
        Self {
            bool: primitive(Primitive::Bool),
            i64: primitive(Primitive::Integer(Integer::I64)),
            none: Type::new(database, TypeKind::None),
            array: Type::new(
                database,
                TypeKind::Array {
                    element: f64,
                    rank: 1,
                },
            ),
        }
    }
}

/// A statement.
fn statement(kind: StatementKind<'_>) -> Statement<'_> {
    Statement { kind, span: span() }
}

/// A terminator.
fn end(kind: TerminatorKind<'_>) -> Terminator<'_> {
    Terminator { kind, span: span() }
}

/// `place = move from`.
fn move_into<'db>(place: impl Into<Place>, from: impl Into<Place>) -> Statement<'db> {
    statement(StatementKind::Assign(
        place.into(),
        Rvalue::Use(Operand::Move(from.into())),
    ))
}

/// `drop(place)`.
fn drop<'db>(place: impl Into<Place>) -> Statement<'db> {
    statement(StatementKind::Drop(place.into()))
}

/// A parameter in `mode`.
fn parameter(name: &str, mode: Mode) -> LocalKind {
    LocalKind::Parameter {
        name: name.into(),
        mode,
    }
}

/// A callback for bodies that move no fields, so never ask what a struct declares.
fn no_structs<'db>(_: &Nominal<'db>) -> Declared<'db> {
    unreachable!("the body moves no fields")
}

/// Runs the pass, checks the result is a valid body, and returns its text form.
fn elaborated<'db>(
    database: &'db Database,
    mut body: Body<'db>,
    declared: &dyn Fn(&Nominal<'db>) -> Declared<'db>,
) -> String {
    elaborate(database, &mut body, declared);
    assert_eq!(body.validate(database), Vec::new());
    body.display(database).to_string()
}

/// A value never moved keeps its drop, and a value moved on every path loses it.
#[test]
fn certain_drops_stay_and_impossible_ones_go() {
    let database = Database::default();
    let types = Types::new(&database);
    let mut body = Body::new(types.none, span());
    let a = body.push_local(types.array, parameter("a", Mode::Owned), span());
    let b = body.push_local(types.array, parameter("b", Mode::Owned), span());
    let c = body.push_local(
        types.array,
        LocalKind::Variable { name: "c".into() },
        span(),
    );
    let entry = body.push_block(end(TerminatorKind::Return));
    body.blocks[entry.index()].statements = vec![move_into(c, a), drop(c), drop(b), drop(a)];

    assert_eq!(
        elaborated(&database, body, &no_structs),
        "\
fn(_1: Owned[Array[f64, 1]], _2: Owned[Array[f64, 1]]) -> None {
    let _0: None; // return
    let _1: Array[f64, 1]; // parameter a
    let _2: Array[f64, 1]; // parameter b
    let _3: Array[f64, 1]; // c

    bb0: {
        _3 = move _1;
        drop(_3);
        drop(_2);
        return;
    }
}
"
    );
}

/// A value moved on one branch only gets a flag that guards its drop where the branches meet.
#[test]
fn moved_on_one_branch_gets_a_flag() {
    let database = Database::default();
    let types = Types::new(&database);
    let mut body = Body::new(types.none, span());
    let c = body.push_local(types.bool, parameter("c", Mode::Borrow), span());
    let a = body.push_local(types.array, parameter("a", Mode::Owned), span());
    let b = body.push_local(
        types.array,
        LocalKind::Variable { name: "b".into() },
        span(),
    );
    body.push_block(end(TerminatorKind::Branch {
        condition: Operand::Copy(c.into()),
        then: BlockId::new(1),
        otherwise: BlockId::new(2),
    }));
    let then = body.push_block(end(TerminatorKind::Goto(BlockId::new(2))));
    body.blocks[then.index()].statements = vec![move_into(b, a), drop(b)];
    let join = body.push_block(end(TerminatorKind::Return));
    body.blocks[join.index()].statements = vec![drop(a)];

    assert_eq!(
        elaborated(&database, body, &no_structs),
        "\
fn(_1: bool, _2: Owned[Array[f64, 1]]) -> None {
    let _0: None; // return
    let _1: bool; // parameter c
    let _2: Array[f64, 1]; // parameter a
    let _3: Array[f64, 1]; // b
    let _4: bool;

    bb0: {
        _4 = True;
        branch copy _1 -> [true: bb1, false: bb2];
    }

    bb1: {
        _3 = move _2;
        _4 = False;
        drop(_3);
        goto -> bb2;
    }

    bb2: {
        branch copy _4 -> [true: bb3, false: bb4];
    }

    bb3: {
        drop(_2);
        goto -> bb4;
    }

    bb4: {
        _4 = False;
        return;
    }
}
"
    );
}

/// A field moved out of a struct leaves the other fields that own memory to be dropped one by
/// one; fields that own nothing, such as an `i64`, need no drop.
#[test]
fn partly_moved_struct_drops_its_remaining_fields() {
    let database = Database::default();
    let types = Types::new(&database);
    let (file, name) = named(&database, "Atoms");
    let atoms = Type::new(
        &database,
        TypeKind::Struct(Nominal {
            file,
            name,
            arguments: Vec::new(),
        }),
    );
    let fields = vec![types.array, types.i64, types.array];
    let declared = |_: &Nominal<'_>| Declared::Struct(fields.clone());

    let mut body = Body::new(types.none, span());
    let a = body.push_local(atoms, parameter("atoms", Mode::Owned), span());
    let positions = body.push_local(
        types.array,
        LocalKind::Variable {
            name: "positions".into(),
        },
        span(),
    );
    let entry = body.push_block(end(TerminatorKind::Return));
    body.blocks[entry.index()].statements = vec![
        move_into(positions, Place::from(a).project(Projection::Field(0))),
        drop(positions),
        drop(a),
    ];

    assert_eq!(
        elaborated(&database, body, &declared),
        "\
fn(_1: Owned[Atoms]) -> None {
    let _0: None; // return
    let _1: Atoms; // parameter atoms
    let _2: Array[f64, 1]; // positions

    bb0: {
        _2 = move _1.0;
        drop(_2);
        drop(_1.2);
        return;
    }
}
"
    );
}

/// A call fills its destination only when it returns, so the flag of a value a call may or may
/// not have filled is set on the call's success edge alone, in a block of its own.
#[test]
fn call_result_sets_its_flag_on_the_success_edge() {
    let database = Database::default();
    let types = Types::new(&database);
    let (file, name) = named(&database, "make");
    let package = Package::new(&database, vec![file]);
    let make = Item::new(&database, package, file, name);

    let mut body = Body::new(types.none, span());
    let c = body.push_local(types.bool, parameter("c", Mode::Borrow), span());
    let a = body.push_local(
        types.array,
        LocalKind::Variable { name: "a".into() },
        span(),
    );
    body.push_block(end(TerminatorKind::Branch {
        condition: Operand::Copy(c.into()),
        then: BlockId::new(1),
        otherwise: BlockId::new(2),
    }));
    body.push_block(end(TerminatorKind::Call {
        callee: Callee::Function {
            item: make,
            type_arguments: Vec::new(),
        },
        arguments: vec![Argument::Borrow(c.into())],
        destination: a.into(),
        target: BlockId::new(2),
        error: None,
    }));
    let join = body.push_block(end(TerminatorKind::Return));
    body.blocks[join.index()].statements = vec![drop(a)];

    assert_eq!(
        elaborated(&database, body, &no_structs),
        "\
fn(_1: bool) -> None {
    let _0: None; // return
    let _1: bool; // parameter c
    let _2: Array[f64, 1]; // a
    let _3: bool;

    bb0: {
        _3 = False;
        branch copy _1 -> [true: bb1, false: bb2];
    }

    bb1: {
        _2 = call make(_1) -> [return: bb3];
    }

    bb2: {
        branch copy _3 -> [true: bb4, false: bb5];
    }

    bb3: {
        _3 = True;
        goto -> bb2;
    }

    bb4: {
        drop(_2);
        goto -> bb5;
    }

    bb5: {
        _3 = False;
        return;
    }
}
"
    );
}

/// A struct with a field moved out still drops a `File` field, which holds an open file to
/// close, and needs no drop for a `Philox` field, which is only numbers.
#[test]
fn a_partly_moved_struct_still_closes_its_file() {
    let database = Database::default();
    let types = Types::new(&database);
    let standard = |standard| Type::new(&database, TypeKind::Standard(standard));
    let (file, name) = named(&database, "Run");
    let run = Type::new(
        &database,
        TypeKind::Struct(Nominal {
            file,
            name,
            arguments: Vec::new(),
        }),
    );
    let fields = vec![
        types.array,
        standard(StandardType::File),
        standard(StandardType::Philox),
    ];
    let declared = |_: &Nominal<'_>| Declared::Struct(fields.clone());

    let mut body = Body::new(types.none, span());
    let a = body.push_local(run, parameter("run", Mode::Owned), span());
    let samples = body.push_local(
        types.array,
        LocalKind::Variable {
            name: "samples".into(),
        },
        span(),
    );
    let entry = body.push_block(end(TerminatorKind::Return));
    body.blocks[entry.index()].statements = vec![
        move_into(samples, Place::from(a).project(Projection::Field(0))),
        drop(samples),
        drop(a),
    ];

    assert_eq!(
        elaborated(&database, body, &declared),
        "\
fn(_1: Owned[Run]) -> None {
    let _0: None; // return
    let _1: Run; // parameter run
    let _2: Array[f64, 1]; // samples

    bb0: {
        _2 = move _1.0;
        drop(_2);
        drop(_1.1);
        return;
    }
}
"
    );
}
