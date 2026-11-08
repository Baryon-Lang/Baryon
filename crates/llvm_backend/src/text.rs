//! `str` values (decision 0022): a pointer to UTF-8 bytes and a length word, `{ ptr, i64 }`.
//!
//! The low 63 bits of the length word are the byte count. The top bit is set when the value does
//! not own its bytes, as for a literal, whose bytes are a constant in the program: a literal
//! costs nothing at run time, and a drop frees only a string whose bit is clear. Every read of a
//! length goes through [`length`](Function::text_length), which masks the bit off, so no other
//! code needs to know about it.

use crate::function::Function;

/// The LLVM type of a `str` value.
pub(crate) const TEXT: &str = "{ ptr, i64 }";

/// The top bit of the length word, set when the bytes are not owned.
const STATIC: u64 = 1 << 63;

impl Function<'_, '_, '_> {
    /// The `str` value of the literal `text`, as an LLVM constant of type [`TEXT`]: its bytes,
    /// which are never copied, and its length with the "not owned" bit set.
    pub(crate) fn text_constant(&mut self, text: &str) -> String {
        let bytes = self.constants.bytes(text.as_bytes());
        // The length word's bits as LLVM writes an `i64`: signed, since the top bit is set.
        let word = (text.len() as u64 | STATIC).cast_signed();
        format!("{{ ptr {bytes}, i64 {word} }}")
    }

    /// The pointer to the bytes of the `str` value `value`, and their number with the flag
    /// masked off.
    pub(crate) fn text_parts(&mut self, value: &str) -> (String, String) {
        let bytes = self.emit(&format!("extractvalue {TEXT} {value}, 0"));
        let word = self.emit(&format!("extractvalue {TEXT} {value}, 1"));
        (bytes, self.text_length(&word))
    }

    /// The byte count of the length word `word`: its low 63 bits.
    pub(crate) fn text_length(&mut self, word: &str) -> String {
        self.emit(&format!("and i64 {word}, {}", STATIC - 1))
    }

    /// Frees the bytes of the `str` at `address` when it owns them, that is when the top bit of
    /// its length word is clear; a literal's bytes are left alone.
    pub(crate) fn free_text(&mut self, address: &str) {
        let field = self.emit(&format!(
            "getelementptr inbounds {TEXT}, ptr {address}, i32 0, i32 1"
        ));
        let word = self.emit(&format!("load i64, ptr {field}"));
        // The top bit is the sign bit, so an owned string's word is not negative.
        let owned = self.emit(&format!("icmp sge i64 {word}, 0"));
        let (free, done) = (self.fresh_block(), self.fresh_block());
        self.line(&format!("br i1 {owned}, label %{free}, label %{done}"));
        self.line_label(&free);
        let bytes = self.emit(&format!("load ptr, ptr {address}"));
        self.declare(
            "npl_rt_deallocate",
            "declare void @npl_rt_deallocate(ptr, i64, i64) nounwind",
        );
        self.line(&format!(
            "call void @npl_rt_deallocate(ptr {bytes}, i64 {word}, i64 1)"
        ));
        self.line(&format!("br label %{done}"));
        self.line_label(&done);
    }
}
