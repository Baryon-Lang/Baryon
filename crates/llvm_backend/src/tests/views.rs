//! Views: the header a `Slice` step builds, elements read through it, its length, and the
//! panic of a range out of bounds.

use super::{Database, assign, build, end, parameter, primitive, run, span};
use mir::{Body, Check, Local, Operand, Place, Projection, Rvalue, SliceIndex, TerminatorKind};
use types::{Float, Integer, Mode, Primitive, Type, TypeKind};

/// The parameters every function takes: `grid: Array[f64, 2]`, then the `i64`s `start`,
/// `length`, `step`, `column` and `k`.
fn parameters(database: &Database) -> Vec<(Type<'_>, Mode)> {
    let f64 = primitive(database, Primitive::Float(Float::F64));
    let i64 = primitive(database, Primitive::Integer(Integer::I64));
    let grid = Type::new(
        database,
        TypeKind::Array {
            element: f64,
            rank: 2,
        },
    );
    vec![
        (grid, Mode::Borrow),
        (i64, Mode::Borrow),
        (i64, Mode::Borrow),
        (i64, Mode::Borrow),
        (i64, Mode::Borrow),
        (i64, Mode::Borrow),
    ]
}

/// A body returning `returns` with [`parameters`], one `bool` local, and one block that
/// first asserts `check` on that local when there is one, then assigns `rvalue` to the return
/// value.
fn body<'db>(
    database: &'db Database,
    returns: Type<'db>,
    rvalue: Rvalue<'db>,
    check: Option<Check<'db>>,
) -> Body<'db> {
    let mut body = Body::new(returns, span());
    for (index, (ty, mode)) in parameters(database).into_iter().enumerate() {
        body.push_local(ty, parameter(&format!("p{index}"), mode), span());
    }
    let flag = body.push_local(
        primitive(database, Primitive::Bool),
        mir::LocalKind::Temporary,
        span(),
    );
    let statements = vec![assign(Local::RETURN, rvalue)];
    if let Some(check) = check {
        let entry = body.push_block(end(TerminatorKind::Assert {
            condition: Operand::Copy(flag.into()),
            check,
            target: mir::BlockId::new(1),
        }));
        body.blocks[entry.index()].statements = vec![assign(
            flag,
            Rvalue::Use(Operand::Constant(mir::Constant {
                ty: primitive(database, Primitive::Bool),
                value: mir::ConstantValue::Bool(false),
            })),
        )];
    }
    let block = body.push_block(end(TerminatorKind::Return));
    body.blocks[block.index()].statements = statements;
    body
}

/// `grid[start for length by step, column]`: rows from `start`, of one column.
fn column() -> Place {
    Place::from(Local::new(1)).project(Projection::Slice(vec![
        SliceIndex::Range {
            start: Local::new(2),
            length: Local::new(3),
            step: Local::new(4),
        },
        SliceIndex::Index(Local::new(5)),
    ]))
}

/// The C program: a 4 by 3 grid holding `10 * i + j`, read through views of its columns.
const MAIN: &str = r#"
    #include <stdio.h>
    #include <stdint.h>
    typedef struct { double *data; int64_t length[2], stride[2]; } Grid;
    double pick(const Grid *, int64_t, int64_t, int64_t, int64_t, int64_t);
    int64_t count(const Grid *, int64_t, int64_t, int64_t, int64_t, int64_t);
    double fail(const Grid *, int64_t, int64_t, int64_t, int64_t, int64_t);
    int main(void) {
        double data[12];
        for (int i = 0; i < 12; i++) data[i] = 10 * (i / 3) + i % 3;
        Grid grid = { data, { 4, 3 }, { 3, 1 } };
        printf("%g %g %g\n", pick(&grid, 1, 2, 2, 2, 1), pick(&grid, 3, 4, -1, 0, 3),
               pick(&grid, 0, 4, 1, 1, 0));
        printf("%lld\n", (long long)count(&grid, 0, 2, 2, 1, 0));
        fail(&grid, 0, 0, 1, 0, 0);
        return 0;
    }
"#;

/// A view's header points at its first element with the base's strides times its steps, so
/// an element read through it is the base's element there, forwards or backwards; its length
/// is the range's; and a failed `SliceInBounds` check calls the runtime's slice panic with the
/// start, stop, dimension and length.
#[test]
fn views_read_through_their_strides() {
    let database = Database::default();
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let element = column().project(Projection::Index(vec![Local::new(6)]));
    let check = Check::SliceInBounds {
        dimension: 0,
        start: Operand::Constant(mir::Constant::integer(i64, 2)),
        stop: Operand::Constant(mir::Constant::integer(i64, 12)),
        length: Operand::Constant(mir::Constant::integer(i64, 10)),
    };
    let functions = [
        (
            "pick",
            body(&database, f64, Rvalue::Use(Operand::Copy(element)), None),
        ),
        (
            "count",
            body(&database, i64, Rvalue::Length(column(), 0), None),
        ),
        (
            "fail",
            body(
                &database,
                f64,
                Rvalue::Use(Operand::Constant(mir::Constant::float(f64, 0.0))),
                Some(check),
            ),
        ),
    ];
    let text = super::module(&database, &functions);
    assert!(
        text.contains("alloca { ptr, [1 x i64], [1 x i64] }"),
        "{text}"
    );
    let executable = build("views", &text, MAIN);
    assert_eq!(
        run(&executable, &[]),
        Err("32 0 1\n2\npanic: slice 2:12 is out of range for dimension 0 of length 10\n  --> heat.byn:1:1\n".to_owned())
    );
}

/// `grid[start for length by step, None, column]`: a column of rows with a new axis between,
/// a view of rank 2 whose second dimension has length 1 and uses up none of the grid's.
fn with_new_axis() -> Place {
    Place::from(Local::new(1)).project(Projection::Slice(vec![
        SliceIndex::Range {
            start: Local::new(2),
            length: Local::new(3),
            step: Local::new(4),
        },
        SliceIndex::NewAxis,
        SliceIndex::Index(Local::new(5)),
    ]))
}

/// The C program for [`a_new_axis_has_length_one_and_takes_no_dimension`]: the same grid.
const NEW_AXIS_MAIN: &str = r#"
    #include <stdio.h>
    #include <stdint.h>
    typedef struct { double *data; int64_t length[2], stride[2]; } Grid;
    double pick(const Grid *, int64_t, int64_t, int64_t, int64_t, int64_t);
    int64_t axis(const Grid *, int64_t, int64_t, int64_t, int64_t, int64_t);
    int main(void) {
        double data[12];
        for (int i = 0; i < 12; i++) data[i] = 10 * (i / 3) + i % 3;
        Grid grid = { data, { 4, 3 }, { 3, 1 } };
        printf("%g %lld\n", pick(&grid, 1, 2, 2, 2, 0), (long long)axis(&grid, 0, 4, 1, 1, 0));
        return 0;
    }
"#;

/// A new axis adds a dimension of length 1 to the view's header and moves its data pointer by
/// nothing, so `view[k, k]` with `k` 0 reads the grid at the range's start, and the view's
/// dimension 1 has length 1.
#[test]
fn a_new_axis_has_length_one_and_takes_no_dimension() {
    let database = Database::default();
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let element = with_new_axis().project(Projection::Index(vec![Local::new(6), Local::new(6)]));
    let functions = [
        (
            "pick",
            body(&database, f64, Rvalue::Use(Operand::Copy(element)), None),
        ),
        (
            "axis",
            body(&database, i64, Rvalue::Length(with_new_axis(), 1), None),
        ),
    ];
    let text = super::module(&database, &functions);
    assert!(
        text.contains("alloca { ptr, [2 x i64], [2 x i64] }"),
        "{text}"
    );
    let executable = build("new-axis", &text, NEW_AXIS_MAIN);
    assert_eq!(run(&executable, &[]), Ok("12 1\n".to_owned()));
}
