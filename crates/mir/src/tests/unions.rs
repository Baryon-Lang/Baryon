//! Tests of unions (decision 0032): building one from a member, reading which member it holds
//! with [`MemberIndex`](crate::Rvalue::MemberIndex), and reading that member with the
//! [`Member`](crate::Projection::Member) step.

use super::{Database, assign, end, named, primitive, span};
use crate::{BlockId, Body, LocalKind, Operand, Place, Projection, Rvalue, TerminatorKind};
use types::{Float, Integer, Mode, Nominal, Primitive, Type, TypeKind};

/// `match boundary: case Dirichlet(value=v): return v; case _: return 0.0` over a
/// `Dirichlet | Periodic`, after building the union from a `Dirichlet`.
#[test]
fn a_union_is_built_switched_on_and_read() {
    let database = Database::default();
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let class = |name: &str| {
        let (file, name) = named(&database, name);
        Type::new(
            &database,
            TypeKind::Struct(Nominal {
                file,
                name,
                arguments: Vec::new(),
            }),
        )
    };
    let dirichlet = class("Dirichlet");
    let periodic = class("Periodic");
    let boundary = types::union(&database, [periodic, dirichlet]);

    let mut body = Body::new(f64, span());
    let wall = body.push_local(
        dirichlet,
        LocalKind::Parameter {
            name: "wall".into(),
            mode: Mode::Owned,
        },
        span(),
    );
    let union = body.push_local(boundary, LocalKind::Temporary, span());
    let which = body.push_local(i64, LocalKind::Temporary, span());
    let entry = body.push_block(end(TerminatorKind::Switch {
        value: Operand::Copy(which.into()),
        cases: vec![(0, BlockId::new(1))],
        otherwise: BlockId::new(2),
    }));
    body.blocks[entry.index()].statements = vec![
        assign(
            union,
            Rvalue::Aggregate(boundary, vec![Operand::Move(wall.into())]),
        ),
        assign(which, Rvalue::MemberIndex(union.into())),
    ];
    let matched = body.push_block(end(TerminatorKind::Return));
    body.blocks[matched.index()].statements = vec![assign(
        crate::Local::RETURN,
        Rvalue::Use(Operand::Copy(
            Place::from(union)
                .project(Projection::Member(0))
                .project(Projection::Field(0)),
        )),
    )];
    let otherwise = body.push_block(end(TerminatorKind::Return));
    body.blocks[otherwise.index()].statements = vec![assign(
        crate::Local::RETURN,
        Rvalue::Use(Operand::Constant(crate::Constant::float(f64, 0.0))),
    )];

    assert_eq!(body.validate(&database), Vec::new());
    assert_eq!(
        body.display(&database).to_string(),
        "\
fn(_1: Owned[Dirichlet]) -> f64 {
    let _0: f64; // return
    let _1: Dirichlet; // parameter wall
    let _2: Dirichlet | Periodic;
    let _3: i64;

    bb0: {
        _2 = Dirichlet | Periodic(move _1);
        _3 = member_index(_2);
        switch copy _3 -> [0: bb1, otherwise: bb2];
    }

    bb1: {
        _0 = copy _2.member[0].0;
        return;
    }

    bb2: {
        _0 = 0.0_f64;
        return;
    }
}
"
    );
}
