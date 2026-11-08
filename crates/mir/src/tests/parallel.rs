//! Tests of `prange` loops: an outlined body run by `ParallelFor`, in the text form and the
//! validator.

use super::{Database, assign, end, primitive, span};
use crate::{
    AccumulateOperator, Argument, BinaryOperator, BlockId, Body, Constant, Invalid, Local,
    LocalKind, Operand, Place, Projection, Reduction, ReductionOperator, Rvalue, Statement,
    StatementKind, TerminatorKind,
};
use types::{Float, Integer, Mode, Primitive, Type, TypeKind};

/// The loop of `def count(xs: Array[f64, 1]) -> f64`:
///
/// ```python
/// total = 0.0
/// for i in prange(len(xs)):
///     total += xs[i]
/// return total
/// ```
///
/// reduced to its shape: the body borrows `xs`, reduces into `total` with `Add`, and takes the
/// iteration number first. The outlined body is written after the function.
fn summing(database: &Database) -> Body<'_> {
    let f64 = primitive(database, Primitive::Float(Float::F64));
    let i64 = primitive(database, Primitive::Integer(Integer::I64));
    let none = Type::new(database, TypeKind::None);
    let array = Type::new(
        database,
        TypeKind::Array {
            element: f64,
            rank: 1,
        },
    );
    let parameter = |name: &str, mode| LocalKind::Parameter {
        name: name.into(),
        mode,
    };

    let mut outlined = Body::new(none, span());
    outlined.push_local(i64, parameter("i", Mode::Owned), span());
    outlined.push_local(array, parameter("xs", Mode::Borrow), span());
    let total = outlined.push_local(f64, parameter("total", Mode::InOut), span());
    let entry = outlined.push_block(end(TerminatorKind::Return));
    outlined.blocks[entry.index()].statements.push(assign(
        total,
        Rvalue::Binary(
            BinaryOperator::Add,
            Operand::Copy(total.into()),
            Operand::Constant(Constant::float(f64, 1.0)),
        ),
    ));

    let mut body = Body::new(f64, span());
    let xs = body.push_local(array, parameter("xs", Mode::Borrow), span());
    let count = body.push_local(i64, LocalKind::Temporary, span());
    let total = body.push_local(
        f64,
        LocalKind::Variable {
            name: "total".into(),
        },
        span(),
    );
    body.outlined.push(outlined);
    let entry = body.push_block(end(TerminatorKind::ParallelFor {
        body: 0,
        count: Operand::Copy(count.into()),
        arguments: vec![Argument::Borrow(xs.into())],
        reductions: vec![Reduction {
            place: total.into(),
            operator: ReductionOperator::Add,
        }],
        accumulations: Vec::new(),
        target: BlockId::new(1),
    }));
    body.blocks[entry.index()].statements = vec![
        assign(count, Rvalue::Length(xs.into(), 0)),
        assign(
            total,
            Rvalue::Use(Operand::Constant(Constant::float(f64, 0.0))),
        ),
    ];
    let exit = body.push_block(end(TerminatorKind::Return));
    body.blocks[exit.index()].statements.push(assign(
        Local::RETURN,
        Rvalue::Use(Operand::Copy(total.into())),
    ));
    body
}

/// The text form writes the loop as `parallel_for`, with the count, the arguments and the
/// reductions, and each outlined body after the function as `body0`, `body1` and so on.
#[test]
fn parallel_for_names_its_outlined_body() {
    let database = Database::default();
    let body = summing(&database);
    assert_eq!(body.validate(&database), Vec::new());
    assert_eq!(
        body.display(&database).to_string(),
        "\
fn(_1: Array[f64, 1]) -> f64 {
    let _0: f64; // return
    let _1: Array[f64, 1]; // parameter xs
    let _2: i64;
    let _3: f64; // total

    bb0: {
        _2 = len(_1, 0);
        _3 = 0.0_f64;
        parallel_for body0(copy _2; _1, reduce Add _3) -> bb1;
    }

    bb1: {
        _0 = copy _3;
        return;
    }
}

body0: fn(_1: Owned[i64], _2: Array[f64, 1], _3: InOut[f64]) -> None {
    let _0: None; // return
    let _1: i64; // parameter i
    let _2: Array[f64, 1]; // parameter xs
    let _3: f64; // parameter total

    bb0: {
        _3 = Add(copy _3, 1.0_f64);
        return;
    }
}
"
    );
}

/// A `ParallelFor` must name an outlined body that exists and pass it one value per parameter;
/// a rule broken inside an outlined body is reported with its position.
#[test]
fn parallel_for_rules_are_reported() {
    let database = Database::default();
    let mut body = summing(&database);
    let TerminatorKind::ParallelFor { arguments, .. } = &mut body.blocks[0].terminator.kind else {
        unreachable!("the entry block runs the loop");
    };
    arguments.clear();
    body.outlined[0].blocks.clear();
    let mut missing = summing(&database);
    missing.outlined.clear();

    let problems = [body.validate(&database), missing.validate(&database)].concat();
    assert_eq!(
        problems,
        vec![
            Invalid::OutlinedParameters(BlockId::ENTRY),
            Invalid::Outlined(0, Box::new(Invalid::NoBlocks)),
            Invalid::UnknownOutlined(BlockId::ENTRY, 0),
        ]
    );
    let messages = problems.iter().map(ToString::to_string).collect::<Vec<_>>();
    assert_eq!(
        messages,
        [
            "bb0 passes its outlined body the wrong number of values",
            "in outlined body 0: the body has no blocks",
            "bb0 runs outlined body 0, which does not exist",
        ]
    );
}

/// The loop of `def count(bins: InOut[Array[i64, 1]], n: i64)`:
///
/// ```python
/// for i in prange(n, accumulate=bins):
///     bins[i] += 1
/// ```
///
/// reduced to its shape, without the bounds check: the body takes the iteration, then the
/// accumulated array, and adds into it with `Accumulate`.
fn counting(database: &Database) -> Body<'_> {
    let i64 = primitive(database, Primitive::Integer(Integer::I64));
    let none = Type::new(database, TypeKind::None);
    let array = Type::new(
        database,
        TypeKind::Array {
            element: i64,
            rank: 1,
        },
    );
    let parameter = |name: &str, mode| LocalKind::Parameter {
        name: name.into(),
        mode,
    };

    let mut outlined = Body::new(none, span());
    let i = outlined.push_local(i64, parameter("i", Mode::Owned), span());
    let bins = outlined.push_local(array, parameter("bins", Mode::InOut), span());
    let entry = outlined.push_block(end(TerminatorKind::Return));
    outlined.blocks[entry.index()].statements.push(Statement {
        kind: StatementKind::Accumulate {
            place: Place::from(bins).project(Projection::Index(vec![i])),
            operator: AccumulateOperator::Add,
            value: Operand::Constant(Constant::integer(i64, 1)),
        },
        span: span(),
    });

    let mut body = Body::new(none, span());
    let bins = body.push_local(array, parameter("bins", Mode::InOut), span());
    let n = body.push_local(i64, parameter("n", Mode::Borrow), span());
    body.outlined.push(outlined);
    body.push_block(end(TerminatorKind::ParallelFor {
        body: 0,
        count: Operand::Copy(n.into()),
        arguments: Vec::new(),
        reductions: Vec::new(),
        accumulations: vec![bins.into()],
        target: BlockId::new(1),
    }));
    body.push_block(end(TerminatorKind::Return));
    body
}

/// The text form lists each accumulated array after the reductions, and writes an
/// `Accumulate` with its operator.
#[test]
fn accumulate_is_written_with_its_array() {
    let database = Database::default();
    let body = counting(&database);
    assert_eq!(body.validate(&database), Vec::new());
    assert_eq!(
        body.display(&database).to_string(),
        "\
fn(_1: InOut[Array[i64, 1]], _2: i64) -> None {
    let _0: None; // return
    let _1: Array[i64, 1]; // parameter bins
    let _2: i64; // parameter n

    bb0: {
        parallel_for body0(copy _2; accumulate _1) -> bb1;
    }

    bb1: {
        return;
    }
}

body0: fn(_1: Owned[i64], _2: InOut[Array[i64, 1]]) -> None {
    let _0: None; // return
    let _1: i64; // parameter i
    let _2: Array[i64, 1]; // parameter bins

    bb0: {
        accumulate _2[_1] += 1_i64;
        return;
    }
}
"
    );
}

/// An `Accumulate` adds only into a parameter its `ParallelFor` lists as accumulated: not into
/// another parameter of the outlined body, and never in a function's own body.
#[test]
fn accumulate_outside_an_accumulated_parameter_is_reported() {
    let database = Database::default();
    let mut unlisted = counting(&database);
    let TerminatorKind::ParallelFor { accumulations, .. } = &mut unlisted.blocks[0].terminator.kind
    else {
        unreachable!("the entry block runs the loop");
    };
    let bins = accumulations.remove(0);
    let TerminatorKind::ParallelFor { arguments, .. } = &mut unlisted.blocks[0].terminator.kind
    else {
        unreachable!("the entry block runs the loop");
    };
    arguments.push(Argument::InOut(bins));

    let mut outside = counting(&database);
    let accumulate = outside.outlined[0].blocks[0].statements.remove(0);
    outside.blocks[1].statements.push(accumulate);

    let problems = [unlisted.validate(&database), outside.validate(&database)].concat();
    assert_eq!(
        problems,
        vec![
            Invalid::Outlined(
                0,
                Box::new(Invalid::NotAccumulated(BlockId::ENTRY, Local::new(2)))
            ),
            Invalid::NotAccumulated(BlockId::new(1), Local::new(2)),
        ]
    );
    assert_eq!(
        problems[1].to_string(),
        "bb1 accumulates into _2, which is not an accumulated parameter"
    );
}
