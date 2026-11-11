//! Writes `x = f(x, ...)` into `x`'s own array instead of a new one.

use mir::{
    Body, Constant, Elementwise, Local, LocalKind, Operand, Place, Projection, Rvalue, SliceIndex,
    Statement, StatementKind,
};
use types::{Integer, Mode, Primitive, Type, TypeKind};

/// Rewrites one update in `block` to write in place, and says whether it did. `mentions`
/// counts how often each local of `body` is named anywhere in it.
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
/// Before, `x`'s old array was freed and a new one allocated every time the line ran.
pub(crate) fn update_one<'db>(
    db: &'db dyn salsa::Database,
    body: &mut Body<'db>,
    block: usize,
    mentions: &[usize],
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
    }
    let view = Place::from(target).project(Projection::Slice(ranges));
    let statements = &mut body.blocks[block].statements;
    let StatementKind::Assign(_, Rvalue::Elementwise(tree)) = &statements[position].kind else {
        return false;
    };
    let mut tree = (**tree).clone();
    read_view(&mut tree, target, &view);
    written.push(Statement {
        kind: StatementKind::Assign(
            view,
            Rvalue::Elementwise(Box::new(Elementwise::Fused(Box::new(tree), span))),
        ),
        span,
    });
    statements.splice(position..position + 3, written);
    true
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
