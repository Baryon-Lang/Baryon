//! Tests of [`Fused`](crate::Elementwise::Fused) subexpressions: their text form, and that the
//! structural rules and [`StatementKind::locals`] see the leaves inside them.

use super::{Database, assign, end, primitive, span};
use crate::{
    BinaryOperator, Body, Elementwise, Invalid, Local, LocalKind, Operand, Phase, Rvalue,
    TerminatorKind,
};
use types::{Float, Mode, Primitive, Type, TypeKind};

/// `fused(a * b) + c` writes the subexpression inside `fused(...)`, the leaves inside it count
/// as the statement's locals, a leaf the body does not have is reported, and a ready body may
/// not hold it.
#[test]
fn fused_subexpressions_are_part_of_their_expression() {
    let database = Database::default();
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let vector = Type::new(
        &database,
        TypeKind::Array {
            element: f64,
            rank: 1,
        },
    );
    let mut body = Body::new(vector, span());
    let parameter = |name: &str| LocalKind::Parameter {
        name: name.into(),
        mode: Mode::Borrow,
    };
    let a = body.push_local(vector, parameter("a"), span());
    let b = body.push_local(vector, parameter("b"), span());
    let c = body.push_local(vector, parameter("c"), span());
    let entry = body.push_block(end(TerminatorKind::Return));
    let leaf = |local: Local| Box::new(Elementwise::Leaf(Operand::Copy(local.into())));
    let tree = |left: Local| {
        Rvalue::Elementwise(Box::new(Elementwise::Binary(
            BinaryOperator::Add,
            Box::new(Elementwise::Fused(
                Box::new(Elementwise::Binary(
                    BinaryOperator::Multiply,
                    leaf(left),
                    leaf(b),
                )),
                span(),
            )),
            leaf(c),
        )))
    };
    body.blocks[entry.index()].statements = vec![assign(Local::RETURN, tree(a))];

    assert_eq!(body.validate(&database), Vec::new());
    assert!(
        body.display(&database)
            .to_string()
            .contains("_0 = elementwise Add(fused(Multiply(copy _1, copy _2)), copy _3);")
    );
    assert_eq!(
        body.blocks[entry.index()].statements[0].kind.locals(),
        vec![Local::RETURN, a, b, c]
    );

    let unknown = Local::new(9);
    body.blocks[entry.index()].statements = vec![assign(Local::RETURN, tree(unknown))];
    assert_eq!(
        body.validate(&database),
        vec![Invalid::UnknownLocal(entry, unknown)]
    );

    body.blocks[entry.index()].statements = vec![assign(Local::RETURN, tree(a))];
    body.phase = Phase::Ready;
    assert_eq!(body.validate(&database), vec![Invalid::NotExpanded(entry)]);
}
