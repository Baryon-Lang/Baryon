//! Tests of `simd` loops: an ordinary loop whose header block is marked, in the text form and
//! the validator.

use super::{Database, end, primitive, span};
use crate::{BlockId, Body, Invalid, LocalKind, Operand, TerminatorKind};
use types::{Primitive, Type, TypeKind};

/// The shape of `for i in simd(range(n)):` with an empty body: the entry jumps to the header,
/// the header tests the condition, the body jumps back to it, and the exit returns.
fn looping(database: &Database) -> Body<'_> {
    let none = Type::new(database, TypeKind::None);
    let bool = primitive(database, Primitive::Bool);
    let mut body = Body::new(none, span());
    let more = body.push_local(bool, LocalKind::Temporary, span());
    let header = BlockId::new(1);
    body.push_block(end(TerminatorKind::Goto(header)));
    body.push_block(end(TerminatorKind::Branch {
        condition: Operand::Copy(more.into()),
        then: BlockId::new(2),
        otherwise: BlockId::new(3),
    }));
    body.push_block(end(TerminatorKind::Goto(header)));
    body.push_block(end(TerminatorKind::Return));
    body.vectorized.push(header);
    body
}

/// The header of a `simd` loop is written `bb1 (simd):`, and a marked block that exists is
/// valid.
#[test]
fn the_header_of_a_simd_loop_is_marked() {
    let database = Database::default();
    let body = looping(&database);
    assert_eq!(body.validate(&database), []);
    let text = body.display(&database).to_string();
    assert!(text.contains("\n    bb1 (simd): {\n"), "{text}");
    assert!(text.contains("\n    bb2: {\n"), "{text}");
}

/// A mark on a block the body does not have is reported.
#[test]
fn a_mark_on_a_missing_block_is_invalid() {
    let database = Database::default();
    let mut body = looping(&database);
    body.vectorized.push(BlockId::new(9));
    let problems = body.validate(&database);
    assert_eq!(problems, [Invalid::UnknownVectorized(BlockId::new(9))]);
    assert_eq!(
        problems[0].to_string(),
        "bb9 is marked as a `simd` loop header but does not exist"
    );
}
