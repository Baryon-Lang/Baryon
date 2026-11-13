//! The steps a text array is built from and read by (`Array[str, 1]`, decision 0045), each a
//! [`Builtin::TextArray`](mir::Builtin::TextArray) call: the bytes a `str` takes
//! ([`TextArrayStep::ByteLength`]), copying them into the array's buffer
//! ([`TextArrayStep::CopyBytes`]) and borrowing one element back out of it
//! ([`TextArrayStep::View`]).
//!
//! A text array keeps every element's bytes end to end in one `Array[u8, 1]`, as Arrow's
//! `large_utf8` does, so none of the three needs the runtime: a length is one word, a copy is
//! one `llvm.memcpy`, and an element is a pointer into the buffer with a length.

use crate::Unsupported;
use crate::function::{Function, block_label};
use crate::text::{STATIC, TEXT};
use mir::{Argument, BlockId, Place, TextArrayStep};

impl<'db> Function<'_, '_, 'db> {
    /// Writes the text array step `step` and goes to `target`.
    pub(crate) fn text_array_step(
        &mut self,
        step: TextArrayStep,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        match step {
            TextArrayStep::ByteLength => self.text_byte_length(arguments, destination, target),
            TextArrayStep::CopyBytes => self.text_copy_bytes(arguments, destination, target),
            TextArrayStep::View => self.text_view(arguments, destination, target),
        }
    }

    /// Writes the number of bytes of a `str` into `destination`, an `i64`, and goes to
    /// `target`. The count is the low 63 bits of the string's length word, so nothing is read
    /// but that word.
    fn text_byte_length(
        &mut self,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let [text] = arguments else {
            return Err(Unsupported::new("bytes of `str` with these arguments"));
        };
        let (_, length) = self.text_argument(text)?;
        let (address, _) = self.place(destination)?;
        self.line(&format!("store i64 {length}, ptr {address}"));
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }

    /// Copies the bytes of a `str` into a buffer of bytes from a position, writes the position
    /// after them into `destination`, an `i64`, and goes to `target`. The buffer's elements lie
    /// one after another from its data pointer, which lowering guarantees, so the copy is one
    /// `llvm.memcpy`; an empty string copies nothing and gives the position back.
    fn text_copy_bytes(
        &mut self,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let [text, Argument::InOut(buffer), Argument::Owned(position)] = arguments else {
            return Err(Unsupported::new(
                "`str` into text array with these arguments",
            ));
        };
        let (bytes, length) = self.text_argument(text)?;
        let at = self.buffer_address(buffer, position)?;
        self.declare(
            "llvm.memcpy.p0.p0.i64",
            "declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)",
        );
        self.line(&format!(
            "call void @llvm.memcpy.p0.p0.i64(ptr {at}, ptr {bytes}, i64 {length}, i1 false)"
        ));
        let (position, ty) = self.value(position)?;
        let position = self.widen(&position, ty)?;
        let next = self.emit(&format!("add i64 {position}, {length}"));
        let (address, _) = self.place(destination)?;
        self.line(&format!("store i64 {next}, ptr {address}"));
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }

    /// Writes into `destination` a `str` that borrows `length` bytes of a buffer from a
    /// position, flagged as not owned (decision 0022) so that dropping it frees nothing, and
    /// goes to `target`.
    fn text_view(
        &mut self,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let [
            Argument::Borrow(buffer),
            Argument::Owned(position),
            Argument::Owned(length),
        ] = arguments
        else {
            return Err(Unsupported::new("text array element with these arguments"));
        };
        let at = self.buffer_address(buffer, position)?;
        let (length, ty) = self.value(length)?;
        let length = self.widen(&length, ty)?;
        let word = self.emit(&format!("or i64 {length}, {}", STATIC.cast_signed()));
        let view = self.emit(&format!("insertvalue {TEXT} poison, ptr {at}, 0"));
        let view = self.emit(&format!("insertvalue {TEXT} {view}, i64 {word}, 1"));
        let (address, _) = self.place(destination)?;
        self.line(&format!("store {TEXT} {view}, ptr {address}"));
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }

    /// The address of the byte at `position` of the buffer of bytes at `buffer`, an
    /// `Array[u8, 1]` whose elements lie one after another from its data pointer.
    fn buffer_address(
        &mut self,
        buffer: &Place,
        position: &mir::Operand<'db>,
    ) -> Result<String, Unsupported> {
        let (header, _) = self.place(buffer)?;
        let data = self.emit(&format!("load ptr, ptr {header}"));
        let (position, ty) = self.value(position)?;
        let position = self.widen(&position, ty)?;
        Ok(self.emit(&format!(
            "getelementptr inbounds i8, ptr {data}, i64 {position}"
        )))
    }
}
