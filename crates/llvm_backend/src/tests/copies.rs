//! `Builtin::Copy`, `x.copy()` on a type parameter (roadmap 1.9.60): each type the backend
//! copies, and one it reports.

use super::{Database, build, end, parameter, primitive, run, span};
use crate::{Module, Unsupported};
use mir::{
    Argument, BlockId, Body, Builtin, Callee, Local, Operand, Place, Projection, Rvalue,
    TerminatorKind,
};
use types::{Float, Integer, Mode, Primitive, Type, TypeKind};

/// `def copy(x: T) -> T: return x.copy()`, written as lowering writes it in an instance.
fn copied(ty: Type<'_>) -> Body<'_> {
    let mut body = Body::new(ty, span());
    let x = body.push_local(ty, parameter("x", Mode::Borrow), span());
    body.push_block(end(TerminatorKind::Call {
        callee: Callee::Builtin(Builtin::Copy),
        arguments: vec![Argument::Borrow(x.into())],
        destination: Local::RETURN.into(),
        target: BlockId::new(1),
        error: None,
    }));
    body.push_block(end(TerminatorKind::Return));
    body
}

/// The module text of `copy` at `ty`, or what the backend reports.
fn module(database: &Database, ty: Type<'_>) -> Result<String, Unsupported> {
    let symbol = super::symbol_of(database);
    let sources = super::sources();
    let mut module = Module::new(
        database,
        &sources,
        &super::no_structs,
        &super::no_names,
        &symbol,
    );
    module.define("copy", &copied(ty), &[])?;
    Ok(module.finish())
}

/// A copy of a transposed view, an `Array[f64, 2]` whose strides are `[1, 3]`, holds its
/// elements in row-major order with strides `[2, 1]`, in a buffer of its own.
#[test]
fn an_array_view_is_copied_row_major() {
    let database = Database::default();
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let grid = Type::new(
        &database,
        TypeKind::Array {
            element: f64,
            rank: 2,
        },
    );
    let text = module(&database, grid).unwrap();
    let main = r#"
        #include <stdio.h>
        #include <stdint.h>
        typedef struct { double *data; int64_t length[2], stride[2]; } Grid;
        void copy(Grid *, const Grid *);
        int main(void) {
            double data[6] = {1, 2, 3, 4, 5, 6};
            Grid view = {data, {3, 2}, {1, 3}};
            Grid copied;
            copy(&copied, &view);
            printf("%lld x %lld, strides %lld %lld, own %d:", (long long)copied.length[0],
                   (long long)copied.length[1], (long long)copied.stride[0],
                   (long long)copied.stride[1], copied.data != data);
            for (int i = 0; i < 6; i++) printf(" %g", copied.data[i]);
            printf("\n");
            return 0;
        }
    "#;
    let executable = build("copy-array", &text, main);
    assert_eq!(
        run(&executable, &[]).unwrap(),
        "3 x 2, strides 2 1, own 1: 1 4 2 5 3 6\n"
    );
}

/// `def third(xs: list[i64], i: i64) -> i64: ys = xs.copy(); xs[i] = 0; return ys[i]`, as
/// lowering writes it in an instance, without the drops and checks.
fn third(database: &Database) -> Body<'_> {
    let int = primitive(database, Primitive::Integer(Integer::I64));
    let list = Type::new(database, TypeKind::List(int));
    let mut body = Body::new(int, span());
    let xs = body.push_local(list, parameter("xs", Mode::InOut), span());
    let i = body.push_local(int, parameter("i", Mode::Borrow), span());
    let ys = body.push_local(list, mir::LocalKind::Temporary, span());
    body.push_block(end(TerminatorKind::Call {
        callee: Callee::Builtin(Builtin::Copy),
        arguments: vec![Argument::Borrow(xs.into())],
        destination: ys.into(),
        target: BlockId::new(1),
        error: None,
    }));
    let finish = body.push_block(end(TerminatorKind::Return));
    body.blocks[finish.index()].statements = vec![
        super::assign(
            Place::from(xs).project(Projection::Index(vec![i])),
            Rvalue::Use(Operand::Constant(mir::Constant::integer(int, 0))),
        ),
        super::assign(
            Local::RETURN,
            Rvalue::Use(Operand::Copy(
                Place::from(ys).project(Projection::Index(vec![i])),
            )),
        ),
    ];
    body
}

/// A copy of a `list[i64]` is a new buffer of its elements, filled with one `llvm.memcpy`, so
/// changing the list leaves the copy alone; a copy of an `i64` is the value.
#[test]
fn a_list_of_numbers_is_copied_in_one_step() {
    let database = Database::default();
    let text = super::module(&database, &[("third", third(&database))]);
    let main = r#"
        #include <stdio.h>
        #include <stdint.h>
        typedef struct { int64_t *data, length, capacity; } List;
        int64_t third(List *, int64_t);
        int main(void) {
            int64_t data[4] = {3, 1, 4, 1};
            List list = {data, 3, 4};
            int64_t copied = third(&list, 2);
            printf("%lld %lld\n", (long long)copied, (long long)data[2]);
            return 0;
        }
    "#;
    let executable = build("copy-list", &text, main);
    assert_eq!(run(&executable, &[]).unwrap(), "4 0\n");
    assert!(text.contains("call void @llvm.memcpy.p0.p0.i64("), "{text}");
    let int = primitive(&database, Primitive::Integer(Integer::I64));
    let number = module(&database, int).unwrap();
    assert!(number.contains("= load i64, ptr %_1\n"), "{number}");
    assert!(!number.contains("call "), "{number}");
}

/// A copy of a `list[str]` copies each `str` through the runtime's string builder.
#[test]
fn a_list_of_text_copies_each_element() {
    let database = Database::default();
    let text_type = Type::new(&database, TypeKind::Str);
    let list = Type::new(&database, TypeKind::List(text_type));
    let text = module(&database, list).unwrap();
    assert!(text.contains("call ptr @baryon_rt_text_new()"), "{text}");
    assert!(text.contains("@baryon_rt_text_finish(ptr "), "{text}");
}

/// A type the backend does not copy, such as a list of lists, is reported by name.
#[test]
fn other_types_are_reported() {
    let database = Database::default();
    let int = primitive(&database, Primitive::Integer(Integer::I64));
    let inner = Type::new(&database, TypeKind::List(int));
    let outer = Type::new(&database, TypeKind::List(inner));
    let reported = module(&database, outer).unwrap_err();
    assert!(
        reported
            .to_string()
            .contains("copying `list[list[i64]]` through a type parameter"),
        "{reported}"
    );
}
