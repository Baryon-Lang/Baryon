//! Unit-stride loop versions (roadmap 3.3.1): runs an innermost counted loop in a copy where
//! every array it can reach has a last stride of 1, chosen by one test of the strides before it.
//!
//! An element's address is the sum of each index times its dimension's stride, read from the
//! array's header, since a view may skip elements. Almost every array a loop reads is a whole
//! array, whose last stride is 1, and then the last multiplication does nothing. The copy marks
//! its blocks in [`Body::unit_strides`](mir::Body::unit_strides), and the backend leaves that
//! multiplication out there:
//!
//! ```text
//! pre-check: unit = stride(a, 0) == 1 & stride(p.values, 0) == 1 & ...
//!            branch unit -> [true: the copy, marked, false: the loop as it was]
//! ```
//!
//! # Which arrays
//!
//! Each `Array` the loop indexes, in a statement's destination or operands or a call's
//! arguments, that is a local the loop does not replace or a field of one, one or two levels
//! down. Lists, dicts and `Fixed` arrays have no strides and are left out. An index the walk
//! does not see, such as one inside a format string, keeps its multiplication, which is only
//! slower.
//!
//! # Why it changes nothing a program can see
//!
//! The copy runs only when each marked array's last stride is 1, and nothing in the loop can
//! replace one of those arrays, so leaving out a multiplication by 1 finds the same elements in
//! the same order. Writing elements keeps an array's strides.
//!
//! # Cost
//!
//! Only innermost loops are versioned, so the code of each grows to twice its size at most; the
//! checks hoisting before it can already have doubled it. A loop that allocates an array is
//! left as it is: allocating costs far more than the multiplications.

use crate::loops::{CountedLoop, Headers, counted_loops, for_each_target, writes};
use mir::{
    Argument, BinaryOperator, BlockId, Body, Constant, Local, LocalKind, Operand, Place,
    Projection, Rvalue, Statement, StatementKind, Terminator, TerminatorKind,
};
use types::{Declared, Integer, Nominal, Primitive, Type, TypeKind};

/// How many fields down from a local an array is looked for: `p.values`, or `s.grid.u`.
const DEEPEST_FIELD: usize = 2;

/// Versions every innermost counted loop of `body` that names an `Array` it does not replace,
/// as the module documentation describes. `declared` says what each struct declares.
pub(crate) fn version_all<'db>(
    db: &'db dyn salsa::Database,
    body: &mut Body<'db>,
    declared: &dyn Fn(&Nominal<'db>) -> Declared<'db>,
) {
    let mut done = Vec::new();
    loop {
        let mut headers = Headers::new(body);
        let next = counted_loops(body, &done.clone()).find_map(|counted| {
            let innermost = counted
                .blocks
                .iter()
                .all(|&block| block == counted.header || !headers.is_header(block));
            let arrays = if innermost && !allocates(body, &counted) {
                arrays(db, body, &counted, declared)
            } else {
                Vec::new()
            };
            if arrays.is_empty() {
                done.push(counted.header);
                None
            } else {
                Some((counted, arrays))
            }
        });
        let Some((counted, arrays)) = next else {
            return;
        };
        done.push(counted.header);
        let copy = copy(db, body, &counted, &arrays);
        done.push(copy);
    }
}

/// Whether `counted` allocates an array on some pass. Such a loop spends its time allocating
/// and filling, not in the multiplications a copy would save, so it is left as it is, which
/// also keeps each allocation written once in the program.
fn allocates(body: &Body<'_>, counted: &CountedLoop<'_>) -> bool {
    counted.blocks.iter().any(|&block| {
        body.blocks[block].statements.iter().any(|statement| {
            matches!(
                statement.kind,
                StatementKind::Assign(_, Rvalue::NewArray(..))
            )
        })
    })
}

/// The arrays of `counted` whose strides a pre-check tests: each place, a local the loop names
/// and does not replace or a field of one, that holds an `Array`, with its rank.
fn arrays<'db>(
    db: &'db dyn salsa::Database,
    body: &Body<'db>,
    counted: &CountedLoop<'db>,
    declared: &dyn Fn(&Nominal<'db>) -> Declared<'db>,
) -> Vec<(Place, u32)> {
    let mut named: Vec<Local> = Vec::new();
    for &block in &counted.blocks {
        let data = &body.blocks[block];
        let statements = data
            .statements
            .iter()
            .flat_map(|statement| statement.kind.locals());
        for local in statements.chain(data.terminator.kind.locals()) {
            if !named.contains(&local) {
                named.push(local);
            }
        }
    }
    let mut found = Vec::new();
    for local in named {
        if writes(body, &counted.blocks, local) {
            continue;
        }
        let ty = body.locals[local.index()].ty;
        collect(db, declared, Place::from(local), ty, &mut found);
    }
    let indexed = indexed(body, counted);
    found.retain(|(place, _)| indexed.contains(place));
    found
}

/// Every place `counted` indexes an element of: the part before an
/// [`Index`](Projection::Index) step of each place its statements assign or read through an
/// operand, and of each argument its calls pass.
fn indexed(body: &Body<'_>, counted: &CountedLoop<'_>) -> Vec<Place> {
    let mut places: Vec<&Place> = Vec::new();
    for &block in &counted.blocks {
        let data = &body.blocks[block];
        for statement in &data.statements {
            if let StatementKind::Assign(place, rvalue) = &statement.kind {
                places.push(place);
                places.extend(operands(rvalue).into_iter().filter_map(read));
            }
        }
        match &data.terminator.kind {
            TerminatorKind::Call { arguments, .. } => {
                for argument in arguments {
                    places.extend(match argument {
                        Argument::Borrow(place) | Argument::InOut(place) => Some(place),
                        Argument::Owned(operand) => read(operand),
                    });
                }
            }
            TerminatorKind::Branch { condition, .. } => places.extend(read(condition)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    for place in places {
        let index = place
            .projections
            .iter()
            .position(|step| matches!(step, Projection::Index(_)));
        if let Some(index) = index {
            let base = Place {
                local: place.local,
                projections: place.projections[..index].to_vec(),
            };
            if !found.contains(&base) {
                found.push(base);
            }
        }
    }
    found
}

/// The operands of `rvalue` a walk for indexed places reads.
fn operands<'a, 'db>(rvalue: &'a Rvalue<'db>) -> Vec<&'a Operand<'db>> {
    match rvalue {
        Rvalue::Use(operand) | Rvalue::Unary(_, operand) | Rvalue::Convert(operand, _) => {
            vec![operand]
        }
        Rvalue::Binary(_, left, right) | Rvalue::CheckedBinary(_, left, right) => {
            vec![left, right]
        }
        Rvalue::Aggregate(_, operands) => operands.iter().collect(),
        _ => Vec::new(),
    }
}

/// The place `operand` reads, if it reads one.
fn read<'a>(operand: &'a Operand<'_>) -> Option<&'a Place> {
    match operand {
        Operand::Copy(place) | Operand::Move(place) => Some(place),
        Operand::Constant(_) => None,
    }
}

/// Adds `place`, of type `ty`, to `found` when it is an `Array`, or the `Array`s among its
/// fields when it is a struct or tuple not yet [`DEEPEST_FIELD`] fields down.
fn collect<'db>(
    db: &'db dyn salsa::Database,
    declared: &dyn Fn(&Nominal<'db>) -> Declared<'db>,
    place: Place,
    ty: Type<'db>,
    found: &mut Vec<(Place, u32)>,
) {
    let fields = match ty.kind(db) {
        TypeKind::Array { rank, .. } if *rank > 0 => {
            found.push((place, *rank));
            return;
        }
        TypeKind::Tuple(items) => items.clone(),
        TypeKind::Struct(nominal) => match declared(nominal) {
            Declared::Struct(fields) => fields,
            Declared::Enum { .. } => return,
        },
        _ => return,
    };
    if place.projections.len() >= DEEPEST_FIELD {
        return;
    }
    for (position, field) in (0..).zip(fields) {
        let inner = place.clone().project(Projection::Field(position));
        collect(db, declared, inner, field, found);
    }
}

/// Adds a copy of `counted` whose blocks mark `arrays` as of unit stride, and a block before
/// both that tests their last strides and picks the copy when every one is 1. Returns the
/// copy's header.
fn copy<'db>(
    db: &'db dyn salsa::Database,
    body: &mut Body<'db>,
    counted: &CountedLoop<'db>,
    arrays: &[(Place, u32)],
) -> usize {
    let first = body.blocks.len();
    let copy_of = |block: usize| {
        counted
            .blocks
            .iter()
            .position(|&member| member == block)
            .map(|position| first + position)
    };
    for (position, &block) in counted.blocks.iter().enumerate() {
        let mut copied = body.blocks[block].clone();
        for_each_target(&mut copied.terminator.kind, &mut |target| {
            if let Some(index) = copy_of(target.index()) {
                *target = BlockId::new(index);
            }
        });
        body.blocks.push(copied);
        let id = BlockId::new(first + position);
        if body.vectorized.contains(&BlockId::new(block)) {
            body.vectorized.push(id);
        }
        for (place, _) in arrays {
            body.unit_strides.push((id, place.clone()));
        }
    }
    let header = copy_of(counted.header).unwrap_or(first);
    let span = body.blocks[counted.header].terminator.span;
    let int = Type::new(db, TypeKind::Primitive(Primitive::Integer(Integer::I64)));
    let boolean = Type::new(db, TypeKind::Primitive(Primitive::Bool));
    let mut statements = Vec::new();
    let mut all: Option<Operand<'db>> = None;
    let mut assign = |body: &mut Body<'db>, ty: Type<'db>, rvalue: Rvalue<'db>| {
        let local = body.push_local(ty, LocalKind::Temporary, span);
        statements.push(Statement {
            kind: StatementKind::Assign(Place::from(local), rvalue),
            span,
        });
        Operand::Copy(Place::from(local))
    };
    for (place, rank) in arrays {
        let stride = assign(body, int, Rvalue::Stride(place.clone(), rank - 1));
        let one = Operand::Constant(Constant::integer(int, 1));
        let unit = assign(
            body,
            boolean,
            Rvalue::Binary(BinaryOperator::Equal, stride, one),
        );
        all = Some(match all {
            None => unit,
            Some(earlier) => assign(
                body,
                boolean,
                Rvalue::Binary(BinaryOperator::BitAnd, earlier, unit),
            ),
        });
    }
    let Some(condition) = all else {
        return header;
    };
    let pre_check = body.push_block(Terminator {
        kind: TerminatorKind::Branch {
            condition,
            then: BlockId::new(header),
            otherwise: BlockId::new(counted.header),
        },
        span,
    });
    body.blocks[pre_check.index()].statements = statements;
    for &entry in &counted.entries {
        for_each_target(&mut body.blocks[entry].terminator.kind, &mut |target| {
            if target.index() == counted.header {
                *target = pre_check;
            }
        });
    }
    header
}
