//! Tests of the backend over bodies built by hand, as lowering builds them. Most compile the
//! module with `clang`, link it with a small C program and run it, so they check what the code
//! computes, not only the text; `clang` is what the driver uses too (decision 0020). The
//! program links [`RUNTIME`], a stand-in for the runtime's functions, so a test sees
//! what a failed check reports without this crate depending on the runtime.

mod arrays;
mod debug;
mod dicts;
mod errors;
mod math;
mod panics;
mod parallel;
mod print;
mod process;
mod text;
mod vector;
mod views;

use crate::{Module, Unsupported};
use database::{Db, File, Package, ParsedFile, Resolution};
use mir::{
    Argument, BinaryOperator, BlockId, Body, Callee, Check, Local, LocalKind, Operand, Place,
    Projection, Rvalue, Statement, StatementKind, Terminator, TerminatorKind,
};
use source::{FileId, SourceFile, SourceMap, Span};
use std::path::PathBuf;
use std::process::Command;
use syntax::{NodeId, Role, SyntaxKind, TreeBuilder};
use types::{Declared, Float, Integer, Item, Mode, Nominal, Primitive, Type, TypeKind};

/// A database whose files each hold one name, standing for a definition a call names.
#[salsa::db]
#[derive(Clone, Default)]
struct Database {
    /// Salsa's inputs and caches.
    storage: salsa::Storage<Self>,
}

#[salsa::db]
impl salsa::Database for Database {}

#[salsa::db]
impl Db for Database {
    fn parse(&self, file: File) -> &ParsedFile {
        one_name(self, file)
    }

    fn resolve(&self, _: Package, _: File) -> &Resolution {
        unreachable!("backend tests never resolve names")
    }
}

/// The tree of a file that is one name: a module holding an expression statement.
#[salsa::tracked(returns(ref), no_eq)]
fn one_name(db: &dyn Db, file: File) -> ParsedFile {
    let end = u32::try_from(file.source(db).text().len()).unwrap();
    let mut builder = TreeBuilder::new(file.id(db));
    builder.open(SyntaxKind::Module, Role::Root, 0, end);
    builder.open(SyntaxKind::ExpressionStatement, Role::Body, 0, end);
    builder.leaf(SyntaxKind::Name, Role::Value, 0, end);
    builder.close();
    builder.close();
    ParsedFile::new(builder.finish(), Vec::new())
}

/// The item a file holding only `name` defines.
fn item<'db>(database: &'db Database, name: &str) -> Item<'db> {
    let file = File::new(
        database,
        FileId::from_index(0),
        SourceFile::new("a.byn", name).unwrap(),
    );
    let tree = database.parse(file).tree();
    let statement = tree.children(tree.root()).next().unwrap();
    let node: NodeId = tree.children(statement).next().unwrap();
    Item::new(database, Package::new(database, Vec::new()), file, node)
}

/// The source file the test spans point into, `heat.byn`, whose first line is an `assert`.
const SOURCE: &str = "assert total >= 0,\n    'total is negative'\n";

/// The sources holding [`SOURCE`] as file 0.
fn sources() -> SourceMap {
    let mut sources = SourceMap::new();
    sources.add("heat.byn", SOURCE).unwrap();
    sources
}

/// The span every test statement carries: the first character of [`SOURCE`].
fn span() -> Span {
    Span::new(FileId::from_index(0), 0, 1)
}

/// The primitive type `primitive`.
fn primitive(database: &Database, primitive: Primitive) -> Type<'_> {
    Type::new(database, TypeKind::Primitive(primitive))
}

/// An assignment statement.
fn assign(place: impl Into<Place>, rvalue: Rvalue<'_>) -> Statement<'_> {
    Statement {
        kind: StatementKind::Assign(place.into(), rvalue),
        span: span(),
    }
}

/// A terminator.
fn end(kind: TerminatorKind<'_>) -> Terminator<'_> {
    Terminator { kind, span: span() }
}

/// A parameter named `name` passed in `mode`.
fn parameter(name: &str, mode: Mode) -> LocalKind {
    LocalKind::Parameter {
        name: name.into(),
        mode,
    }
}

/// `def f(a: T, b: T) -> R: return a <operator> b`, where `R` is `returns`.
fn binary<'db>(ty: Type<'db>, returns: Type<'db>, operator: BinaryOperator) -> Body<'db> {
    let mut body = Body::new(returns, span());
    let a = body.push_local(ty, parameter("a", Mode::Borrow), span());
    let b = body.push_local(ty, parameter("b", Mode::Borrow), span());
    let block = body.push_block(end(TerminatorKind::Return));
    body.blocks[block.index()].statements = vec![assign(
        Local::RETURN,
        Rvalue::Binary(operator, Operand::Copy(a.into()), Operand::Copy(b.into())),
    )];
    body
}

/// `def f(x: From) -> To: return To(x)`.
fn conversion<'db>(from: Type<'db>, to: Type<'db>) -> Body<'db> {
    let mut body = Body::new(to, span());
    let x = body.push_local(from, parameter("x", Mode::Borrow), span());
    let block = body.push_block(end(TerminatorKind::Return));
    body.blocks[block.index()].statements = vec![assign(
        Local::RETURN,
        Rvalue::Convert(Operand::Copy(x.into()), to),
    )];
    body
}

/// What no test struct declares.
fn no_structs<'db>(_: &Nominal<'db>) -> Declared<'db> {
    Declared::Struct(Vec::new())
}

/// What no test struct or enum is named.
fn no_names(_: &Nominal<'_>) -> crate::ClassNames {
    crate::ClassNames::default()
}

/// The symbol of a called item: the name its file holds.
fn symbol_of<'db>(database: &'db Database) -> impl Fn(&Item<'db>, &[Type<'db>]) -> String {
    move |item, _| item.file(database).source(database).text().to_owned()
}

/// A module of the functions `functions`, each a symbol and a body.
fn module<'db>(database: &'db Database, functions: &[(&str, Body<'db>)]) -> String {
    let symbol = symbol_of(database);
    let sources = sources();
    let mut module = Module::new(database, &sources, &no_structs, &no_names, &symbol);
    for (name, body) in functions {
        module.define(name, body, &[]).unwrap();
    }
    module.finish()
}

/// A directory of its own for the test `name`.
fn scratch(name: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("llvm_backend-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

/// The runtime's panic and array memory functions, written in C as the runtime prints them (decision 0021), but
/// to standard output, which a test reads.
const RUNTIME: &str = r#"
    #include <stdint.h>
    #include <stdio.h>
    #include <stdlib.h>
    typedef struct { const char *file; size_t length; uint32_t line, column; } Location;
    static void stop(const Location *at) {
        if (at) printf("  --> %.*s:%u:%u\n", (int)at->length, at->file, at->line, at->column);
        exit(101);
    }
    void baryon_rt_panic(const char *message, size_t length, const Location *at) {
        printf("panic: %.*s\n", (int)length, message);
        stop(at);
    }
    void baryon_rt_panic_length_mismatch(uint32_t dimension, int64_t expected, int64_t found,
                                      const Location *at) {
        printf("panic: array lengths differ in dimension %u: %lld and %lld\n", dimension,
               (long long)expected, (long long)found);
        stop(at);
    }
    void baryon_rt_panic_exponent(int64_t exponent, const Location *at) {
        printf("panic: negative exponent %lld in integer `**`\n", (long long)exponent);
        stop(at);
    }
    void baryon_rt_panic_slice(int64_t start, int64_t end, uint32_t dimension,
                               int64_t length, const Location *at) {
        printf("panic: slice %lld:%lld is out of range for dimension %u of length %lld\n",
               (long long)start, (long long)end, dimension, (long long)length);
        stop(at);
    }
    void baryon_rt_panic_index(int64_t index, uint32_t dimension, int64_t length,
                            const Location *at) {
        printf("panic: index %lld is out of range for dimension %u of length %lld\n",
               (long long)index, dimension, (long long)length);
        stop(at);
    }
    void *baryon_rt_allocate_array(int64_t count, size_t size, size_t align, const Location *at) {
        (void)align;
        (void)at;
        return malloc(count * size);
    }
    void baryon_rt_deallocate(void *pointer, size_t bytes, size_t align) {
        printf("freed %zu bytes aligned to %zu\n", bytes, align);
        free(pointer);
    }
"#;

/// Compiles `module` at `-O2` with the C program `main` and [`RUNTIME`] into an executable, and returns its
/// path. Fails the test, with `clang`'s message, when the module is not valid LLVM IR.
fn build(name: &str, module: &str, main: &str) -> PathBuf {
    let directory = scratch(name);
    std::fs::write(directory.join("module.ll"), module).unwrap();
    std::fs::write(directory.join("main.c"), main).unwrap();
    std::fs::write(directory.join("runtime.c"), RUNTIME).unwrap();
    let executable = directory.join("program");
    let output = Command::new("clang")
        .current_dir(&directory)
        .args([
            "-O2",
            "-Wno-override-module",
            "module.ll",
            "main.c",
            "runtime.c",
            "-lm",
            "-o",
        ])
        .arg(&executable)
        .output()
        .expect("the backend tests need `clang` on the PATH (decision 0020)");
    assert!(
        output.status.success(),
        "clang rejected the module:\n{}\n{module}",
        String::from_utf8_lossy(&output.stderr)
    );
    executable
}

/// Runs `executable` with `arguments`: its output, or what it printed when it panicked, which
/// is exiting with status 101. Fails the test on any other way of stopping.
fn run(executable: &PathBuf, arguments: &[&str]) -> Result<String, String> {
    let output = Command::new(executable).args(arguments).output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    match output.status.code() {
        Some(0) => Ok(stdout),
        Some(101) => Err(stdout),
        status => panic!("the program stopped with {status:?}, printing {stdout:?}"),
    }
}

/// `//`, `%` and the shifts on signed integers give Python's results, including the cases
/// where LLVM's own instructions are undefined: dividing the most negative value by `-1`,
/// and shifting by the width or more.
#[test]
fn integer_operators_follow_python() {
    let database = Database::default();
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let text = module(
        &database,
        &[
            (
                "floor_divide",
                binary(i64, i64, BinaryOperator::FloorDivide),
            ),
            ("modulo", binary(i64, i64, BinaryOperator::Remainder)),
            ("shift_left", binary(i64, i64, BinaryOperator::ShiftLeft)),
            ("shift_right", binary(i64, i64, BinaryOperator::ShiftRight)),
        ],
    );
    let main = r#"
        #include <stdio.h>
        #include <stdint.h>
        int64_t floor_divide(int64_t, int64_t);
        int64_t modulo(int64_t, int64_t);
        int64_t shift_left(int64_t, int64_t);
        int64_t shift_right(int64_t, int64_t);
        int main(int count, char **arguments) {
            (void)arguments;
            /* Read through `count` so the optimizer cannot fold the calls. */
            int64_t one = count, pairs[][2] = {{7, 2}, {-7, 2}, {7, -2}, {-7, -2}, {INT64_MIN, -1}};
            for (int i = 0; i < 5; i++) {
                printf("%lld %lld\n", (long long)floor_divide(pairs[i][0] * one, pairs[i][1]),
                       (long long)modulo(pairs[i][0] * one, pairs[i][1]));
            }
            printf("%lld %lld %lld %lld\n", (long long)shift_left(one, 3),
                   (long long)shift_left(one, 64), (long long)shift_right(-8 * one, 1),
                   (long long)shift_right(-8 * one, 70));
            return 0;
        }
    "#;
    let executable = build("integers", &text, main);
    assert_eq!(
        run(&executable, &[]).unwrap(),
        "3 1\n-4 1\n-4 -1\n3 -1\n-9223372036854775808 0\n8 0 -4 -1\n"
    );
}

/// `//` and `%` on floats give Python's results, signs of zero included.
#[test]
fn float_operators_follow_python() {
    let database = Database::default();
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let text = module(
        &database,
        &[
            (
                "floor_divide",
                binary(f64, f64, BinaryOperator::FloorDivide),
            ),
            ("modulo", binary(f64, f64, BinaryOperator::Remainder)),
        ],
    );
    let main = r#"
        #include <stdio.h>
        double floor_divide(double, double);
        double modulo(double, double);
        int main(int count, char **arguments) {
            (void)arguments;
            double one = count, pairs[][2] = {{7.5, 2}, {-7.5, 2}, {7.5, -2}, {-7.5, -2}, {5, -5}};
            for (int i = 0; i < 5; i++) {
                printf("%g %g\n", floor_divide(pairs[i][0] * one, pairs[i][1]),
                       modulo(pairs[i][0] * one, pairs[i][1]));
            }
            return 0;
        }
    "#;
    let executable = build("floats", &text, main);
    assert_eq!(
        run(&executable, &[]).unwrap(),
        "3 1.5\n-4 0.5\n-4 -0.5\n3 -1.5\n-1 -0\n"
    );
}

/// The body of `step`: adds `n` to `total` with an overflow check, then calls `report(total)`.
fn step_body(database: &Database) -> Body<'_> {
    let i64 = primitive(database, Primitive::Integer(Integer::I64));
    let bool = primitive(database, Primitive::Bool);
    let none = Type::new(database, TypeKind::None);
    let pair = Type::new(database, TypeKind::Tuple(vec![i64, bool]));

    let mut body = Body::new(none, span());
    let total = body.push_local(i64, parameter("total", Mode::InOut), span());
    let n = body.push_local(i64, parameter("n", Mode::Borrow), span());
    let sum = body.push_local(pair, LocalKind::Temporary, span());
    let fits = body.push_local(bool, LocalKind::Temporary, span());
    let add = body.push_block(end(TerminatorKind::Assert {
        condition: Operand::Copy(fits.into()),
        check: Check::NoOverflow(BinaryOperator::Add),
        target: BlockId::new(1),
    }));
    body.blocks[add.index()].statements = vec![
        assign(
            sum,
            Rvalue::CheckedBinary(
                BinaryOperator::Add,
                Operand::Copy(total.into()),
                Operand::Copy(n.into()),
            ),
        ),
        assign(
            fits,
            Rvalue::Unary(
                mir::UnaryOperator::Not,
                Operand::Copy(Place::from(sum).project(Projection::Field(1))),
            ),
        ),
    ];
    let store = body.push_block(end(TerminatorKind::Call {
        callee: Callee::Function {
            item: item(database, "report"),
            type_arguments: Vec::new(),
        },
        arguments: vec![Argument::Borrow(total.into())],
        destination: Local::RETURN.into(),
        target: BlockId::new(2),
        error: None,
    }));
    body.blocks[store.index()].statements = vec![assign(
        total,
        Rvalue::Use(Operand::Copy(
            Place::from(sum).project(Projection::Field(0)),
        )),
    )];
    body.push_block(end(TerminatorKind::Return));

    body
}

/// `def step(total: InOut[i64], n: i64)`: adds `n` to `total` with an overflow check, then
/// calls `report(total)`, a function defined elsewhere. Shows the `InOut` pointer, the checked
/// arithmetic into a `tuple[i64, bool]`, the failure block with its message and location
/// constants, and the declarations of the runtime's panic and of the callee, and runs it.
#[test]
fn inout_parameters_checked_arithmetic_and_calls() {
    let database = Database::default();
    let body = step_body(&database);
    let text = module(&database, &[("step", body)]);
    assert_eq!(
        text,
        "\
define void @step(ptr noalias %a1, i64 %a2) {
start:
  %_2 = alloca i64
  %_3 = alloca { i64, i1 }
  %_4 = alloca i1
  store i64 %a2, ptr %_2
  br label %bb0
bb0:
  %t0 = load i64, ptr %a1
  %t1 = load i64, ptr %_2
  %t2 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %t0, i64 %t1)
  store { i64, i1 } %t2, ptr %_3
  %t3 = getelementptr inbounds { i64, i1 }, ptr %_3, i32 0, i32 1
  %t4 = load i1, ptr %t3
  %t5 = xor i1 %t4, true
  store i1 %t5, ptr %_4
  %t6 = load i1, ptr %_4
  br i1 %t6, label %bb1, label %p0
bb1:
  %t7 = getelementptr inbounds { i64, i1 }, ptr %_3, i32 0, i32 0
  %t8 = load i64, ptr %t7
  store i64 %t8, ptr %a1
  %t9 = load i64, ptr %a1
  call void @report(i64 %t9)
  br label %bb2
bb2:
  ret void
p0:
  call void @baryon_rt_panic(ptr @baryon.str.2, i64 23, ptr @baryon.loc.1)
  unreachable
}

@baryon.str.0 = private unnamed_addr constant [8 x i8] c\"heat.byn\"
@baryon.loc.1 = private unnamed_addr constant { ptr, i64, i32, i32 } { ptr @baryon.str.0, i64 8, i32 1, i32 1 }
@baryon.str.2 = private unnamed_addr constant [23 x i8] c\"integer overflow in `+`\"

declare void @baryon_rt_panic(ptr, i64, ptr) cold noreturn nounwind
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
declare void @report(i64)
"
    );
    let main = r#"
        #include <stdio.h>
        #include <stdint.h>
        #include <stdlib.h>
        void step(int64_t *, int64_t);
        void report(int64_t total) { printf("%lld\n", (long long)total); }
        int main(int count, char **arguments) {
            (void)count;
            int64_t total = strtoll(arguments[1], NULL, 10);
            step(&total, 5);
            return total == strtoll(arguments[1], NULL, 10) + 5 ? 0 : 1;
        }
    "#;
    let executable = build("step", &text, main);
    assert_eq!(run(&executable, &["37"]).as_deref(), Ok("42\n"));
    assert_eq!(
        run(&executable, &["9223372036854775807"]),
        Err("panic: integer overflow in `+`\n  --> heat.byn:1:1\n".to_owned())
    );
}

/// A body that uses a type this slice does not translate gets an error naming it, and the
/// module is left as it was.
#[test]
fn unsupported_types_are_named() {
    let database = Database::default();
    let unknown = Type::new(&database, TypeKind::Unknown);
    let symbol = symbol_of(&database);
    let sources = sources();
    let mut module = Module::new(&database, &sources, &no_structs, &no_names, &symbol);
    let result = module.define("make", &Body::new(unknown, span()), &[]);
    assert_eq!(
        result.map_err(|error: Unsupported| error.to_string()),
        Err("the LLVM backend does not compile values of type `unknown` yet".to_owned())
    );
    assert_eq!(module.finish(), "");
}
