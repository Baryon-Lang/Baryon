//! Tests of unit-stride loop versions (roadmap 3.3.1): which arrays a loop's copy marks, and
//! what the block before it tests.

use super::{Builder, Database, a, assign, copy, counter};
use crate::hoist;
use mir::{BlockId, Body, Operand, Place, Projection, Rvalue, TerminatorKind};
use types::Declared;

/// A loop that reads `x = a[i]`, after a check of `i` against `len(a)` read in the loop; when
/// `replaced`, the loop also assigns `a` to itself, which counts as replacing it.
fn reading(database: &Database, replaced: bool) -> Body<'_> {
    let mut builder = Builder::new(database);
    let float = builder.body.locals[a().index()].ty;
    let [length, x] = [builder.int, float].map(|ty| builder.temporary(ty));
    let (mut statements, check) = builder.check(counter(), copy(length), 3);
    statements.insert(0, assign(length, Rvalue::Length(Place::from(a()), 0)));
    let element = Place::from(a()).project(Projection::Index(vec![counter()]));
    let mut read = vec![assign(x, Rvalue::Use(Operand::Copy(element)))];
    if replaced {
        read.push(assign(a(), Rvalue::Use(copy(a()))));
    }
    builder.finish(vec![
        (statements, check),
        (read, TerminatorKind::Goto(BlockId::new(4))),
    ])
}

/// Each block of the loop's unit-stride copy marks `a`, and the block before it tests
/// `stride(a, 0) == 1`.
#[test]
fn an_indexed_array_gets_a_unit_stride_copy() {
    let database = Database::default();
    let mut body = reading(&database, false);
    hoist(&database, &mut body, &|_| Declared::Struct(Vec::new()));

    assert!(body.validate(&database).is_empty());
    assert!(!body.unit_strides.is_empty());
    assert!(
        body.unit_strides
            .iter()
            .all(|(_, place)| *place == Place::from(a()))
    );
    let text = body.display(&database).to_string();
    assert!(text.contains("= stride(_3, 0);"), "{text}");
    assert!(text.contains("Equal(copy "), "{text}");
    assert!(text.contains(" (unit stride _3): {"), "{text}");
}

/// A loop that replaces `a` is not copied for it: a new array may have any strides.
#[test]
fn a_replaced_array_gets_no_copy() {
    let database = Database::default();
    let mut body = reading(&database, true);
    hoist(&database, &mut body, &|_| Declared::Struct(Vec::new()));

    assert!(body.validate(&database).is_empty());
    assert!(body.unit_strides.is_empty());
    assert!(!body.display(&database).to_string().contains("stride("));
}
