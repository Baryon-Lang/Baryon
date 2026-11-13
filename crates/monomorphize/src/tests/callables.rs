//! Tests of callable parameters (decision 0026): an instance's `Callable` parameter becomes a
//! direct call of the function passed for it, including through copies and further calls.

use super::{Database, named, span};
use crate::instantiate;
use database::Package;
use mir::{
    BinaryOperator, BlockId, Body, Callee, Local, LocalKind, Operand, Rvalue, Statement,
    StatementKind, Terminator, TerminatorKind,
};
use types::{Float, Item, Mode, Primitive, Type, TypeKind};

/// `def step[S: VectorSpace](f: Callable[[S], S], s: S, h: f64) -> S: return h * f(s)`,
/// instantiated with `S` a `Fixed` vector and `f` the function `lorenz` (decision 0026): the
/// call of `f` becomes a direct call of `lorenz`, and `h * …`, scalar arithmetic on `S` in the
/// generic body, becomes a whole-array expression for array expansion.
#[test]
fn callable_parameters_become_direct_calls_and_vector_arithmetic_whole_array() {
    let database = Database::default();
    let (file, name) = named(&database, "S");
    let s = Type::new(&database, TypeKind::Parameter { file, name });
    let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    let vector = Type::new(
        &database,
        TypeKind::Fixed {
            element: f64,
            shape: types::Shape::new(vec![3]).unwrap(),
        },
    );
    let signature = |argument| {
        Type::new(
            &database,
            TypeKind::Callable {
                parameters: vec![argument],
                returns: argument,
            },
        )
    };
    let (generic, concrete) = (signature(s), signature(vector));
    let (lorenz_file, lorenz_name) = named(&database, "lorenz");
    let package = Package::new(&database, vec![lorenz_file]);
    let lorenz = Item::new(&database, package, lorenz_file, lorenz_name);
    let function = Type::new(
        &database,
        TypeKind::Function {
            item: lorenz,
            callable: concrete,
        },
    );

    let mut body = Body::new(s, span());
    let parameter = |name: &str| LocalKind::Parameter {
        name: name.into(),
        mode: Mode::Borrow,
    };
    let f = body.push_local(generic, parameter("f"), span());
    let state = body.push_local(s, parameter("s"), span());
    let h = body.push_local(f64, parameter("h"), span());
    let derivative = body.push_local(s, LocalKind::Temporary, span());
    body.push_block(Terminator {
        kind: TerminatorKind::Call {
            callee: Callee::Value(Operand::Copy(f.into())),
            arguments: vec![mir::Argument::Borrow(state.into())],
            destination: derivative.into(),
            target: BlockId::new(1),
            error: None,
        },
        span: span(),
    });
    let scale = body.push_block(Terminator {
        kind: TerminatorKind::Return,
        span: span(),
    });
    body.blocks[scale.index()].statements = vec![Statement {
        kind: StatementKind::Assign(
            Local::RETURN.into(),
            Rvalue::Binary(
                BinaryOperator::Multiply,
                Operand::Copy(h.into()),
                Operand::Copy(derivative.into()),
            ),
        ),
        span: span(),
    }];

    let instance = instantiate(&database, &body, &[s, generic], &[vector, function]);
    assert!(matches!(
        instance.locals[f.index()].ty.kind(&database),
        TypeKind::Function { item, .. } if *item == lorenz
    ));
    // `f`'s type is now `lorenz`'s own, which shows as its signature.
    assert_eq!(
        instance.display(&database).to_string(),
        "\
fn(_1: Callable[[Fixed[f64, (3,)]], Fixed[f64, (3,)]], _2: Fixed[f64, (3,)], _3: f64) -> Fixed[f64, (3,)] {
    let _0: Fixed[f64, (3,)]; // return
    let _1: Callable[[Fixed[f64, (3,)]], Fixed[f64, (3,)]]; // parameter f
    let _2: Fixed[f64, (3,)]; // parameter s
    let _3: f64; // parameter h
    let _4: Fixed[f64, (3,)];

    bb0: {
        _4 = call lorenz(_2) -> [return: bb1];
    }

    bb1: {
        _0 = elementwise Multiply(copy _3, copy _4);
        return;
    }
}
"
    );
}

/// `def both(f: Callable[[f64], f64], g: Callable[[f64], f64])` with `h = g`, `apply(g)` and
/// `h()`, instantiated with `f` the function `square` and `g` the function `cube`. The two
/// parameters share one `Callable` type, so substitution alone cannot tell them apart: the copy
/// `h` takes `cube`'s type from `g`, its call becomes a direct call of `cube`, and
/// `forward_callables` gives `apply` the type argument `cube`, not the shared `Callable`.
#[test]
fn callables_passed_on_and_copied_keep_their_function() {
    let database = Database::default();
    let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    let callable = Type::new(
        &database,
        TypeKind::Callable {
            parameters: vec![f64],
            returns: f64,
        },
    );
    let function = |name: &str| {
        let (file, node) = named(&database, name);
        let package = Package::new(&database, vec![file]);
        let item = Item::new(&database, package, file, node);
        let ty = Type::new(&database, TypeKind::Function { item, callable });
        (item, ty)
    };
    let ((_, square), (cube_item, cube), (apply, _)) =
        (function("square"), function("cube"), function("apply"));

    let mut body = Body::new(f64, span());
    let parameter = |name: &str| LocalKind::Parameter {
        name: name.into(),
        mode: Mode::Borrow,
    };
    let f = body.push_local(callable, parameter("f"), span());
    let g = body.push_local(callable, parameter("g"), span());
    let h = body.push_local(callable, LocalKind::Temporary, span());
    let entry = body.push_block(Terminator {
        kind: TerminatorKind::Call {
            callee: Callee::Function {
                item: apply,
                type_arguments: vec![callable],
            },
            arguments: vec![mir::Argument::Borrow(g.into())],
            destination: Local::RETURN.into(),
            target: BlockId::new(1),
            error: None,
        },
        span: span(),
    });
    body.blocks[entry.index()].statements = vec![Statement {
        kind: StatementKind::Assign(h.into(), Rvalue::Use(Operand::Copy(g.into()))),
        span: span(),
    }];
    body.push_block(Terminator {
        kind: TerminatorKind::Call {
            callee: Callee::Value(Operand::Copy(h.into())),
            arguments: Vec::new(),
            destination: Local::RETURN.into(),
            target: BlockId::new(2),
            error: None,
        },
        span: span(),
    });
    body.push_block(Terminator {
        kind: TerminatorKind::Return,
        span: span(),
    });

    let mut instance = instantiate(&database, &body, &[callable, callable], &[square, cube]);
    crate::forward_callables(&database, &mut instance, &|item| {
        if item == apply { vec![0] } else { Vec::new() }
    });
    assert_eq!(instance.locals[f.index()].ty, square);
    assert_eq!(instance.locals[h.index()].ty, cube);
    let TerminatorKind::Call {
        callee: Callee::Function { type_arguments, .. },
        ..
    } = &instance.blocks[0].terminator.kind
    else {
        panic!("the first block calls `apply`");
    };
    assert_eq!(type_arguments, &[cube]);
    assert!(matches!(
        &instance.blocks[1].terminator.kind,
        TerminatorKind::Call { callee: Callee::Function { item, .. }, .. } if *item == cube_item
    ));
}
