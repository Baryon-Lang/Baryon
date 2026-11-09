//! Tests of views: the [`Slice`](crate::Projection::Slice) step, its checks, its text form and
//! the rule that a view is only borrowed.

use super::{Database, assign, end, primitive, span};
use crate::{
    Argument, BlockId, Body, Callee, Check, Elementwise, Invalid, Local, LocalKind, Operand, Place,
    Projection, Rvalue, SliceIndex, Statement, StatementKind, TerminatorKind,
};
use types::{Float, Integer, Mode, Primitive, Type, TypeKind};

/// A body of `None` with one `InOut[Array[f64, 2]]` parameter `grid` (local 1) and `i64`
/// locals 2 to 5, and the view `grid[_2 for _3 by _4, _5]`: a column of the matrix.
fn column(database: &Database) -> (Body<'_>, Place) {
    let none = Type::new(database, TypeKind::None);
    let f64 = primitive(database, Primitive::Float(Float::F64));
    let i64 = primitive(database, Primitive::Integer(Integer::I64));
    let matrix = Type::new(
        database,
        TypeKind::Array {
            element: f64,
            rank: 2,
        },
    );
    let mut body = Body::new(none, span());
    let grid = body.push_local(
        matrix,
        LocalKind::Parameter {
            name: "grid".into(),
            mode: Mode::InOut,
        },
        span(),
    );
    let [start, length, step, index] =
        [(); 4].map(|()| body.push_local(i64, LocalKind::Temporary, span()));
    let view = Place::from(grid).project(Projection::Slice(vec![
        SliceIndex::Range {
            start,
            length,
            step,
        },
        SliceIndex::Index(index),
    ]));
    (body, view)
}

/// `grid[:, k] += 1.0` keeps the view as the destination and a leaf of the whole-array
/// expression, and the text form shows each range as its start, length and step.
#[test]
fn a_view_is_written_by_a_whole_array_expression() {
    let database = Database::default();
    let (mut body, view) = column(&database);
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let entry = body.push_block(end(TerminatorKind::Return));
    body.blocks[entry.index()].statements = vec![assign(
        view.clone(),
        Rvalue::Elementwise(Box::new(Elementwise::Binary(
            crate::BinaryOperator::Add,
            Box::new(Elementwise::Leaf(Operand::Copy(view.clone()))),
            Box::new(Elementwise::Leaf(Operand::Constant(
                crate::Constant::float(f64, 1.0),
            ))),
        ))),
    )];

    assert_eq!(body.validate(&database), Vec::new());
    let text = body.display(&database).to_string();
    assert!(text.contains(
        "_1[_2 for _3 by _4, _5] = elementwise Add(copy _1[_2 for _3 by _4, _5], 1.0_f64);"
    ));
}

/// A view may be passed to a call as a borrow and have its length read, and one element of it
/// is an ordinary place, which may be copied.
#[test]
fn a_view_may_be_borrowed_and_measured() {
    let database = Database::default();
    let (mut body, view) = column(&database);
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let (length, element) = (
        body.push_local(i64, LocalKind::Temporary, span()),
        body.push_local(f64, LocalKind::Temporary, span()),
    );
    let call = body.push_block(end(TerminatorKind::Call {
        callee: Callee::Builtin(crate::Builtin::Sum),
        arguments: vec![Argument::InOut(view.clone())],
        destination: element.into(),
        target: BlockId::new(1),
        error: None,
    }));
    body.blocks[call.index()].statements = vec![
        assign(length, Rvalue::Length(view.clone(), 0)),
        assign(
            element,
            Rvalue::Use(Operand::Copy(
                view.project(Projection::Index(vec![Local::new(2)])),
            )),
        ),
    ];
    body.push_block(end(TerminatorKind::Return));

    assert_eq!(body.validate(&database), Vec::new());
}

/// Copying, moving, dropping or assigning a view whole is reported: each would store a borrow
/// or free memory the view does not own.
#[test]
fn a_view_is_never_a_whole_value() {
    let database = Database::default();
    let uses: [fn(Place, Local) -> StatementKind<'static>; 4] = [
        |view, copy| StatementKind::Assign(copy.into(), Rvalue::Use(Operand::Copy(view))),
        |view, copy| StatementKind::Assign(copy.into(), Rvalue::Use(Operand::Move(view))),
        |view, _| StatementKind::Drop(view),
        |view, copy| StatementKind::Assign(view, Rvalue::Use(Operand::Copy(copy.into()))),
    ];
    for kind in uses {
        let (mut body, view) = column(&database);
        let vector = Type::new(
            &database,
            TypeKind::Array {
                element: primitive(&database, Primitive::Float(Float::F64)),
                rank: 1,
            },
        );
        let copy = body.push_local(vector, LocalKind::Temporary, span());
        let entry = body.push_block(end(TerminatorKind::Return));
        body.blocks[entry.index()].statements = vec![Statement {
            kind: kind(view, copy),
            span: span(),
        }];
        let problems = body.validate(&database);
        assert_eq!(problems, [Invalid::WholeView(entry)]);
        assert_eq!(problems[0].to_string(), "bb0 uses a view as a whole value");
    }
}

/// A slice's locals are checked to exist like an index's.
#[test]
fn a_slice_names_only_locals_that_exist() {
    let database = Database::default();
    let (mut body, _) = column(&database);
    let missing = Local::new(9);
    let view = Place::from(Local::new(1)).project(Projection::Slice(vec![
        SliceIndex::Index(Local::new(2)),
        SliceIndex::Range {
            start: Local::new(3),
            length: Local::new(4),
            step: missing,
        },
    ]));
    let entry = body.push_block(end(TerminatorKind::Return));
    body.blocks[entry.index()].statements = vec![assign(
        view,
        Rvalue::Elementwise(Box::new(Elementwise::Leaf(Operand::Copy(Place::from(
            Local::new(1),
        ))))),
    )];
    assert_eq!(
        body.validate(&database),
        [Invalid::UnknownLocal(entry, missing)]
    );
}

/// The two slice checks: the range lies within the dimension, reporting where it starts and
/// stops and the length, and the step is not 0.
#[test]
fn slice_checks_report_the_range() {
    let database = Database::default();
    let (mut body, _) = column(&database);
    let bool = primitive(&database, Primitive::Bool);
    let inside = body.push_local(bool, LocalKind::Temporary, span());
    let check = Check::SliceInBounds {
        dimension: 1,
        start: Operand::Copy(Local::new(2).into()),
        stop: Operand::Copy(Local::new(3).into()),
        length: Operand::Copy(Local::new(4).into()),
    };
    assert_eq!(check.operands().len(), 3);
    assert!(Check::NonZeroStep.operands().is_empty());
    body.push_block(end(TerminatorKind::Assert {
        condition: Operand::Copy(inside.into()),
        check,
        target: BlockId::new(1),
    }));
    body.push_block(end(TerminatorKind::Assert {
        condition: Operand::Copy(inside.into()),
        check: Check::NonZeroStep,
        target: BlockId::new(2),
    }));
    body.push_block(end(TerminatorKind::Return));

    assert_eq!(body.validate(&database), Vec::new());
    let text = body.display(&database).to_string();
    assert!(text.contains(
        "assert(copy _6, slice in bounds in dimension 1: copy _2:copy _3 within copy _4) -> bb1;"
    ));
    assert!(text.contains("assert(copy _6, non-zero step) -> bb2;"));
}
