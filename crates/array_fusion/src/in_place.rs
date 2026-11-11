//! Writes `x = f(x, ...)` into `x`'s own array instead of a new one.

use mir::{
    BasicBlock, BinaryOperator, Body, Constant, Elementwise, Local, LocalKind, Operand, Place,
    Projection, Rvalue, SliceIndex, Statement, StatementKind, Terminator, TerminatorKind,
};
use types::{Integer, Mode, Primitive, Type, TypeKind};

/// Rewrites one update in `block` to write in place, and says whether it did. `mentions`
/// counts how often each local of `body` is named anywhere in it. A block this adds that holds
/// the update as it was, for when broadcasting would change `x`'s shape, is added to
/// `fallbacks`, so the caller never rewrites it.
///
/// Lowering writes `x = x * k + b` as three statements: the expression into an unnamed
/// temporary, a drop of `x`'s old array, and a move of the temporary into `x`. When
///
/// 1. `x` owns its array: a variable, an unnamed temporary, or an `Owned` or `InOut` parameter,
///    of an `Array` type;
/// 2. the expression reads `x`, and only as a whole array, never a view or a field of it;
/// 3. the temporary is named nowhere else,
///
/// the three become an assignment to a view of all of `x`, which array expansion writes in
/// place. Each element of `x` is read just before it is written, at the same index, so every
/// result is the same. The expression is wrapped as [`Fused`](Elementwise::Fused) at its
/// statement's span, so its lengths are checked against its first array, with the same message,
/// before anything is written; `x` is one of its arrays, so the result has `x`'s shape.
///
/// Broadcasting (roadmap 3.9) can make the result larger than `x`: `x = x * 2.0 + b` with `x`
/// of length 1 and `b` of length 3 gives a new `x` of length 3, which `x`'s array cannot hold.
/// When the expression reads another array, the rewrite is guarded: the in-place statement runs
/// only when every other array has, in each dimension it lines up with, `x`'s length or 1, which
/// keeps `x`'s shape, and the update as it was runs otherwise. Every other array's rank must be
/// known from its local's type, so an expression that reads an array through a struct field is
/// left as it was.
///
/// Before, `x`'s old array was freed and a new one allocated every time the line ran.
pub(crate) fn update_one<'db>(
    db: &'db dyn salsa::Database,
    body: &mut Body<'db>,
    block: usize,
    mentions: &[usize],
    fallbacks: &mut Vec<usize>,
) -> bool {
    let statements = &body.blocks[block].statements;
    let Some((position, target, rank)) = (0..statements.len()).find_map(|position| {
        updated(db, body, &statements[position..], mentions)
            .map(|(target, rank)| (position, target, rank))
    }) else {
        return false;
    };
    let span = body.blocks[block].statements[position].span;
    let i64 = Type::new(db, TypeKind::Primitive(Primitive::Integer(Integer::I64)));
    let mut written = Vec::new();
    let mut constant = |body: &mut Body<'db>, value| {
        let local = body.push_local(i64, LocalKind::Temporary, span);
        written.push(Statement {
            kind: StatementKind::Assign(
                local.into(),
                Rvalue::Use(Operand::Constant(Constant::integer(i64, value))),
            ),
            span,
        });
        local
    };
    let start = constant(body, 0);
    let step = constant(body, 1);
    let mut ranges = Vec::new();
    let mut lengths = Vec::new();
    for dimension in 0..rank {
        let length = body.push_local(i64, LocalKind::Temporary, span);
        written.push(Statement {
            kind: StatementKind::Assign(
                length.into(),
                Rvalue::Length(Place::from(target), dimension),
            ),
            span,
        });
        ranges.push(SliceIndex::Range {
            start,
            length,
            step,
        });
        lengths.push(Operand::Copy(Place::from(length)));
    }
    let view = Place::from(target).project(Projection::Slice(ranges));
    let statements = &mut body.blocks[block].statements;
    let StatementKind::Assign(_, Rvalue::Elementwise(tree)) = &statements[position].kind else {
        return false;
    };
    let mut tree = (**tree).clone();
    let Some(others) = other_arrays(db, body, &tree, target) else {
        return false;
    };
    read_view(&mut tree, target, &view);
    let update = Statement {
        kind: StatementKind::Assign(
            view,
            Rvalue::Elementwise(Box::new(Elementwise::Fused(Box::new(tree), span))),
        ),
        span,
    };
    if others.is_empty() {
        written.push(update);
        body.blocks[block]
            .statements
            .splice(position..position + 3, written);
        return true;
    }
    let keeps = keeps_shape(db, body, &others, &lengths, span, &mut written);
    let data = &mut body.blocks[block];
    let rest = data.statements.split_off(position + 3);
    let original = data.statements.split_off(position);
    data.statements.extend(written);
    let terminator = std::mem::replace(
        &mut data.terminator,
        Terminator {
            kind: TerminatorKind::Unreachable,
            span,
        },
    );
    let join = body.push_block(terminator);
    body.blocks[join.index()].statements = rest;
    let goto = Terminator {
        kind: TerminatorKind::Goto(join),
        span,
    };
    let in_place = body.push_block(goto.clone());
    body.blocks[in_place.index()].statements.push(update);
    let fallback = body.push_block(goto);
    body.blocks[fallback.index()] = BasicBlock {
        statements: original,
        terminator: body.blocks[fallback.index()].terminator.clone(),
    };
    fallbacks.push(fallback.index());
    body.blocks[block].terminator = Terminator {
        kind: TerminatorKind::Branch {
            condition: keeps,
            then: in_place,
            otherwise: fallback,
        },
        span,
    };
    true
}

/// Every array `tree` reads other than the whole of `target`, with its rank, or `None` when the
/// rank of one cannot be told from its local's type.
fn other_arrays(
    db: &dyn salsa::Database,
    body: &Body<'_>,
    tree: &Elementwise<'_>,
    target: Local,
) -> Option<Vec<(Place, u32)>> {
    let mut leaves = Vec::new();
    places(tree, &mut leaves);
    let mut others = Vec::new();
    for place in leaves {
        if *place == Place::from(target) {
            continue;
        }
        let local = body.locals.get(place.local.index())?.ty;
        let rank = match (local.kind(db), place.projections.as_slice()) {
            (TypeKind::Array { rank, .. }, []) => *rank,
            (TypeKind::Array { .. } | TypeKind::Fixed { .. }, [Projection::Slice(dimensions)]) => {
                let kept = dimensions
                    .iter()
                    .filter(|dimension| dimension.in_view())
                    .count();
                u32::try_from(kept).ok()?
            }
            (TypeKind::Primitive(_), []) => continue,
            _ => return None,
        };
        others.push((place.clone(), rank));
    }
    Some(others)
}

/// The places `tree` reads, fused subexpressions included, appended to `found`.
fn places<'a>(tree: &'a Elementwise<'_>, found: &mut Vec<&'a Place>) {
    match tree {
        Elementwise::Leaf(Operand::Copy(place) | Operand::Move(place)) => found.push(place),
        Elementwise::Leaf(Operand::Constant(_)) => {}
        Elementwise::Unary(_, operand) | Elementwise::Fused(operand, _) => places(operand, found),
        Elementwise::Binary(_, left, right) => {
            places(left, found);
            places(right, found);
        }
    }
}

/// Appends to `written` the statements that compute whether every array in `others` has, in
/// each dimension, the length in `lengths` it lines up with, counted from the last, or 1; and
/// returns the `bool` that holds the answer.
fn keeps_shape<'db>(
    db: &'db dyn salsa::Database,
    body: &mut Body<'db>,
    others: &[(Place, u32)],
    lengths: &[Operand<'db>],
    span: source::Span,
    written: &mut Vec<Statement<'db>>,
) -> Operand<'db> {
    let i64 = Type::new(db, TypeKind::Primitive(Primitive::Integer(Integer::I64)));
    let bool = Type::new(db, TypeKind::Primitive(Primitive::Bool));
    let mut assign = |body: &mut Body<'db>, ty: Type<'db>, rvalue: Rvalue<'db>| {
        let local = body.push_local(ty, LocalKind::Temporary, span);
        written.push(Statement {
            kind: StatementKind::Assign(local.into(), rvalue),
            span,
        });
        Operand::Copy(Place::from(local))
    };
    let mut keeps: Option<Operand<'db>> = None;
    let one = Operand::Constant(Constant::integer(i64, 1));
    for (place, rank) in others {
        let offset = lengths.len().saturating_sub(*rank as usize);
        for (dimension, expected) in (0..*rank).zip(&lengths[offset..]) {
            let length = assign(body, i64, Rvalue::Length(place.clone(), dimension));
            let same = assign(
                body,
                bool,
                Rvalue::Binary(BinaryOperator::Equal, length.clone(), expected.clone()),
            );
            let repeats = assign(
                body,
                bool,
                Rvalue::Binary(BinaryOperator::Equal, length, one.clone()),
            );
            let fits = assign(
                body,
                bool,
                Rvalue::Binary(BinaryOperator::BitOr, same, repeats),
            );
            keeps = Some(match keeps {
                None => fits,
                Some(sofar) => assign(
                    body,
                    bool,
                    Rvalue::Binary(BinaryOperator::BitAnd, sofar, fits),
                ),
            });
        }
    }
    keeps.unwrap_or(Operand::Constant(Constant {
        ty: bool,
        value: mir::ConstantValue::Bool(true),
    }))
}

/// The local and rank of the array that the first three of `statements` update, when they are
/// an update that can be written in place, as [`update_one`] describes.
fn updated(
    db: &dyn salsa::Database,
    body: &Body<'_>,
    statements: &[Statement<'_>],
    mentions: &[usize],
) -> Option<(Local, u32)> {
    let [computed, dropped, stored, ..] = statements else {
        return None;
    };
    let StatementKind::Assign(temporary, Rvalue::Elementwise(tree)) = &computed.kind else {
        return None;
    };
    let StatementKind::Drop(old) = &dropped.kind else {
        return None;
    };
    let StatementKind::Assign(target, Rvalue::Use(Operand::Move(source))) = &stored.kind else {
        return None;
    };
    let whole = |place: &Place| place.projections.is_empty();
    if !(whole(temporary) && whole(old) && whole(target) && whole(source))
        || source != temporary
        || old != target
        || mentions.get(temporary.local.index()) != Some(&2)
        || body.locals.get(temporary.local.index())?.kind != LocalKind::Temporary
    {
        return None;
    }
    let declaration = body.locals.get(target.local.index())?;
    let owns = match &declaration.kind {
        LocalKind::Variable { .. } | LocalKind::Temporary => true,
        LocalKind::Parameter { mode, .. } => matches!(mode, Mode::Owned | Mode::InOut),
        LocalKind::Return => false,
    };
    let TypeKind::Array { rank, .. } = declaration.ty.kind(db) else {
        return None;
    };
    let reads = whole_reads(tree, target.local);
    let named = computed.kind.locals();
    let mentioned = named.iter().filter(|local| **local == target.local).count();
    (owns && reads > 0 && reads == mentioned).then_some((target.local, *rank))
}

/// How many leaves of `tree`, fused subexpressions included, copy the whole of `local`.
fn whole_reads(tree: &Elementwise<'_>, local: Local) -> usize {
    match tree {
        Elementwise::Leaf(Operand::Copy(place)) => usize::from(*place == Place::from(local)),
        Elementwise::Leaf(_) => 0,
        Elementwise::Unary(_, operand) | Elementwise::Fused(operand, _) => {
            whole_reads(operand, local)
        }
        Elementwise::Binary(_, left, right) => whole_reads(left, local) + whole_reads(right, local),
    }
}

/// Makes every leaf of `tree` that copies the whole of `local` read `view` instead.
fn read_view(tree: &mut Elementwise<'_>, local: Local, view: &Place) {
    match tree {
        Elementwise::Leaf(Operand::Copy(place)) if *place == Place::from(local) => {
            *place = view.clone();
        }
        Elementwise::Leaf(_) => {}
        Elementwise::Unary(_, operand) | Elementwise::Fused(operand, _) => {
            read_view(operand, local, view);
        }
        Elementwise::Binary(_, left, right) => {
            read_view(left, local, view);
            read_view(right, local, view);
        }
    }
}
