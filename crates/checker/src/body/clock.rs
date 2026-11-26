//! The standard `time` module's clocks (roadmap 1.13.19, card 61): `time.perf_counter()`,
//! `time.monotonic()` and `time.time()`, each an `f64` of seconds with Python's meaning.
//!
//! The compiler builds them in, as it builds in `sys.exit` and `os.getenv`, so they are checked
//! here rather than against declared signatures. Any other member of `time`, such as
//! `time.sleep`, is reported as not in Baryon yet.

use super::Body;
use super::calls::Arguments;
use syntax::NodeId;
use types::{Float, Primitive, Type, TypeKind};

/// Whether `member` of `time` is one of the clocks the compiler builds in.
pub(super) fn is_clock(member: &str) -> bool {
    matches!(member, "perf_counter" | "monotonic" | "time")
}

impl<'db> Body<'db> {
    /// `time.member()`, where `member` is one of the clocks [`is_clock`] names: an `f64` of
    /// seconds. A clock takes no arguments, as in Python.
    pub(super) fn clock_call(
        &mut self,
        node: NodeId,
        member: &str,
        arguments: &Arguments,
    ) -> Type<'db> {
        if !arguments.positional.is_empty() || !arguments.keywords.is_empty() {
            self.untyped(arguments);
            return self.lowerer.error(
                node,
                &format!("`time.{member}` takes no arguments"),
                "wrong arguments",
                Some(&format!("call it as `time.{member}()`")),
            );
        }
        Type::new(self.db, TypeKind::Primitive(Primitive::Float(Float::F64)))
    }
}
