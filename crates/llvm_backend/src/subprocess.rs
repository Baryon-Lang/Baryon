//! Running another program (decision 0036, part 8): `subprocess.run` is one call of the runtime.
//!
//! The call can fail, and the runtime raises the way a compiled function does (the `call`
//! module): its last parameter is the address of the error edge's local, a
//! `Raises[None, OSError]`, and it returns an `i1` that is true when it failed. The result, a
//! `CompletedProcess`, goes through an `out` pointer, the destination's address, which the
//! runtime writes only on success.
//!
//! | Built-in | Runtime call |
//! | --- | --- |
//! | [`Run`](mir::Builtin::Run) | `baryon_rt_subprocess_run(args, capture, given, input, length, cwd, length, env, timeout, location, out, error)` |
//!
//! `given` is a constant that says which of `input`, `cwd`, `env` and `timeout` the call wrote,
//! one bit each: 1, 2, 4 and 8. One left out arrives as the constant `None` and is passed as a
//! null pointer or zero, which the runtime does not read.

use crate::Unsupported;
use crate::function::{Function, block_label};
use mir::{Argument, BlockId, ConstantValue, ErrorEdge, Operand, Place};

/// The runtime function's declaration.
const DECLARATION: &str = "declare zeroext i1 @baryon_rt_subprocess_run(ptr, i1 zeroext, \
     i8 zeroext, ptr, i64, ptr, i64, ptr, double, ptr, ptr, ptr) nounwind";

/// The bits of `given` for `input`, `cwd`, `env` and `timeout`, in the arguments' order.
const GIVEN: [u8; 4] = [1, 2, 4, 8];

impl<'db> Function<'_, '_, 'db> {
    /// Writes a call of `subprocess.run`, storing the `CompletedProcess` at `destination` and
    /// going to `target`, or to the error edge's block with the `OSError` in its local.
    pub(crate) fn subprocess_run(
        &mut self,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
        error: Option<&ErrorEdge>,
    ) -> Result<(), Unsupported> {
        let edge = error.ok_or_else(|| Unsupported::new("`subprocess.run` with no error edge"))?;
        let [
            Argument::Borrow(args),
            Argument::Owned(capture),
            input,
            cwd,
            env,
            timeout,
        ] = arguments
        else {
            return Err(Unsupported::new("`subprocess.run` with these arguments"));
        };
        let (args, _) = self.place(args)?;
        let (capture, _) = self.value(capture)?;
        let options = [input, cwd, env, timeout];
        let given: u8 = (GIVEN.iter().zip(options))
            .filter(|(_, option)| !left_out(option))
            .map(|(bit, _)| bit)
            .sum();
        let mut texts = Vec::new();
        for text in [input, cwd] {
            texts.push(if left_out(text) {
                ("null".to_owned(), "0".to_owned())
            } else {
                self.text_argument(text)?
            });
        }
        let env = match env {
            Argument::Borrow(place) => {
                let (address, _) = self.place(place)?;
                self.emit(&format!("load ptr, ptr {address}"))
            }
            _ if left_out(env) => "null".to_owned(),
            _ => return Err(Unsupported::new("`subprocess.run` with this `env`")),
        };
        let timeout = match timeout {
            _ if left_out(timeout) => "0.0".to_owned(),
            Argument::Owned(seconds) => self.value(seconds)?.0,
            _ => return Err(Unsupported::new("`subprocess.run` with this `timeout`")),
        };
        let location = self.constants.location(self.span);
        let (out, _) = self.place(destination)?;
        let (raised, _) = self.place(&edge.error.into())?;
        let [(input, input_length), (cwd, cwd_length)] = &texts[..] else {
            return Err(Unsupported::new("`subprocess.run` with these arguments"));
        };
        self.declare("baryon_rt_subprocess_run", DECLARATION);
        let failed = self.emit(&format!(
            "call zeroext i1 @baryon_rt_subprocess_run(ptr {args}, i1 zeroext {capture}, \
             i8 zeroext {given}, ptr {input}, i64 {input_length}, ptr {cwd}, i64 {cwd_length}, \
             ptr {env}, double {timeout}, ptr {location}, ptr {out}, ptr {raised})"
        ));
        self.line(&format!(
            "br i1 {failed}, label %{}, label %{}",
            block_label(edge.target),
            block_label(target)
        ));
        Ok(())
    }
}

/// Whether `argument` is the constant `None`: a keyword the call left out.
fn left_out(argument: &Argument<'_>) -> bool {
    matches!(
        argument,
        Argument::Owned(Operand::Constant(constant)) if constant.value == ConstantValue::None
    )
}
