//! Tests of instantiation: every place a type appears is substituted.

use crate::instantiate;
use database::{Db, File, Package, ParsedFile, Resolution};
use mir::{
    BinaryOperator, BlockId, Body, Callee, Constant, ConstantValue, Elementwise, Local, LocalKind,
    Operand, Phase, Rvalue, Statement, StatementKind, Terminator, TerminatorKind,
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

/// `def total[T: Number](xs: list[T]) -> T`, reduced to the parts that carry types: a constant
/// `T(0)`, a conversion to `T`, a whole-array expression with a `T` scalar, and a call of a generic
/// function at `[T]`. Instantiated at `f32`, every `T` becomes `f32`, the literal `2` is written
/// as the float `2.0`, and the body is monomorphic.
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
        _3 = elementwise Multiply(copy _3, 2.0_f32);
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

/// A literal of a type parameter's type keeps its value as written until the instance gives it a
/// number type: an integer stays an integer at `i64`, becomes a float at `f64`, and becomes a
/// complex number with no imaginary part at `complex128`.
#[test]
fn literals_take_the_instance_number_type() {
    let database = Database::default();
    let (file, name) = named(&database, "T");
    let t = Type::new(&database, TypeKind::Parameter { file, name });
    let primitive = |primitive| Type::new(&database, TypeKind::Primitive(primitive));
    let mut body = Body::new(t, span());
    let entry = body.push_block(Terminator {
        kind: TerminatorKind::Return,
        span: span(),
    });
    body.blocks[entry.index()].statements = vec![Statement {
        kind: StatementKind::Assign(
            Local::RETURN.into(),
            Rvalue::Use(Operand::Constant(Constant::integer(t, -3))),
        ),
        span: span(),
    }];
    let written = |ty| {
        let instance = instantiate(&database, &body, &[t], &[ty]);
        match &instance.blocks[0].statements[0].kind {
            StatementKind::Assign(_, Rvalue::Use(Operand::Constant(constant))) => {
                constant.value.clone()
            }
            other => panic!("expected a constant, found {other:?}"),
        }
    };
    assert_eq!(
        written(primitive(Primitive::Integer(Integer::I64))),
        ConstantValue::Integer(-3)
    );
    assert_eq!(
        written(primitive(Primitive::Float(Float::F64))),
        ConstantValue::Float((-3.0_f64).to_bits())
    );
    assert_eq!(
        written(primitive(Primitive::Complex(types::Complex::C128))),
        ConstantValue::Complex {
            real: (-3.0_f64).to_bits(),
            imaginary: 0.0_f64.to_bits(),
        }
    );
}

/// The body of a `prange` loop in a generic function is instantiated with the function: its
/// locals and the loop's count take the instance's types.
#[test]
fn outlined_bodies_are_substituted() {
    let database = Database::default();
    let (file, name) = named(&database, "T");
    let t = Type::new(&database, TypeKind::Parameter { file, name });
    let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    let none = Type::new(&database, TypeKind::None);
    let mut outlined = Body::new(none, span());
    outlined.push_local(t, LocalKind::Temporary, span());
    let mut body = Body::new(t, span());
    body.outlined.push(outlined);
    body.push_block(Terminator {
        kind: TerminatorKind::ParallelFor {
            body: 0,
            count: Operand::Constant(Constant::integer(t, 4)),
            arguments: Vec::new(),
            reductions: Vec::new(),
            accumulations: Vec::new(),
            target: BlockId::new(0),
        },
        span: span(),
    });

    let instance = instantiate(&database, &body, &[t], &[f64]);
    assert_eq!(instance.outlined[0].locals[1].ty, f64);
    let TerminatorKind::ParallelFor { count, .. } = &instance.blocks[0].terminator.kind else {
        panic!("the loop is kept");
    };
    assert_eq!(
        count,
        &Operand::Constant(Constant {
            ty: f64,
            value: ConstantValue::Float(4.0_f64.to_bits()),
        })
    );
}

/// `def step[S: VectorSpace](f: Callable[[S], S], s: S, h: f64) -> S: return h * f(s)`,
/// instantiated with `S` a `Fixed` vector and `f` the function `lorenz` (decision 0026): the
/// call of `f` becomes a direct call of `lorenz`, and `h * …`, scalar arithmetic on `S` in the
/// generic body, becomes a whole-array expression for array expansion.
#[test]
fn callable_parameters_become_direct_calls_and_vector_arithmetic_whole_array() {
    let database = Database::default();
    let (file, name) = named(&database, "S");
    let s = Type::new(&database, TypeKind::Parameter { file, name });
    let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    let vector = Type::new(
        &database,
        TypeKind::Fixed {
            element: f64,
            shape: types::Shape::new(vec![3]).unwrap(),
        },
    );
    let signature = |argument| {
        Type::new(
            &database,
            TypeKind::Callable {
                parameters: vec![argument],
                returns: argument,
            },
        )
    };
    let (generic, concrete) = (signature(s), signature(vector));
    let (lorenz_file, lorenz_name) = named(&database, "lorenz");
    let package = Package::new(&database, vec![lorenz_file]);
    let lorenz = Item::new(&database, package, lorenz_file, lorenz_name);
    let function = Type::new(
        &database,
        TypeKind::Function {
            item: lorenz,
            callable: concrete,
        },
    );

    let mut body = Body::new(s, span());
    let parameter = |name: &str| LocalKind::Parameter {
        name: name.into(),
        mode: Mode::Borrow,
    };
    let f = body.push_local(generic, parameter("f"), span());
    let state = body.push_local(s, parameter("s"), span());
    let h = body.push_local(f64, parameter("h"), span());
    let derivative = body.push_local(s, LocalKind::Temporary, span());
    body.push_block(Terminator {
        kind: TerminatorKind::Call {
            callee: Callee::Value(Operand::Copy(f.into())),
            arguments: vec![mir::Argument::Borrow(state.into())],
            destination: derivative.into(),
            target: BlockId::new(1),
            error: None,
        },
        span: span(),
    });
    let scale = body.push_block(Terminator {
        kind: TerminatorKind::Return,
        span: span(),
    });
    body.blocks[scale.index()].statements = vec![Statement {
        kind: StatementKind::Assign(
            Local::RETURN.into(),
            Rvalue::Binary(
                BinaryOperator::Multiply,
                Operand::Copy(h.into()),
                Operand::Copy(derivative.into()),
            ),
        ),
        span: span(),
    }];

    let instance = instantiate(&database, &body, &[s, generic], &[vector, function]);
    assert!(matches!(
        instance.locals[f.index()].ty.kind(&database),
        TypeKind::Function { item, .. } if *item == lorenz
    ));
    // `f`'s type is now `lorenz`'s own, which shows as its signature.
    assert_eq!(
        instance.display(&database).to_string(),
        "\
fn(_1: Callable[[Fixed[f64, (3,)]], Fixed[f64, (3,)]], _2: Fixed[f64, (3,)], _3: f64) -> Fixed[f64, (3,)] {
    let _0: Fixed[f64, (3,)]; // return
    let _1: Callable[[Fixed[f64, (3,)]], Fixed[f64, (3,)]]; // parameter f
    let _2: Fixed[f64, (3,)]; // parameter s
    let _3: f64; // parameter h
    let _4: Fixed[f64, (3,)];

    bb0: {
        _4 = call lorenz(_2) -> [return: bb1];
    }

    bb1: {
        _0 = elementwise Multiply(copy _3, copy _4);
        return;
    }
}
"
    );
}

/// `def twice[T: Number](x: T) -> T: return x + x`, where lowering writes `x + x` into a local
/// of type `T`: at `i32` the addition is checked for overflow as integer `+` is (decision 0030),
/// with the result in a new block after the `Assert`; at `f64` it stays plain arithmetic.
#[test]
fn integer_instances_check_overflow() {
    let database = Database::default();
    let (file, name) = named(&database, "T");
    let t = Type::new(&database, TypeKind::Parameter { file, name });
    let primitive = |primitive| Type::new(&database, TypeKind::Primitive(primitive));
    let mut body = Body::new(t, span());
    let x = body.push_local(
        t,
        LocalKind::Parameter {
            name: "x".into(),
            mode: Mode::Borrow,
        },
        span(),
    );
    let sum = body.push_local(t, LocalKind::Temporary, span());
    let entry = body.push_block(Terminator {
        kind: TerminatorKind::Return,
        span: span(),
    });
    let assign = |place: Local, rvalue| Statement {
        kind: StatementKind::Assign(place.into(), rvalue),
        span: span(),
    };
    body.blocks[entry.index()].statements = vec![
        assign(
            sum,
            Rvalue::Binary(
                BinaryOperator::Add,
                Operand::Copy(x.into()),
                Operand::Copy(x.into()),
            ),
        ),
        assign(Local::RETURN, Rvalue::Use(Operand::Copy(sum.into()))),
    ];

    let integer = instantiate(
        &database,
        &body,
        &[t],
        &[primitive(Primitive::Integer(Integer::I32))],
    );
    assert_eq!(
        integer.display(&database).to_string(),
        "\
fn(_1: i32) -> i32 {
    let _0: i32; // return
    let _1: i32; // parameter x
    let _2: i32;
    let _3: tuple[i32, bool];
    let _4: bool;

    bb0: {
        _3 = CheckedAdd(copy _1, copy _1);
        _4 = Not(copy _3.1);
        assert(copy _4, no overflow: Add) -> bb1;
    }

    bb1: {
        _2 = copy _3.0;
        _0 = copy _2;
        return;
    }
}
"
    );
    let float = instantiate(
        &database,
        &body,
        &[t],
        &[primitive(Primitive::Float(Float::F64))],
    );
    assert_eq!(float.blocks.len(), 1);
    assert!(matches!(
        float.blocks[0].statements[0].kind,
        StatementKind::Assign(_, Rvalue::Binary(BinaryOperator::Add, ..))
    ));
}

/// `def both(f: Callable[[f64], f64], g: Callable[[f64], f64])` with `h = g`, `apply(g)` and
/// `h()`, instantiated with `f` the function `square` and `g` the function `cube`. The two
/// parameters share one `Callable` type, so substitution alone cannot tell them apart: the copy
/// `h` takes `cube`'s type from `g`, its call becomes a direct call of `cube`, and
/// `forward_callables` gives `apply` the type argument `cube`, not the shared `Callable`.
#[test]
fn callables_passed_on_and_copied_keep_their_function() {
    let database = Database::default();
    let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    let callable = Type::new(
        &database,
        TypeKind::Callable {
            parameters: vec![f64],
            returns: f64,
        },
    );
    let function = |name: &str| {
        let (file, node) = named(&database, name);
        let package = Package::new(&database, vec![file]);
        let item = Item::new(&database, package, file, node);
        let ty = Type::new(&database, TypeKind::Function { item, callable });
        (item, ty)
    };
    let ((_, square), (cube_item, cube), (apply, _)) =
        (function("square"), function("cube"), function("apply"));

    let mut body = Body::new(f64, span());
    let parameter = |name: &str| LocalKind::Parameter {
        name: name.into(),
        mode: Mode::Borrow,
    };
    let f = body.push_local(callable, parameter("f"), span());
    let g = body.push_local(callable, parameter("g"), span());
    let h = body.push_local(callable, LocalKind::Temporary, span());
    let entry = body.push_block(Terminator {
        kind: TerminatorKind::Call {
            callee: Callee::Function {
                item: apply,
                type_arguments: vec![callable],
            },
            arguments: vec![mir::Argument::Borrow(g.into())],
            destination: Local::RETURN.into(),
            target: BlockId::new(1),
            error: None,
        },
        span: span(),
    });
    body.blocks[entry.index()].statements = vec![Statement {
        kind: StatementKind::Assign(h.into(), Rvalue::Use(Operand::Copy(g.into()))),
        span: span(),
    }];
    body.push_block(Terminator {
        kind: TerminatorKind::Call {
            callee: Callee::Value(Operand::Copy(h.into())),
            arguments: Vec::new(),
            destination: Local::RETURN.into(),
            target: BlockId::new(2),
            error: None,
        },
        span: span(),
    });
    body.push_block(Terminator {
        kind: TerminatorKind::Return,
        span: span(),
    });

    let mut instance = instantiate(&database, &body, &[callable, callable], &[square, cube]);
    crate::forward_callables(&database, &mut instance, &|item| {
        if item == apply { vec![0] } else { Vec::new() }
    });
    assert_eq!(instance.locals[f.index()].ty, square);
    assert_eq!(instance.locals[h.index()].ty, cube);
    let TerminatorKind::Call {
        callee: Callee::Function { type_arguments, .. },
        ..
    } = &instance.blocks[0].terminator.kind
    else {
        panic!("the first block calls `apply`");
    };
    assert_eq!(type_arguments, &[cube]);
    assert!(matches!(
        &instance.blocks[1].terminator.kind,
        TerminatorKind::Call { callee: Callee::Function { item, .. }, .. } if *item == cube_item
    ));
}
