//! Finds one temporary that can be fused, and fuses it.

use mir::{
    BasicBlock, Body, Elementwise, Local, LocalKind, Operand, Place, Rvalue, Statement,
    StatementKind,
};
use source::Span;
use types::{Primitive, TypeKind};

/// Where a temporary's whole-array expression is, and the statement that stores it.
struct Definition<'db> {
    /// The statements to remove: the one computing the expression, and, when it computes into
    /// an unnamed temporary moved into a named variable, the move, `t = move _8`.
    removed: Vec<usize>,
    /// The expression.
    tree: Elementwise<'db>,
    /// The span of the statement that computes it.
    span: Span,
}

/// Fuses one temporary in `block` into the statement that reads it, and says whether it did.
/// `mentions` counts how often each local of `body` is named anywhere in it.
pub(crate) fn fuse_one(
    db: &dyn salsa::Database,
    body: &mut Body<'_>,
    block: usize,
    mentions: &[usize],
) -> bool {
    let statements = &body.blocks[block].statements;
    for (position, statement) in statements.iter().enumerate() {
        let StatementKind::Assign(_, Rvalue::Elementwise(tree)) = &statement.kind else {
            continue;
        };
        for candidate in whole_leaves(tree) {
            let Some(definition) = definition(db, body, block, position, candidate, mentions)
            else {
                continue;
            };
            replace(&mut body.blocks[block], position, candidate, definition);
            return true;
        }
    }
    false
}

/// The local of every leaf of `tree` that reads a whole local, `copy _5` or `move _5`, outside
/// fused subexpressions.
fn whole_leaves(tree: &Elementwise<'_>) -> Vec<Local> {
    match tree {
        Elementwise::Leaf(Operand::Copy(place) | Operand::Move(place))
            if place.projections.is_empty() =>
        {
            vec![place.local]
        }
        Elementwise::Leaf(_) | Elementwise::Fused(..) => Vec::new(),
        Elementwise::Unary(_, operand) => whole_leaves(operand),
        Elementwise::Binary(_, left, right) => {
            let mut leaves = whole_leaves(left);
            leaves.extend(whole_leaves(right));
            leaves
        }
    }
}

/// The definition of `temporary`, read by the statement at `use_at` of `block`, when it can be
/// fused there:
///
/// 1. it is a variable or an unnamed temporary of a `float` or `complex` array type, so no
///    operation in its expression can panic, and computing its elements later, between the
///    reader's, changes nothing anyone can see;
/// 2. it is assigned once in the whole body, earlier in the same block, a whole-array
///    expression either directly or through an unnamed temporary that holds nothing else;
/// 3. apart from that assignment and its drops, it is named once in the whole body, by the
///    reader, so its value is needed nowhere else;
/// 4. no statement between the assignment and the reader names a local the expression reads,
///    so the expression gives the same elements when the reader computes them.
fn definition<'db>(
    db: &dyn salsa::Database,
    body: &Body<'db>,
    block: usize,
    use_at: usize,
    temporary: Local,
    mentions: &[usize],
) -> Option<Definition<'db>> {
    let declaration = body.locals.get(temporary.index())?;
    if !matches!(
        declaration.kind,
        LocalKind::Variable { .. } | LocalKind::Temporary
    ) || !has_float_elements(db, declaration.ty)
    {
        return None;
    }
    let statements = &body.blocks[block].statements;
    let assigned_at = statements[..use_at]
        .iter()
        .rposition(|statement| assigns(statement, temporary))?;
    let (removed, tree, span) = match &statements[assigned_at].kind {
        StatementKind::Assign(_, Rvalue::Elementwise(tree)) => {
            (vec![assigned_at], tree, statements[assigned_at].span)
        }
        StatementKind::Assign(_, Rvalue::Use(Operand::Move(source)))
            if source.projections.is_empty()
                && body.locals.get(source.local.index())?.kind == LocalKind::Temporary
                && mentions.get(source.local.index()) == Some(&2) =>
        {
            let computed_at = statements[..assigned_at]
                .iter()
                .rposition(|statement| assigns(statement, source.local))?;
            let StatementKind::Assign(_, Rvalue::Elementwise(tree)) = &statements[computed_at].kind
            else {
                return None;
            };
            (
                vec![computed_at, assigned_at],
                tree,
                statements[computed_at].span,
            )
        }
        _ => return None,
    };
    let drops = count_drops(body, temporary);
    if mentions.get(temporary.index()) != Some(&(drops + 2)) {
        return None;
    }
    let read = read_locals(&statements[removed[0]]);
    let untouched = statements[removed[0] + 1..use_at]
        .iter()
        .enumerate()
        .filter(|&(offset, _)| !removed.contains(&(removed[0] + 1 + offset)))
        .all(|(offset, statement)| {
            let named = statement.kind.locals();
            let before_assignment = removed[0] + 1 + offset < assigned_at;
            let drops_old_value = before_assignment && is_drop_of(statement, temporary);
            drops_old_value
                || !named
                    .iter()
                    .any(|local| *local == temporary || read.contains(local))
        });
    untouched.then(|| Definition {
        removed,
        tree: (**tree).clone(),
        span,
    })
}

/// Whether `ty` is an array of floats or complex numbers.
fn has_float_elements(db: &dyn salsa::Database, ty: types::Type<'_>) -> bool {
    let element = match ty.kind(db) {
        TypeKind::Array { element, .. } | TypeKind::Fixed { element, .. } => *element,
        _ => return false,
    };
    matches!(
        element.kind(db),
        TypeKind::Primitive(Primitive::Float(_) | Primitive::Complex(_))
    )
}

/// Whether `statement` assigns the whole of `local`.
fn assigns(statement: &Statement<'_>, local: Local) -> bool {
    matches!(&statement.kind, StatementKind::Assign(place, _) if *place == Place::from(local))
}

/// Whether `statement` drops the whole of `local`.
fn is_drop_of(statement: &Statement<'_>, local: Local) -> bool {
    matches!(&statement.kind, StatementKind::Drop(place) if *place == Place::from(local))
}

/// How many statements of `body` drop the whole of `local`.
fn count_drops(body: &Body<'_>, local: Local) -> usize {
    body.blocks
        .iter()
        .flat_map(|block| &block.statements)
        .filter(|statement| is_drop_of(statement, local))
        .count()
}

/// The locals the statement that computes a temporary reads: every local it names but the
/// temporary it assigns, which comes first.
fn read_locals(statement: &Statement<'_>) -> Vec<Local> {
    statement.kind.locals().into_iter().skip(1).collect()
}

/// Puts `definition`'s expression, fused, in place of the leaf that reads `temporary` in the
/// statement at `use_at`, and removes the statements that computed and stored it.
fn replace<'db>(
    block: &mut BasicBlock<'db>,
    use_at: usize,
    temporary: Local,
    definition: Definition<'db>,
) {
    if let StatementKind::Assign(_, Rvalue::Elementwise(tree)) = &mut block.statements[use_at].kind
    {
        let fused = Elementwise::Fused(Box::new(definition.tree), definition.span);
        substitute(tree, temporary, &mut Some(fused));
    }
    for &position in definition.removed.iter().rev() {
        block.statements.remove(position);
    }
}

/// Replaces the leaf of `tree` that reads the whole of `temporary` with `fused`, taken once.
fn substitute<'db>(
    tree: &mut Elementwise<'db>,
    temporary: Local,
    fused: &mut Option<Elementwise<'db>>,
) {
    match tree {
        Elementwise::Leaf(Operand::Copy(place) | Operand::Move(place))
            if *place == Place::from(temporary) =>
        {
            if let Some(replacement) = fused.take() {
                *tree = replacement;
            }
        }
        Elementwise::Leaf(_) | Elementwise::Fused(..) => {}
        Elementwise::Unary(_, operand) => substitute(operand, temporary, fused),
        Elementwise::Binary(_, left, right) => {
            substitute(left, temporary, fused);
            substitute(right, temporary, fused);
        }
    }
}
