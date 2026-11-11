//! The standard streams (decision 0036, part 6): the `File` that `sys.stdin`, `sys.stdout` or
//! `sys.stderr` gives, `print(..., flush=True)` and `input`, each a call of the runtime.
//!
//! | Built-in | Runtime call |
//! | --- | --- |
//! | [`StandardStream`](mir::Builtin::StandardStream) | `baryon_rt_file_standard(stream)`, a new `File` |
//! | [`FlushOutput`](mir::Builtin::FlushOutput) | `baryon_rt_flush()` |
//! | [`Input`](mir::Builtin::Input) | `baryon_rt_input(prompt, length, out, error)`, which raises `EOFError` |
//!
//! [`PrintError`](mir::Builtin::PrintError) is `print` with the runtime's stream 2, and
//! `print(..., file=f)` is [`FileMethod::Print`](mir::FileMethod::Print), in the `print` and
//! `files` modules.

use crate::Unsupported;
use crate::function::{Function, block_label};
use mir::{Argument, BlockId, ErrorEdge, Place};

impl<'db> Function<'_, '_, 'db> {
    /// Writes `destination = sys.stdin`, `sys.stdout` or `sys.stderr`, a new `File` on the
    /// stream the argument numbers, and goes to `target`.
    pub(crate) fn standard_stream(
        &mut self,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let [Argument::Owned(stream)] = arguments else {
            return Err(Unsupported::new("a standard stream with these arguments"));
        };
        let (stream, _) = self.value(stream)?;
        let stream = self.emit(&format!("trunc i64 {stream} to i8"));
        self.declare(
            "baryon_rt_file_standard",
            "declare ptr @baryon_rt_file_standard(i8 zeroext) nounwind",
        );
        let file = self.emit(&format!(
            "call ptr @baryon_rt_file_standard(i8 zeroext {stream})"
        ));
        let (address, _) = self.place(destination)?;
        self.line(&format!("store ptr {file}, ptr {address}"));
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }

    /// Writes out what standard output holds, for `print(..., flush=True)`, and goes to
    /// `target`.
    pub(crate) fn flush_output(
        &mut self,
        arguments: &[Argument<'db>],
        target: BlockId,
    ) -> Result<(), Unsupported> {
        if !arguments.is_empty() {
            return Err(Unsupported::new("a flush of `print` with arguments"));
        }
        self.declare(
            "baryon_rt_flush",
            "declare void @baryon_rt_flush() nounwind",
        );
        self.line("call void @baryon_rt_flush()");
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }

    /// Writes `destination = input(prompt)` and goes to `target`, or to the error edge's block
    /// with the `EOFError` in its local at the end of the input. The runtime raises as a
    /// compiled function does: it takes the prompt, the destination's address, written only on
    /// success, and the error local's address, and returns an `i1` that is true when it failed.
    pub(crate) fn input(
        &mut self,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
        error: Option<&ErrorEdge>,
    ) -> Result<(), Unsupported> {
        let edge = error.ok_or_else(|| Unsupported::new("`input` with no error edge"))?;
        let [prompt] = arguments else {
            return Err(Unsupported::new("`input` with these arguments"));
        };
        let (bytes, length) = self.text_argument(prompt)?;
        let (out, _) = self.place(destination)?;
        let (raised, _) = self.place(&edge.error.into())?;
        self.declare(
            "baryon_rt_input",
            "declare zeroext i1 @baryon_rt_input(ptr, i64, ptr, ptr) nounwind",
        );
        let failed = self.emit(&format!(
            "call zeroext i1 @baryon_rt_input(ptr {bytes}, i64 {length}, ptr {out}, ptr {raised})"
        ));
        self.line(&format!(
            "br i1 {failed}, label %{}, label %{}",
            block_label(edge.target),
            block_label(target)
        ));
        Ok(())
    }
}
