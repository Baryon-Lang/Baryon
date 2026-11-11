//! Tests of expanding a whole-array expression whose destination is a view: written in place,
//! or through a new array when a leaf may overlap the view.

use super::{Database, expanded, leaf, span};
use mir::{
    Body, Elementwise, Local, LocalKind, Operand, Place, Projection, Rvalue, SliceIndex, Statement,
    StatementKind, Terminator, TerminatorKind,
};
use types::{Float, Mode, Primitive, Type, TypeKind};

/// A body of `None` with the parameter `a: InOut[Array[f64, 1]]` (local 1) and `i64` locals 2
/// to 7: two ranges of `a`, `a[_2 for _3 by _4]` and `a[_5 for _6 by _7]`. Its one block
/// assigns `tree` to the first view.
fn into_view<'db>(
    database: &'db Database,
    tree: impl FnOnce(&Place, &Place) -> Elementwise<'db>,
) -> Body<'db> {
    let f64 = Type::new(database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    let i64 = Type::new(
        database,
        TypeKind::Primitive(Primitive::Integer(types::Integer::I64)),
    );
    let vector = Type::new(
        database,
        TypeKind::Array {
            element: f64,
            rank: 1,
        },
    );
    let mut body = Body::new(Type::new(database, TypeKind::None), span());
    let a = body.push_local(
        vector,
        LocalKind::Parameter {
            name: "a".into(),
            mode: Mode::InOut,
        },
        span(),
    );
    let locals: Vec<Local> = (0..6)
        .map(|_| body.push_local(i64, LocalKind::Temporary, span()))
        .collect();
    let view = |first: usize| {
        Place::from(a).project(Projection::Slice(vec![SliceIndex::Range {
            start: locals[first],
            length: locals[first + 1],
            step: locals[first + 2],
        }]))
    };
    let (target, other) = (view(0), view(3));
    let entry = body.push_block(Terminator {
        kind: TerminatorKind::Return,
        span: span(),
    });
    body.blocks[entry.index()].statements.push(Statement {
        kind: StatementKind::Assign(
            target.clone(),
            Rvalue::Elementwise(Box::new(tree(&target, &other))),
        ),
        span: span(),
    });
    body
}

/// `a[i:j] = 0.5`: a number alone is written into each element of the view, whose own length
/// sets the loop; nothing is allocated.
#[test]
fn a_number_fills_a_view_in_place() {
    let database = Database::default();
    let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    let body = into_view(&database, |_, _| {
        leaf(Operand::Constant(mir::Constant::float(f64, 0.5)))
    });
    let text = expanded(&database, body);
    assert!(text.contains("_8 = len(_1[_2 for _3 by _4], 0);"), "{text}");
    assert!(
        text.contains("_1[_2 for _3 by _4][_9] = 0.5_f64;"),
        "{text}"
    );
    assert!(!text.contains("new"), "{text}");
}

/// `a[i:j] += a[i:j]`: the only leaf of `a` is the view itself, which each element reads just
/// before writing, so the expression is written in place, with no length to check.
#[test]
fn the_view_itself_is_read_in_place() {
    let database = Database::default();
    let body = into_view(&database, |target, _| {
        Elementwise::Binary(
            mir::BinaryOperator::Add,
            Box::new(leaf(Operand::Copy(target.clone()))),
            Box::new(leaf(Operand::Copy(target.clone()))),
        )
    });
    let text = expanded(&database, body);
    assert!(!text.contains("same length"), "{text}");
    assert!(
        text.contains("_11 = Add(copy _1[_2 for _3 by _4][_9], copy _1[_2 for _3 by _4][_9]);"),
        "{text}"
    );
    assert!(
        text.contains("_1[_2 for _3 by _4][_9] = copy _11;"),
        "{text}"
    );
    assert!(!text.contains("new"), "{text}");
}

/// `a[i:j] = a[k:l]`: the leaf is another view of `a`, which may overlap the destination, so
/// the elements go to a new array first, checked to have the view's length or 1, and are copied
/// into the view after; the new array is dropped. A leaf of length 1 repeats, through the
/// broadcasting nest, which multiplies its index by 0.
#[test]
fn an_overlapping_view_goes_through_a_new_array() {
    let database = Database::default();
    let body = into_view(&database, |_, other| leaf(Operand::Copy(other.clone())));
    let text = expanded(&database, body);
    assert!(
        text.contains("same length in dimension 0: copy _9 == copy _8"),
        "{text}"
    );
    assert!(text.contains("_15 = new Array[f64, 1](copy _8);"), "{text}");
    assert!(
        text.contains("_15[_16] = copy _1[_5 for _6 by _7][_16];"),
        "{text}"
    );
    assert!(
        text.contains("_1[_2 for _3 by _4][_21] = copy _15[_21];"),
        "{text}"
    );
    assert!(
        text.contains("_15[_18] = copy _1[_5 for _6 by _7][_20];"),
        "{text}"
    );
    assert!(text.contains("drop(_15);"), "{text}");
}
