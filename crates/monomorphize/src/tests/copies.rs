//! `own_copies`: a copy of a struct with its own `copy` becomes a call of that method.

use super::{Database, named, span};
use crate::own_copies;
use database::Package;
use mir::{Argument, BlockId, Body, Builtin, Callee, LocalKind, Terminator, TerminatorKind};
use types::{Integer, Item, Mode, Nominal, Primitive, Type, TypeKind};

/// `def both(p: Point, n: i64) -> None: a = p.copy(); b = n.copy()` with `T` made `Point` and
/// `i64`, as lowering writes it: `Point` has its own `copy`, so its copy calls the method, and
/// the `i64` copy stays for the backend.
#[test]
fn a_struct_copy_calls_its_own_method() {
    let database = Database::default();
    let (file, name) = named(&database, "Point");
    let point = Type::new(
        &database,
        TypeKind::Struct(Nominal {
            file,
            name,
            arguments: Vec::new(),
        }),
    );
    let int = Type::new(
        &database,
        TypeKind::Primitive(Primitive::Integer(Integer::I64)),
    );
    let none = Type::new(&database, TypeKind::None);
    let package = Package::new(&database, vec![file]);
    let method = Item::new(&database, package, file, name);

    let mut body = Body::new(none, span());
    let parameter = |name: &str| LocalKind::Parameter {
        name: name.into(),
        mode: Mode::Borrow,
    };
    let p = body.push_local(point, parameter("p"), span());
    let n = body.push_local(int, parameter("n"), span());
    let a = body.push_local(point, LocalKind::Temporary, span());
    let b = body.push_local(int, LocalKind::Temporary, span());
    let copy = |from: mir::Local, into: mir::Local, target: usize| Terminator {
        kind: TerminatorKind::Call {
            callee: Callee::Builtin(Builtin::Copy),
            arguments: vec![Argument::Borrow(from.into())],
            destination: into.into(),
            target: BlockId::new(target),
            error: None,
        },
        span: span(),
    };
    body.push_block(copy(p, a, 1));
    body.push_block(copy(n, b, 2));
    body.push_block(Terminator {
        kind: TerminatorKind::Return,
        span: span(),
    });

    own_copies(&mut body, &|ty| (ty == point).then(|| (method, Vec::new())));

    let callees: Vec<&Callee<'_>> = body
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator.kind {
            TerminatorKind::Call { callee, .. } => Some(callee),
            _ => None,
        })
        .collect();
    assert_eq!(
        callees,
        [
            &Callee::Function {
                item: method,
                type_arguments: Vec::new(),
            },
            &Callee::Builtin(Builtin::Copy),
        ]
    );
}
