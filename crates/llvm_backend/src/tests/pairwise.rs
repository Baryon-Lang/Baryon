//! The pairwise leaf built-ins (decision 0041): each calls its runtime function with the count,
//! the start or the leaf sums, and stores the result. The C program stands in for the runtime.

use super::{Database, build, end, parameter, primitive, run, span};
use mir::{Argument, Body, Builtin, Callee, Local, LocalKind, Operand, Place, TerminatorKind};
use types::{Float, Integer, Mode, Primitive, Type, TypeKind};

/// A body returning `returns` that takes `parameters`, each borrowed, and returns the result of
/// calling `builtin` with `arguments`.
fn calling<'db>(
    returns: Type<'db>,
    parameters: &[Type<'db>],
    builtin: Builtin,
    arguments: Vec<Argument<'db>>,
) -> Body<'db> {
    let mut body = Body::new(returns, span());
    for (index, ty) in parameters.iter().enumerate() {
        body.push_local(*ty, parameter(&format!("p{index}"), Mode::Borrow), span());
    }
    let result = body.push_local(returns, LocalKind::Temporary, span());
    body.push_block(end(TerminatorKind::Call {
        callee: Callee::Builtin(builtin),
        arguments,
        destination: Place::from(result),
        target: mir::BlockId::new(1),
        error: None,
    }));
    let block = body.push_block(end(TerminatorKind::Return));
    body.blocks[block.index()].statements = vec![super::assign(
        Local::RETURN,
        mir::Rvalue::Use(Operand::Copy(Place::from(result))),
    )];
    body
}

/// The C program: stand-ins for the runtime's three functions that show their arguments.
const MAIN: &str = r#"
    #include <stdio.h>
    #include <stdint.h>
    typedef struct { double *data; int64_t length[1], stride[1]; } Vector;
    int64_t leaves(int64_t);
    int64_t leaf_end(int64_t, int64_t);
    double combine(int64_t, const Vector *);
    int64_t baryon_rt_pairwise_leaves(int64_t count) { return count / 100; }
    int64_t baryon_rt_pairwise_leaf_end(int64_t count, int64_t start) { return count - start; }
    double baryon_rt_pairwise_combine_f64(int64_t count, const double *sums, int64_t n) {
        return count + 10 * sums[0] + 100 * sums[n - 1] + 1000 * n;
    }
    int main(void) {
        double sums[3] = { 1, 2, 3 };
        Vector vector = { sums, { 3 }, { 1 } };
        printf("%lld %lld %g\n", (long long)leaves(300), (long long)leaf_end(300, 72),
               combine(5, &vector));
        return 0;
    }
"#;

/// `PairwiseLeaves` passes the count, `PairwiseLeafEnd` the count and the start, and
/// `PairwiseCombine` the count, the first leaf sum's address and the number of sums.
#[test]
fn pairwise_built_ins_call_the_runtime() {
    let database = Database::default();
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let vector = Type::new(
        &database,
        TypeKind::Array {
            element: f64,
            rank: 1,
        },
    );
    let owned = |index: usize| Argument::Owned(Operand::Copy(Place::from(Local::new(index))));
    let functions = [
        (
            "leaves",
            calling(i64, &[i64], Builtin::PairwiseLeaves, vec![owned(1)]),
        ),
        (
            "leaf_end",
            calling(
                i64,
                &[i64, i64],
                Builtin::PairwiseLeafEnd,
                vec![owned(1), owned(2)],
            ),
        ),
        (
            "combine",
            calling(
                f64,
                &[i64, vector],
                Builtin::PairwiseCombine,
                vec![owned(1), Argument::Borrow(Place::from(Local::new(2)))],
            ),
        ),
    ];
    let text = super::module(&database, &functions);
    let executable = build("pairwise", &text, MAIN);
    assert_eq!(run(&executable, &[]), Ok("3 228 3315\n".to_owned()));
}
