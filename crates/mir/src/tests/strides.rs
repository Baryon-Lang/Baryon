//! Tests of unit strides: reading an array's stride, and the blocks where an array is known to
//! have a last stride of 1, in the text form and the validator.

use super::{Database, end, primitive, span};
use crate::{
    BlockId, Body, Invalid, LocalKind, Place, Projection, Rvalue, Statement, StatementKind,
    TerminatorKind,
};
use types::{Float, Integer, Primitive, Type, TypeKind};

/// A body with a rank-1 array `a` that reads its stride into `s`, then returns; `bb0` is
/// marked as holding `a` and `a.0` with a unit stride when `marked`.
fn strided(database: &Database, marked: bool) -> Body<'_> {
    let none = Type::new(database, TypeKind::None);
    let int = primitive(database, Primitive::Integer(Integer::I64));
    let element = primitive(database, Primitive::Float(Float::F64));
    let array = Type::new(database, TypeKind::Array { element, rank: 1 });
    let mut body = Body::new(none, span());
    let a = body.push_local(array, LocalKind::Variable { name: "a".into() }, span());
    let s = body.push_local(int, LocalKind::Temporary, span());
    let block = body.push_block(end(TerminatorKind::Return));
    body.blocks[block.index()].statements.push(Statement {
        kind: StatementKind::Assign(s.into(), Rvalue::Stride(a.into(), 0)),
        span: span(),
    });
    if marked {
        body.unit_strides.push((block, Place::from(a)));
        let field = Place::from(a).project(Projection::Field(0));
        body.unit_strides.push((block, field));
    }
    body
}

/// A stride is written `stride(place, dimension)`, and a marked block lists its arrays.
#[test]
fn a_stride_and_a_unit_stride_mark_are_written() {
    let database = Database::default();
    let body = strided(&database, true);
    assert_eq!(body.validate(&database), []);
    let text = body.display(&database).to_string();
    assert!(text.contains("_2 = stride(_1, 0)"), "{text}");
    assert!(
        text.contains("\n    bb0 (unit stride _1, _1.0): {\n"),
        "{text}"
    );
    let unmarked = strided(&database, false).display(&database).to_string();
    assert!(unmarked.contains("\n    bb0: {\n"), "{unmarked}");
}

/// A mark on a block or local the body does not have is reported.
#[test]
fn a_unit_stride_mark_on_a_missing_block_or_local_is_invalid() {
    let database = Database::default();
    let mut body = strided(&database, false);
    body.unit_strides
        .push((BlockId::new(4), Place::from(crate::Local::new(1))));
    body.unit_strides
        .push((BlockId::new(0), Place::from(crate::Local::new(7))));
    let problems = body.validate(&database);
    assert_eq!(
        problems,
        [
            Invalid::UnknownUnitStride(BlockId::new(4)),
            Invalid::UnknownUnitStride(BlockId::new(0))
        ]
    );
    assert_eq!(
        problems[0].to_string(),
        "bb4 is marked as holding an array of unit stride, but it or the array's local does not \
         exist"
    );
}
