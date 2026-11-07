//! Arrays: a new array's header, its lengths, reading and writing elements, and freeing it.

use super::{Database, assign, build, end, parameter, primitive, run, span};
use mir::{Body, Local, Operand, Place, Projection, Rvalue, StatementKind, TerminatorKind};
use types::{Float, Integer, Mode, Primitive, Type, TypeKind};

/// The types the tests use: `i64`, `f64`, `Array[f64, 2]` and `None`.
struct Types<'db> {
    /// `i64`.
    i64: Type<'db>,
    /// `f64`.
    f64: Type<'db>,
    /// `Array[f64, 2]`.
    grid: Type<'db>,
    /// `None`.
    none: Type<'db>,
}

impl<'db> Types<'db> {
    /// The types, interned in `database`.
    fn new(database: &'db Database) -> Self {
        let f64 = primitive(database, Primitive::Float(Float::F64));
        Self {
            i64: primitive(database, Primitive::Integer(Integer::I64)),
            f64,
            grid: Type::new(
                database,
                TypeKind::Array {
                    element: f64,
                    rank: 2,
                },
            ),
            none: Type::new(database, TypeKind::None),
        }
    }
}

/// A body returning `returns` whose parameters are `parameters`, each a type and mode, and
/// whose one block runs `statements` then returns.
fn body<'db>(
    returns: Type<'db>,
    parameters: &[(Type<'db>, Mode)],
    statements: Vec<mir::Statement<'db>>,
) -> Body<'db> {
    let mut body = Body::new(returns, span());
    for (index, &(ty, mode)) in parameters.iter().enumerate() {
        body.push_local(ty, parameter(&format!("p{index}"), mode), span());
    }
    let block = body.push_block(end(TerminatorKind::Return));
    body.blocks[block.index()].statements = statements;
    body
}

/// `grid[i, j]`, where `grid`, `i` and `j` are the first three parameters.
fn element() -> Place {
    Place::from(Local::new(1)).project(Projection::Index(vec![Local::new(2), Local::new(3)]))
}

/// The functions of the test module.
fn functions<'db>(types: &Types<'db>) -> Vec<(&'static str, Body<'db>)> {
    let Types {
        i64,
        f64,
        grid,
        none,
    } = *types;
    let length = |dimension| {
        body(
            i64,
            &[(grid, Mode::Borrow)],
            vec![assign(
                Local::RETURN,
                Rvalue::Length(Local::new(1).into(), dimension),
            )],
        )
    };
    vec![
        // def make(grid: InOut[Array[f64, 2]], n: i64, m: i64): grid = Array[f64, 2](n, m)
        (
            "make",
            body(
                none,
                &[
                    (grid, Mode::InOut),
                    (i64, Mode::Borrow),
                    (i64, Mode::Borrow),
                ],
                vec![assign(
                    Local::new(1),
                    Rvalue::NewArray(
                        grid,
                        vec![
                            Operand::Copy(Local::new(2).into()),
                            Operand::Copy(Local::new(3).into()),
                        ],
                    ),
                )],
            ),
        ),
        ("rows", length(0)),
        ("columns", length(1)),
        // def get(grid: Array[f64, 2], i: i64, j: i64) -> f64: return grid[i, j]
        (
            "get",
            body(
                f64,
                &[
                    (grid, Mode::Borrow),
                    (i64, Mode::Borrow),
                    (i64, Mode::Borrow),
                ],
                vec![assign(Local::RETURN, Rvalue::Use(Operand::Copy(element())))],
            ),
        ),
        // def set(grid: InOut[Array[f64, 2]], i: i64, j: i64, x: f64): grid[i, j] = x
        (
            "set",
            body(
                none,
                &[
                    (grid, Mode::InOut),
                    (i64, Mode::Borrow),
                    (i64, Mode::Borrow),
                    (f64, Mode::Borrow),
                ],
                vec![assign(
                    element(),
                    Rvalue::Use(Operand::Copy(Local::new(4).into())),
                )],
            ),
        ),
        // Drops the array its caller passes, as the end of its owner's scope does.
        (
            "release",
            body(
                none,
                &[(grid, Mode::InOut)],
                vec![mir::Statement {
                    kind: StatementKind::Drop(Local::new(1).into()),
                    span: span(),
                }],
            ),
        ),
    ]
}

/// The C program: makes an `n` by `m` grid, prints its lengths and strides, writes and reads
/// one element, and frees it.
const MAIN: &str = r#"
    #include <stdio.h>
    #include <stdint.h>
    #include <stdlib.h>
    typedef struct { double *data; int64_t length[2], stride[2]; } Grid;
    void make(Grid *, int64_t, int64_t);
    int64_t rows(const Grid *);
    int64_t columns(const Grid *);
    double get(const Grid *, int64_t, int64_t);
    void set(Grid *, int64_t, int64_t, double);
    void release(Grid *);
    int main(int count, char **arguments) {
        (void)count;
        Grid grid;
        make(&grid, strtoll(arguments[1], NULL, 10), strtoll(arguments[2], NULL, 10));
        printf("%lld x %lld, strides %lld %lld\n", (long long)rows(&grid),
               (long long)columns(&grid), (long long)grid.stride[0], (long long)grid.stride[1]);
        set(&grid, 2, 3, 7.5);
        printf("%g %g\n", get(&grid, 2, 3), grid.data[2 * grid.stride[0] + 3]);
        release(&grid);
        return 0;
    }
"#;

/// A new array is row-major, its elements are where its strides say, and dropping it frees
/// the size it was allocated with.
#[test]
fn arrays_are_made_indexed_and_freed() {
    let database = Database::default();
    let types = Types::new(&database);
    let text = super::module(&database, &functions(&types));
    let executable = build("arrays", &text, MAIN);
    assert_eq!(
        run(&executable, &["3", "4"]).as_deref(),
        Ok("3 x 4, strides 4 1\n7.5 7.5\nfreed 96 bytes aligned to 8\n")
    );
}

/// A shape whose element count does not fit in an `i64` panics before allocating.
#[test]
fn shapes_too_large_for_memory_panic() {
    let database = Database::default();
    let types = Types::new(&database);
    let text = super::module(&database, &functions(&types));
    let executable = build("huge", &text, MAIN);
    assert_eq!(
        run(&executable, &["4294967296", "4294967296"]),
        Err(
            "panic: the array's shape has too many elements for memory\n  --> heat.npl:1:1\n"
                .to_owned()
        )
    );
}

/// `grid[i, j]` loads the data pointer and the strides from the header and adds the scaled
/// indices.
#[test]
fn element_addresses_use_the_strides() {
    let database = Database::default();
    let types = Types::new(&database);
    let get = functions(&types).swap_remove(3);
    let text = super::module(&database, &[get]);
    assert_eq!(
        text,
        "\
define double @get(ptr noalias readonly %a1, i64 %a2, i64 %a3) {
start:
  %_0 = alloca double
  %_2 = alloca i64
  %_3 = alloca i64
  store i64 %a2, ptr %_2
  store i64 %a3, ptr %_3
  br label %bb0
bb0:
  %t0 = load ptr, ptr %a1
  %t1 = load i64, ptr %_2
  %t2 = getelementptr inbounds { ptr, [2 x i64], [2 x i64] }, ptr %a1, i32 0, i32 2, i32 0
  %t3 = load i64, ptr %t2
  %t4 = mul i64 %t1, %t3
  %t5 = load i64, ptr %_3
  %t6 = getelementptr inbounds { ptr, [2 x i64], [2 x i64] }, ptr %a1, i32 0, i32 2, i32 1
  %t7 = load i64, ptr %t6
  %t8 = mul i64 %t5, %t7
  %t9 = add i64 %t4, %t8
  %t10 = getelementptr inbounds double, ptr %t0, i64 %t9
  %t11 = load double, ptr %t10
  store double %t11, ptr %_0
  %t12 = load double, ptr %_0
  ret double %t12
}
"
    );
}
