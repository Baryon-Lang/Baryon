//! What a failed check reports: the runtime function it calls, its message and its location.

use super::{
    Database, Module, SOURCE, build, conversion, end, no_structs, parameter, primitive, run,
    sources, span, symbol_of,
};
use mir::{
    BinaryOperator, BlockId, Body, Check, Local, LocalKind, Operand, Rvalue, Statement,
    StatementKind, TerminatorKind,
};
use source::{FileId, Span};
use types::{Float, Integer, Mode, Primitive, Type, TypeKind};

/// `def check(a: i64, b: i64)`: asserts `a <operator> b` with `check`, from the source at
/// `span`.
fn assert_body<'db>(
    database: &'db Database,
    operator: BinaryOperator,
    check: Check<'db>,
    span: Span,
) -> Body<'db> {
    let i64 = primitive(database, Primitive::Integer(Integer::I64));
    let bool = primitive(database, Primitive::Bool);
    let mut body = Body::new(Type::new(database, TypeKind::None), span);
    let a = body.push_local(i64, parameter("a", Mode::Borrow), span);
    let b = body.push_local(i64, parameter("b", Mode::Borrow), span);
    let holds = body.push_local(bool, LocalKind::Temporary, span);
    let mut assert = end(TerminatorKind::Assert {
        condition: Operand::Copy(holds.into()),
        check,
        target: BlockId::new(1),
    });
    assert.span = span;
    let first = body.push_block(assert);
    body.blocks[first.index()].statements = vec![Statement {
        kind: StatementKind::Assign(
            holds.into(),
            Rvalue::Binary(operator, Operand::Copy(a.into()), Operand::Copy(b.into())),
        ),
        span,
    }];
    body.push_block(end(TerminatorKind::Return));
    body
}

/// The C program that calls `check` with its two arguments and prints `ok` when it returns.
const MAIN: &str = r#"
    #include <stdio.h>
    #include <stdint.h>
    #include <stdlib.h>
    void check(int64_t, int64_t);
    int main(int count, char **arguments) {
        (void)count;
        check(strtoll(arguments[1], NULL, 10), strtoll(arguments[2], NULL, 10));
        printf("ok\n");
        return 0;
    }
"#;

/// A failed `assert` prints its condition and message as written, whitespace joined, at the
/// statement's line and column.
#[test]
fn assertions_print_their_source() {
    let database = Database::default();
    let statement = Span::new(
        FileId::from_index(0),
        0,
        u32::try_from(SOURCE.len() - 1).unwrap(),
    );
    let body = assert_body(
        &database,
        BinaryOperator::GreaterEqual,
        Check::Assertion,
        statement,
    );
    let executable = build(
        "assert",
        &super::module(&database, &[("check", body)]),
        MAIN,
    );
    assert_eq!(run(&executable, &["0", "0"]).as_deref(), Ok("ok\n"));
    assert_eq!(
        run(&executable, &["-1", "0"]),
        Err(
            "panic: assertion failed: total >= 0, 'total is negative'\n  --> heat.npl:1:1\n"
                .to_owned()
        )
    );
}

/// A length check passes both lengths and the dimension to the runtime, which names them.
#[test]
fn length_checks_report_both_lengths() {
    let database = Database::default();
    let check = Check::SameLength {
        dimension: 1,
        expected: Operand::Copy(Local::new(1).into()),
        found: Operand::Copy(Local::new(2).into()),
    };
    // The second line, `    'total is negative'`, from its fifth column.
    let second_line = u32::try_from(SOURCE.find('\'').unwrap()).unwrap();
    let at = Span::new(FileId::from_index(0), second_line, second_line + 1);
    let body = assert_body(&database, BinaryOperator::Equal, check, at);
    let executable = build(
        "lengths",
        &super::module(&database, &[("check", body)]),
        MAIN,
    );
    assert_eq!(run(&executable, &["64", "64"]).as_deref(), Ok("ok\n"));
    assert_eq!(
        run(&executable, &["64", "63"]),
        Err(
            "panic: array lengths differ in dimension 1: 64 and 63\n  --> heat.npl:2:5\n"
                .to_owned()
        )
    );
}

/// An index check passes the index, widened to `i64`, its dimension and the length to the
/// runtime, which names them.
#[test]
fn index_checks_report_the_dimension() {
    let database = Database::default();
    let check = Check::IndexInBounds {
        dimension: 2,
        index: Operand::Copy(Local::new(1).into()),
        length: Operand::Copy(Local::new(2).into()),
    };
    let body = assert_body(&database, BinaryOperator::Less, check, span());
    let executable = build("index", &super::module(&database, &[("check", body)]), MAIN);
    assert_eq!(run(&executable, &["7", "8"]).as_deref(), Ok("ok\n"));
    assert_eq!(
        run(&executable, &["8", "8"]),
        Err(
            "panic: index 8 is out of range for dimension 2 of length 8\n  --> heat.npl:1:1\n"
                .to_owned()
        )
    );
}

/// A body the backend rejects after writing some of its checks leaves no constants behind.
#[test]
fn rejected_bodies_leave_no_constants() {
    let database = Database::default();
    let mut body = assert_body(&database, BinaryOperator::Equal, Check::Assertion, span());
    body.blocks[1].statements = vec![Statement {
        kind: StatementKind::Assign(
            Local::new(3).into(),
            Rvalue::IsPresent(Local::new(1).into()),
        ),
        span: span(),
    }];
    let symbol = symbol_of(&database);
    let sources = sources();
    let mut module = Module::new(&database, &sources, &no_structs, &symbol);
    assert_eq!(
        module
            .define("check", &body, &[])
            .map_err(|error| error.to_string()),
        Err("the LLVM backend does not compile optional values yet".to_owned())
    );
    assert_eq!(module.finish(), "");
}

/// A conversion that cannot keep its value stops the program, as the language reference's
/// "Conversions" requires, and one that can rounds toward zero.
#[test]
fn conversions_check_their_range() {
    let database = Database::default();
    let f64 = primitive(&database, Primitive::Float(Float::F64));
    let i8 = primitive(&database, Primitive::Integer(Integer::I8));
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let u8 = primitive(&database, Primitive::Integer(Integer::U8));
    let text = super::module(
        &database,
        &[
            ("float_to_i8", conversion(f64, i8)),
            ("i64_to_u8", conversion(i64, u8)),
        ],
    );
    let main = r#"
        #include <stdio.h>
        #include <stdint.h>
        #include <stdlib.h>
        #include <string.h>
        int8_t float_to_i8(double);
        uint8_t i64_to_u8(int64_t);
        int main(int count, char **arguments) {
            (void)count;
            if (strcmp(arguments[1], "float") == 0) {
                printf("%d\n", float_to_i8(strtod(arguments[2], NULL)));
            } else {
                printf("%u\n", i64_to_u8(strtoll(arguments[2], NULL, 10)));
            }
            return 0;
        }
    "#;
    let executable = build("conversions", &text, main);
    let convert = |kind: &str, value: &str| run(&executable, &[kind, value]);
    assert_eq!(convert("float", "3.9").as_deref(), Ok("3\n"));
    assert_eq!(convert("float", "-128.9").as_deref(), Ok("-128\n"));
    assert_eq!(convert("float", "127.99").as_deref(), Ok("127\n"));
    let lost = |to: &str| {
        Err(format!(
            "panic: the value does not fit in `{to}`\n  --> heat.npl:1:1\n"
        ))
    };
    assert_eq!(convert("float", "128"), lost("i8"));
    assert_eq!(convert("float", "-129"), lost("i8"));
    assert_eq!(convert("float", "nan"), lost("i8"));
    assert_eq!(convert("integer", "255").as_deref(), Ok("255\n"));
    assert_eq!(convert("integer", "256"), lost("u8"));
    assert_eq!(convert("integer", "-1"), lost("u8"));
}
