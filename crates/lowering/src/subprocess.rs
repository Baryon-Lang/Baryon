//! Running another program (decision 0036, part 8): `subprocess.run(args, ...)` is one call of
//! [`Builtin::Run`], which can fail with `OSError`.
//!
//! The checker records the call's arguments as the command and then `capture_output`, `input`,
//! `cwd`, `env` and `timeout`, each empty when the call leaves it out. The built-in takes all
//! six: `capture_output` left out is `False`, and each of the other four left out is the
//! constant `None`, which tells a backend not to pass it. The command, `env` and a `str` that
//! is not a literal are borrowed, so the call moves nothing.

use crate::builder::Builder;
use mir::{Argument, Builtin, Constant, ConstantValue, Operand, Place};
use syntax::NodeId;
use types::{Float, Primitive, Type, TypeKind};

impl Builder<'_> {
    /// `subprocess.run(...)` into `place`, a new `CompletedProcess`; `arguments` are the
    /// command and the five keywords, as the checker recorded them.
    pub(crate) fn subprocess_run(
        &mut self,
        place: Place,
        arguments: &[Option<NodeId>],
        node: NodeId,
    ) {
        let &[Some(command), capture, input, cwd, env, timeout] = arguments else {
            return self.unsupported(node, "these arguments of `subprocess.run`");
        };
        let nothing = Type::new(self.db, TypeKind::None);
        let left_out = || {
            Argument::Owned(Operand::Constant(Constant {
                ty: nothing,
                value: ConstantValue::None,
            }))
        };
        let command = Argument::Borrow(self.borrow(command));
        let capture = match capture {
            Some(capture) => self.operand(capture),
            None => Operand::Constant(Constant {
                ty: Type::new(self.db, TypeKind::Primitive(Primitive::Bool)),
                value: ConstantValue::Bool(false),
            }),
        };
        let input = input.map_or_else(left_out, |input| self.text_argument(input));
        let cwd = cwd.map_or_else(left_out, |cwd| self.text_argument(cwd));
        let env = env.map_or_else(left_out, |env| Argument::Borrow(self.borrow(env)));
        let seconds = Type::new(self.db, TypeKind::Primitive(Primitive::Float(Float::F64)));
        let timeout = timeout.map_or_else(left_out, |timeout| {
            Argument::Owned(self.operand_as(timeout, seconds))
        });
        let arguments = vec![command, Argument::Owned(capture), input, cwd, env, timeout];
        self.raising_call(Builtin::Run, arguments, place, node);
    }
}
