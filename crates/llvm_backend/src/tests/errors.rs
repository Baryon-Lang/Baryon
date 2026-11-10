//! Functions that can fail: the error pointer they take, the error they write, and the flag.

use super::{
    Database, Module, end, no_names, no_structs, parameter, primitive, sources, span, symbol_of,
};
use mir::{Body, Operand, TerminatorKind};
use types::{Integer, Mode, Primitive, Type, TypeKind};

/// `def fail(x: i64) -> Raises[None, i64]: raise x`, with `i64` standing for an error type: a
/// function that can fail takes `%error` last, writes the tag and the error through it, and
/// returns `true`, the failure flag.
#[test]
fn raising_writes_the_error_and_returns_the_flag() {
    let database = Database::default();
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let none = Type::new(&database, TypeKind::None);
    let mut body = Body::new(none, span());
    let x = body.push_local(i64, parameter("x", Mode::Borrow), span());
    body.push_block(end(TerminatorKind::Raise(Operand::Copy(x.into()))));
    let symbol = symbol_of(&database);
    let sources = sources();
    let mut module = Module::new(&database, &sources, &no_structs, &no_names, &symbol);
    module.define("fail", &body, &[i64]).unwrap();
    assert_eq!(
        module.finish(),
        "\
define i1 @fail(i64 %a1, ptr noalias %error) {
start:
  %_1 = alloca i64
  store i64 %a1, ptr %_1
  br label %bb0
bb0:
  %t0 = load i64, ptr %_1
  %t1 = getelementptr inbounds { i8, [1 x i64] }, ptr %error, i32 0, i32 0
  store i8 0, ptr %t1
  %t2 = getelementptr inbounds { i8, [1 x i64] }, ptr %error, i32 0, i32 1
  store i64 %t0, ptr %t2
  ret i1 true
}
"
    );
}
