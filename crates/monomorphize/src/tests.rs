//! Tests of instantiation: every place a type appears is substituted.

use crate::instantiate;
use database::{Db, File, Package, ParsedFile, Resolution};
use mir::{
    BinaryOperator, BlockId, Body, Callee, Constant, Elementwise, Local, LocalKind, Operand, Phase,
    Rvalue, Statement, StatementKind, Terminator, TerminatorKind,
};
use source::{FileId, SourceFile, Span};
use syntax::{NodeId, Role, SyntaxKind, TreeBuilder};
use types::{Float, Integer, Item, Mode, Primitive, Type, TypeKind};

/// A database whose files each hold one name, standing for a definition or a type parameter.
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
        unreachable!("instantiation never resolves names")
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
        SourceFile::new("a.npl", name).unwrap(),
    );
    let tree = database.parse(file).tree();
    let statement = tree.children(tree.root()).next().unwrap();
    (file, tree.children(statement).next().unwrap())
}

/// The span every test statement carries; the text form does not show spans.
fn span() -> Span {
    Span::new(FileId::from_index(0), 0, 1)
}

/// `def total[T: Number](xs: list[T]) -> T`, reduced to the parts that carry types: a constant
/// `T(0)`, a conversion to `T`, a whole-array expression with a `T` scalar, and a call of a generic
/// function at `[T]`. Instantiated at `f32`, every `T` becomes `f32`, and the body is monomorphic.
#[test]
fn every_type_is_substituted() {
    let database = Database::default();
    let (file, name) = named(&database, "T");
    let t = Type::new(&database, TypeKind::Parameter { file, name });
    let primitive = |primitive| Type::new(&database, TypeKind::Primitive(primitive));
    let f32 = primitive(Primitive::Float(Float::F32));
    let i64 = primitive(Primitive::Integer(Integer::I64));
    let list = Type::new(&database, TypeKind::List(t));
    let array = Type::new(
        &database,
        TypeKind::Array {
            element: t,
            rank: 1,
        },
    );
    let (callee_file, callee_name) = named(&database, "first");
    let package = Package::new(&database, vec![callee_file]);
    let first = Item::new(&database, package, callee_file, callee_name);

    let mut body = Body::new(t, span());
    let xs = body.push_local(
        list,
        LocalKind::Parameter {
            name: "xs".into(),
            mode: Mode::Borrow,
        },
        span(),
    );
    let count = body.push_local(i64, LocalKind::Temporary, span());
    let scaled = body.push_local(array, LocalKind::Temporary, span());
    let statement = |kind| Statement { kind, span: span() };
    let entry = body.push_block(Terminator {
        kind: TerminatorKind::Call {
            callee: Callee::Function {
                item: first,
                type_arguments: vec![t],
            },
            arguments: vec![mir::Argument::Borrow(xs.into())],
            destination: Local::RETURN.into(),
            target: BlockId::new(1),
            error: None,
        },
        span: span(),
    });
    body.blocks[entry.index()].statements = vec![
        statement(StatementKind::Assign(
            Local::RETURN.into(),
            Rvalue::Convert(Operand::Copy(count.into()), t),
        )),
        statement(StatementKind::Assign(
            scaled.into(),
            Rvalue::Elementwise(Box::new(Elementwise::Binary(
                BinaryOperator::Multiply,
                Box::new(Elementwise::Leaf(Operand::Copy(scaled.into()))),
                Box::new(Elementwise::Leaf(Operand::Constant(Constant::integer(
                    t, 2,
                )))),
            ))),
        )),
    ];
    body.push_block(Terminator {
        kind: TerminatorKind::Return,
        span: span(),
    });

    let mut instance = instantiate(&database, &body, &[t], &[f32]);
    assert_eq!(
        instance.display(&database).to_string(),
        "\
fn(_1: list[f32]) -> f32 {
    let _0: f32; // return
    let _1: list[f32]; // parameter xs
    let _2: i64;
    let _3: Array[f32, 1];

    bb0: {
        _0 = f32(copy _2);
        _3 = elementwise Multiply(copy _3, 2_f32);
        _0 = call first[f32](_1) -> [return: bb1];
    }

    bb1: {
        return;
    }
}
"
    );
    // A ready body is also expanded, so drop the whole-array statement; the validator then
    // finds no type parameter left anywhere.
    instance.phase = Phase::Ready;
    instance.blocks[0].statements.remove(1);
    assert_eq!(instance.validate(&database), Vec::new());
}

/// A type parameter the substitution does not list stays as it is: a generic method's body may
/// mention the parameters of its class, which the caller substitutes separately.
#[test]
fn unlisted_parameters_stay() {
    let database = Database::default();
    let (file, name) = named(&database, "T");
    let (other_file, other_name) = named(&database, "U");
    let t = Type::new(&database, TypeKind::Parameter { file, name });
    let u = Type::new(
        &database,
        TypeKind::Parameter {
            file: other_file,
            name: other_name,
        },
    );
    let pair = Type::new(&database, TypeKind::Tuple(vec![t, u]));
    let i64 = Type::new(
        &database,
        TypeKind::Primitive(Primitive::Integer(Integer::I64)),
    );
    let mut body = Body::new(pair, span());
    body.push_block(Terminator {
        kind: TerminatorKind::Return,
        span: span(),
    });

    let instance = instantiate(&database, &body, &[t], &[i64]);
    assert_eq!(
        instance.locals[0].ty,
        Type::new(&database, TypeKind::Tuple(vec![i64, u]))
    );
}
