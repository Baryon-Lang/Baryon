//! Tests of the text form and the validator, over bodies built by hand the way lowering will
//! build them.

use crate::{
    Argument, BasicBlock, BinaryOperator, BlockId, Body, Builtin, Callee, Check, Constant,
    Elementwise, ErrorEdge, Invalid, Local, LocalKind, MathFunction, Operand, Phase, Place,
    Projection, Rvalue, SliceIndex, Statement, StatementKind, Terminator, TerminatorKind,
};
use database::{Db, File, Package, ParsedFile, Resolution};
use source::{FileId, SourceFile, Span};
use syntax::{NodeId, Role, SyntaxKind, TreeBuilder};
use types::{Float, Integer, Item, Mode, Nominal, Primitive, Type, TypeKind};

/// A database whose files each hold one name, such as `parse_number`, standing for a definition.
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
        unreachable!("IR tests never resolve names")
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

/// The primitive type `primitive`.
fn primitive(database: &Database, primitive: Primitive) -> Type<'_> {
    Type::new(database, TypeKind::Primitive(primitive))
}

/// An assignment statement.
fn assign(place: impl Into<Place>, rvalue: Rvalue<'_>) -> Statement<'_> {
    Statement {
        kind: StatementKind::Assign(place.into(), rvalue),
        span: span(),
    }
}

/// A terminator.
fn end(kind: TerminatorKind<'_>) -> Terminator<'_> {
    Terminator { kind, span: span() }
}

/// `def first(values: Owned[Array[f64, 1]]) -> f64: return values[0]`, with its bounds check
/// and the drop of the array it owns.
#[test]
fn bounds_check_and_drop() {
    let database = Database::default();
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let bool = primitive(&database, Primitive::Bool);
    let array = Type::new(
        &database,
        TypeKind::Array {
            element: f64,
            rank: 1,
        },
    );

    let mut body = Body::new(f64, span());
    let values = body.push_local(
        array,
        LocalKind::Parameter {
            name: "values".into(),
            mode: Mode::Owned,
        },
        span(),
    );
    let index = body.push_local(i64, LocalKind::Temporary, span());
    let length = body.push_local(i64, LocalKind::Temporary, span());
    let in_bounds = body.push_local(bool, LocalKind::Temporary, span());
    let check = body.push_block(end(TerminatorKind::Assert {
        condition: Operand::Copy(in_bounds.into()),
        check: Check::IndexInBounds {
            dimension: 0,
            index: Operand::Copy(index.into()),
            length: Operand::Copy(length.into()),
        },
        target: BlockId::new(1),
    }));
    body.blocks[check.index()].statements = vec![
        assign(
            index,
            Rvalue::Use(Operand::Constant(Constant::integer(i64, 0))),
        ),
        assign(length, Rvalue::Length(values.into(), 0)),
        assign(
            in_bounds,
            Rvalue::Binary(
                BinaryOperator::Less,
                Operand::Copy(index.into()),
                Operand::Copy(length.into()),
            ),
        ),
    ];
    let read = body.push_block(end(TerminatorKind::Return));
    body.blocks[read.index()].statements = vec![
        assign(
            Local::RETURN,
            Rvalue::Use(Operand::Copy(
                Place::from(values).project(Projection::Index(vec![index])),
            )),
        ),
        Statement {
            kind: StatementKind::Drop(values.into()),
            span: span(),
        },
    ];

    assert_eq!(body.validate(&database), Vec::new());
    assert_eq!(
        body.display(&database).to_string(),
        "\
fn(_1: Owned[Array[f64, 1]]) -> f64 {
    let _0: f64; // return
    let _1: Array[f64, 1]; // parameter values
    let _2: i64;
    let _3: i64;
    let _4: bool;

    bb0: {
        _2 = 0_i64;
        _3 = len(_1, 0);
        _4 = Less(copy _2, copy _3);
        assert(copy _4, index in bounds in dimension 0: copy _2 < copy _3) -> bb1;
    }

    bb1: {
        _0 = copy _1[_2];
        drop(_1);
        return;
    }
}
"
    );
}

/// `def parse_twice(text: str) -> Raises[i64, ValueError]: return parse_number(text)`: the call
/// has an error edge, and the error is propagated by switching on which error arrived and raising
/// it again.
#[test]
fn error_edge_propagates() {
    let database = Database::default();
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let str = Type::new(&database, TypeKind::Str);
    let none = Type::new(&database, TypeKind::None);
    let (error_file, error_name) = named(&database, "ValueError");
    let value_error = Type::new(
        &database,
        TypeKind::Struct(Nominal {
            file: error_file,
            name: error_name,
            arguments: Vec::new(),
        }),
    );
    let errors = Type::new(
        &database,
        TypeKind::Fallible {
            value: none,
            errors: vec![value_error],
        },
    );
    let (callee_file, callee_name) = named(&database, "parse_number");
    let package = Package::new(&database, vec![callee_file]);
    let parse_number = Item::new(&database, package, callee_file, callee_name);

    let mut body = Body::new(i64, span());
    let text = body.push_local(
        str,
        LocalKind::Parameter {
            name: "text".into(),
            mode: Mode::Borrow,
        },
        span(),
    );
    let error = body.push_local(errors, LocalKind::Temporary, span());
    let which = body.push_local(i64, LocalKind::Temporary, span());
    body.push_block(end(TerminatorKind::Call {
        callee: Callee::Function {
            item: parse_number,
            type_arguments: Vec::new(),
        },
        arguments: vec![Argument::Borrow(text.into())],
        destination: Local::RETURN.into(),
        target: BlockId::new(1),
        error: Some(ErrorEdge {
            error,
            target: BlockId::new(2),
        }),
    }));
    body.push_block(end(TerminatorKind::Return));
    let dispatch = body.push_block(end(TerminatorKind::Switch {
        value: Operand::Copy(which.into()),
        cases: vec![(0, BlockId::new(3))],
        otherwise: BlockId::new(4),
    }));
    body.blocks[dispatch.index()].statements =
        vec![assign(which, Rvalue::ErrorIndex(error.into()))];
    body.push_block(end(TerminatorKind::Raise(Operand::Move(
        Place::from(error).project(Projection::Error(0)),
    ))));
    body.push_block(end(TerminatorKind::Unreachable));

    assert_eq!(body.validate(&database), Vec::new());
    assert_eq!(
        body.display(&database).to_string(),
        "\
fn(_1: str) -> i64 {
    let _0: i64; // return
    let _1: str; // parameter text
    let _2: Raises[None, ValueError];
    let _3: i64;

    bb0: {
        _0 = call parse_number(_1) -> [return: bb1, error _2: bb2];
    }

    bb1: {
        return;
    }

    bb2: {
        _3 = error_index(_2);
        switch copy _3 -> [0: bb3, otherwise: bb4];
    }

    bb3: {
        raise move _2.error[0];
    }

    bb4: {
        unreachable;
    }
}
"
    );
}

/// `a + b * 2.0` over arrays stays one rvalue in a built body, and a ready body may not hold it.
#[test]
fn elementwise_only_in_built_bodies() {
    let database = Database::default();
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let array = Type::new(
        &database,
        TypeKind::Array {
            element: f64,
            rank: 1,
        },
    );
    let mut body = Body::new(array, span());
    let parameter = |name: &str| LocalKind::Parameter {
        name: name.into(),
        mode: Mode::Borrow,
    };
    let a = body.push_local(array, parameter("a"), span());
    let b = body.push_local(array, parameter("b"), span());
    let entry = body.push_block(end(TerminatorKind::Return));
    body.blocks[entry.index()].statements = vec![assign(
        Local::RETURN,
        Rvalue::Elementwise(Box::new(Elementwise::Binary(
            BinaryOperator::Add,
            Box::new(Elementwise::Leaf(Operand::Copy(a.into()))),
            Box::new(Elementwise::Binary(
                BinaryOperator::Multiply,
                Box::new(Elementwise::Leaf(Operand::Copy(b.into()))),
                Box::new(Elementwise::Leaf(Operand::Constant(Constant::float(
                    f64, 2.0,
                )))),
            )),
        ))),
    )];

    assert_eq!(body.validate(&database), Vec::new());
    assert!(
        body.display(&database)
            .to_string()
            .contains("_0 = elementwise Add(copy _1, Multiply(copy _2, 2.0_f64));")
    );

    body.phase = Phase::Ready;
    assert_eq!(body.validate(&database), vec![Invalid::NotExpanded(entry)]);
}

/// A ready body is one instance at concrete types, so a type parameter in it is a bug.
#[test]
fn ready_bodies_are_monomorphic() {
    let database = Database::default();
    let (file, name) = named(&database, "T");
    let parameter = Type::new(&database, TypeKind::Parameter { file, name });
    let list = Type::new(&database, TypeKind::List(parameter));
    let mut body = Body::new(list, span());
    body.push_block(end(TerminatorKind::Return));
    assert_eq!(body.validate(&database), Vec::new());

    body.phase = Phase::Ready;
    let problems = body.validate(&database);
    assert_eq!(problems, vec![Invalid::NotMonomorphic(Local::RETURN)]);
    assert_eq!(
        problems[0].to_string(),
        "_0 has a type parameter in a ready body"
    );
}

/// Each structural rule is reported, with the block or local that breaks it.
#[test]
fn broken_rules_are_reported() {
    let database = Database::default();
    let bool = primitive(&database, Primitive::Bool);

    let mut empty = Body::new(bool, span());
    empty.parameter_count = 1;
    assert_eq!(
        empty.validate(&database),
        vec![Invalid::ParameterOrder(Local::new(1)), Invalid::NoBlocks]
    );

    let mut body = Body::new(bool, span());
    body.push_local(bool, LocalKind::Temporary, span());
    body.push_local(
        bool,
        LocalKind::Parameter {
            name: "late".into(),
            mode: Mode::Borrow,
        },
        span(),
    );
    body.blocks.push(BasicBlock {
        statements: vec![assign(
            Local::new(9),
            Rvalue::IsPresent(Local::new(1).into()),
        )],
        terminator: end(TerminatorKind::Goto(BlockId::new(5))),
    });
    let problems = body.validate(&database);
    assert_eq!(
        problems,
        vec![
            Invalid::ParameterOrder(Local::new(1)),
            Invalid::ParameterOrder(Local::new(2)),
            Invalid::UnknownLocal(BlockId::ENTRY, Local::new(9)),
            Invalid::UnknownBlock(BlockId::ENTRY, BlockId::new(5)),
        ]
    );
    let messages = problems.iter().map(ToString::to_string).collect::<Vec<_>>();
    assert_eq!(
        messages,
        [
            "_1 breaks the parameter order",
            "_2 breaks the parameter order",
            "bb0 uses _9, which does not exist",
            "bb0 jumps to bb5, which does not exist",
        ]
    );
}

/// `c = a * a` over a rank-2 array allocates its result with `a`'s lengths before expansion fills
/// it; the text form shows the allocation as `new` and the array type.
#[test]
fn new_array_names_its_type_and_lengths() {
    let database = Database::default();
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let grid = Type::new(
        &database,
        TypeKind::Array {
            element: f64,
            rank: 2,
        },
    );
    let mut body = Body::new(grid, span());
    let rows = body.push_local(i64, LocalKind::Temporary, span());
    let columns = body.push_local(i64, LocalKind::Temporary, span());
    let entry = body.push_block(end(TerminatorKind::Return));
    body.blocks[entry.index()].statements = vec![assign(
        Local::RETURN,
        Rvalue::NewArray(
            grid,
            vec![Operand::Copy(rows.into()), Operand::Copy(columns.into())],
        ),
    )];

    assert_eq!(body.validate(&database), Vec::new());
    assert!(
        body.display(&database)
            .to_string()
            .contains("_0 = new Array[f64, 2](copy _1, copy _2);")
    );
}

/// `b[:, None]`: a view whose second dimension is a new axis of length 1. It names no local, and
/// the text form writes it as `None`.
#[test]
fn a_new_axis_is_written_none_and_names_no_local() {
    let database = Database::default();
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let array = |rank| Type::new(&database, TypeKind::Array { element: f64, rank });
    let mut body = Body::new(f64, span());
    let b = body.push_local(array(1), LocalKind::Temporary, span());
    let range: Vec<Local> = (0..3)
        .map(|_| body.push_local(i64, LocalKind::Temporary, span()))
        .collect();
    let length = body.push_local(i64, LocalKind::Temporary, span());
    let view = Place::from(b).project(Projection::Slice(vec![
        SliceIndex::Range {
            start: range[0],
            length: range[1],
            step: range[2],
        },
        SliceIndex::NewAxis,
    ]));
    let statement = assign(length, Rvalue::Length(view, 1));
    assert_eq!(
        statement.kind.locals(),
        [length, b, range[0], range[1], range[2]]
    );
    let entry = body.push_block(end(TerminatorKind::Unreachable));
    body.blocks[entry.index()].statements = vec![statement];
    assert!(
        body.display(&database)
            .to_string()
            .contains("_5 = len(_1[_2 for _3 by _4, None], 1);")
    );
}

/// `a + b` over arrays checks that each dimension of `b` has `a`'s length, and the check reports
/// both lengths.
#[test]
fn same_length_check_reports_both_lengths() {
    let database = Database::default();
    let bool = primitive(&database, Primitive::Bool);
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let mut body = Body::new(bool, span());
    let expected = body.push_local(i64, LocalKind::Temporary, span());
    let found = body.push_local(i64, LocalKind::Temporary, span());
    let same = body.push_local(bool, LocalKind::Temporary, span());
    let check = Check::SameLength {
        dimension: 0,
        expected: Operand::Copy(expected.into()),
        found: Operand::Copy(found.into()),
    };
    assert_eq!(check.operands().len(), 2);
    body.push_block(end(TerminatorKind::Assert {
        condition: Operand::Copy(same.into()),
        check,
        target: BlockId::new(1),
    }));
    body.push_block(end(TerminatorKind::Return));

    assert_eq!(body.validate(&database), Vec::new());
    assert!(
        body.display(&database)
            .to_string()
            .contains("assert(copy _3, same length in dimension 0: copy _2 == copy _1) -> bb1;")
    );
}

/// Integer `x ** n` checks that `n` is not negative, and the check reports the exponent; its
/// multiplications check for overflow as `**`, which a release build leaves out.
#[test]
fn exponent_check_reports_the_exponent() {
    let database = Database::default();
    let bool = primitive(&database, Primitive::Bool);
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let mut body = Body::new(bool, span());
    let exponent = body.push_local(i64, LocalKind::Temporary, span());
    let fits = body.push_local(bool, LocalKind::Temporary, span());
    let check = Check::NonNegativeExponent {
        exponent: Operand::Copy(exponent.into()),
    };
    assert_eq!(check.operands().len(), 1);
    assert!(!check.is_overflow() && Check::NoPowerOverflow.is_overflow());
    body.push_block(end(TerminatorKind::Assert {
        condition: Operand::Copy(fits.into()),
        check,
        target: BlockId::new(1),
    }));
    body.push_block(end(TerminatorKind::Assert {
        condition: Operand::Copy(fits.into()),
        check: Check::NonZeroStep { slice: false },
        target: BlockId::new(2),
    }));
    body.push_block(end(TerminatorKind::Return));

    assert_eq!(body.validate(&database), Vec::new());
    let text = body.display(&database).to_string();
    assert!(text.contains("assert(copy _2, non-negative exponent: copy _1) -> bb1;"));
    assert!(text.contains("assert(copy _2, non-zero step) -> bb2;"));
}

/// A call of the built-in `print` names it, with no item and no type arguments.
#[test]
fn builtins_are_called_by_name() {
    let database = Database::default();
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let none = Type::new(&database, TypeKind::None);
    let mut body = Body::new(none, span());
    let x = body.push_local(
        f64,
        LocalKind::Parameter {
            name: "x".into(),
            mode: Mode::Borrow,
        },
        span(),
    );
    let printed = body.push_local(none, LocalKind::Temporary, span());
    body.push_block(end(TerminatorKind::Call {
        callee: Callee::Builtin(Builtin::Print),
        arguments: vec![
            Argument::Borrow(x.into()),
            Argument::Owned(Operand::Constant(Constant::float(f64, 2.0))),
        ],
        destination: printed.into(),
        target: BlockId::new(1),
        error: None,
    }));
    body.push_block(end(TerminatorKind::Return));

    assert_eq!(body.validate(&database), Vec::new());
    assert!(
        body.display(&database)
            .to_string()
            .contains("_2 = call print(_1, owned 2.0_f64) -> [return: bb1];"),
        "{}",
        body.display(&database)
    );
}

/// `x ** 3.0` on floats is a call of the math built-in `pow`, written by its `math` name.
#[test]
fn math_builtins_are_called_by_their_math_names() {
    let database = Database::default();
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let mut body = Body::new(f64, span());
    let x = body.push_local(
        f64,
        LocalKind::Parameter {
            name: "x".into(),
            mode: Mode::Borrow,
        },
        span(),
    );
    body.push_block(end(TerminatorKind::Call {
        callee: Callee::Builtin(Builtin::Math(MathFunction::Power)),
        arguments: vec![
            Argument::Owned(Operand::Copy(x.into())),
            Argument::Owned(Operand::Constant(Constant::float(f64, 3.0))),
        ],
        destination: Local::RETURN.into(),
        target: BlockId::new(1),
        error: None,
    }));
    body.push_block(end(TerminatorKind::Return));

    assert_eq!(body.validate(&database), Vec::new());
    assert!(
        body.display(&database)
            .to_string()
            .contains("_0 = call math.pow(owned copy _1, owned 3.0_f64) -> [return: bb1];"),
        "{}",
        body.display(&database)
    );
}

/// An f-string's pieces are written in order, each value with its specification.
#[test]
fn format_writes_its_pieces() {
    let database = Database::default();
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let str = Type::new(&database, TypeKind::Str);
    let mut body = Body::new(str, span());
    let energy = body.push_local(
        f64,
        LocalKind::Parameter {
            name: "energy".into(),
            mode: Mode::Borrow,
        },
        span(),
    );
    let pieces = vec![
        crate::Piece::Text("energy ".to_owned()),
        crate::Piece::Value(energy.into(), types::FormatSpec::parse(">10.4f").unwrap()),
    ];
    body.push_block(end(TerminatorKind::Return));
    body.blocks[0].statements = vec![assign(Local::RETURN, Rvalue::Format(pieces))];
    assert_eq!(body.validate(&database), Vec::new());
    assert!(
        body.display(&database)
            .to_string()
            .contains("_0 = format(\"energy \", {_1: >10.4f});"),
        "{}",
        body.display(&database)
    );
}

mod fused;
mod parallel;
mod vector;

mod slices;
mod unions;
