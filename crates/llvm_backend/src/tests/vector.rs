//! `simd` loops: the back edge to a marked loop header carries loop metadata asking LLVM to
//! vectorize the loop.

use super::{Database, end, module, parameter, primitive, span};
use mir::{BinaryOperator, BlockId, Body, Constant, Operand, Rvalue, TerminatorKind};
use types::{Integer, Mode, Primitive, Type, TypeKind};

/// `def count(n: i64)`: a loop that counts `i` from 0 to `n`, with its header, `bb1`, marked as
/// a `simd` loop's.
fn counting(database: &Database) -> Body<'_> {
    let i64 = primitive(database, Primitive::Integer(Integer::I64));
    let bool = primitive(database, Primitive::Bool);
    let none = Type::new(database, TypeKind::None);
    let mut body = Body::new(none, span());
    let n = body.push_local(i64, parameter("n", Mode::Borrow), span());
    let i = body.push_local(i64, mir::LocalKind::Temporary, span());
    let more = body.push_local(bool, mir::LocalKind::Temporary, span());
    let header = BlockId::new(1);
    body.push_block(end(TerminatorKind::Goto(header)));
    body.push_block(end(TerminatorKind::Branch {
        condition: Operand::Copy(more.into()),
        then: BlockId::new(2),
        otherwise: BlockId::new(3),
    }));
    body.push_block(end(TerminatorKind::Goto(header)));
    body.push_block(end(TerminatorKind::Return));
    body.blocks[0].statements.push(super::assign(
        i,
        Rvalue::Use(Operand::Constant(Constant::integer(i64, 0))),
    ));
    body.blocks[1].statements.push(super::assign(
        more,
        Rvalue::Binary(
            BinaryOperator::Less,
            Operand::Copy(i.into()),
            Operand::Copy(n.into()),
        ),
    ));
    body.blocks[2].statements.push(super::assign(
        i,
        Rvalue::Binary(
            BinaryOperator::Add,
            Operand::Copy(i.into()),
            Operand::Constant(Constant::integer(i64, 1)),
        ),
    ));
    body.vectorized.push(header);
    body
}

/// Only the back edge carries the metadata, not the entry's jump to the header, and two
/// `simd` loops get two distinct nodes, written after the code.
#[test]
fn the_back_edge_asks_llvm_to_vectorize() {
    let database = Database::default();
    let text = module(
        &database,
        &[
            ("first", counting(&database)),
            ("second", counting(&database)),
        ],
    );
    assert!(text.contains("bb0:\n"), "{text}");
    assert_eq!(text.matches("br label %bb1\n").count(), 2, "{text}");
    assert!(text.contains("  br label %bb1, !llvm.loop !0\n"), "{text}");
    assert!(text.contains("  br label %bb1, !llvm.loop !1\n"), "{text}");
    assert!(
        text.ends_with(
            "!0 = distinct !{!0, !{!\"llvm.loop.vectorize.enable\", i1 true}}\n\
             !1 = distinct !{!1, !{!\"llvm.loop.vectorize.enable\", i1 true}}\n"
        ),
        "{text}"
    );
}

/// With line tables on, loop nodes share the debug information's numbering: each takes the
/// next number after the subprogram of the function it is in.
#[test]
fn loop_nodes_share_the_debug_numbering() {
    let database = Database::default();
    let symbol = super::symbol_of(&database);
    let sources = super::sources();
    let mut module = crate::Module::new(
        &database,
        &sources,
        &super::no_structs,
        &super::no_names,
        &symbol,
    );
    module.debug_info("/home/ada/heat");
    module.define("first", &counting(&database), &[]).unwrap();
    module.define("second", &counting(&database), &[]).unwrap();
    let text = module.finish();
    assert!(
        text.contains("define void @first(i64 %a1) uwtable !dbg !3 {"),
        "{text}"
    );
    assert!(
        text.contains("br label %bb1, !llvm.loop !4, !dbg"),
        "{text}"
    );
    assert!(
        text.contains("define void @second(i64 %a1) uwtable !dbg !5 {"),
        "{text}"
    );
    assert!(
        text.contains("br label %bb1, !llvm.loop !6, !dbg"),
        "{text}"
    );
    assert!(
        text.contains("\n!3 = distinct !DISubprogram(name: \"first\""),
        "{text}"
    );
    assert!(
        text.contains("\n!5 = distinct !DISubprogram(name: \"second\""),
        "{text}"
    );
    assert!(text.contains("\n!4 = distinct !{!4, "), "{text}");
    assert!(text.contains("\n!6 = distinct !{!6, "), "{text}");
}
