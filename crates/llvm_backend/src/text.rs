//! `str` values (decision 0022): a pointer to UTF-8 bytes and a length word, `{ ptr, i64 }`.
//!
//! The low 63 bits of the length word are the byte count. The top bit is set when the value does
//! not own its bytes, as for a literal, whose bytes are a constant in the program: a literal
//! costs nothing at run time, and a drop frees only a string whose bit is clear. Every read of a
//! length goes through [`length`](Function::text_length), which masks the bit off, so no other
//! code needs to know about it.

use crate::Unsupported;
use crate::function::Function;
use crate::ty::Number;
use mir::{Piece, Place};
use types::{FormatSpec, TypeKind};

/// The LLVM type of a `str` value.
pub(crate) const TEXT: &str = "{ ptr, i64 }";

/// The LLVM type of the runtime's `FormatSpec`: the fill, six one-byte parts, the width and the
/// precision.
pub(crate) const FORMAT_SPEC: &str = "{ i32, i8, i8, i8, i8, i8, i8, i64, i64 }";

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
            "baryon_rt_deallocate",
            "declare void @baryon_rt_deallocate(ptr, i64, i64) nounwind",
        );
        self.line(&format!(
            "call void @baryon_rt_deallocate(ptr {bytes}, i64 {word}, i64 1)"
        ));
        self.line(&format!("br label %{done}"));
        self.line_label(&done);
    }
}

impl<'db> Function<'_, '_, 'db> {
    /// The new, owned `str` that `pieces` build, through the runtime's string builder: one call
    /// per piece, then one that hands the bytes over (the `Format` rvalue).
    pub(crate) fn format(&mut self, pieces: &[Piece]) -> Result<String, Unsupported> {
        self.declare(
            "baryon_rt_text_new",
            "declare ptr @baryon_rt_text_new() nounwind",
        );
        let builder = self.emit("call ptr @baryon_rt_text_new()");
        for piece in pieces {
            match piece {
                Piece::Text(text) => {
                    let bytes = self.constants.bytes(text.as_bytes());
                    self.append(
                        &builder,
                        "str",
                        &format!("ptr {bytes}, i64 {}", text.len()),
                        "null",
                    );
                }
                Piece::Value(place, spec) => {
                    let spec = if *spec == FormatSpec::default() {
                        "null".to_owned()
                    } else {
                        self.constants.format_spec(spec)
                    };
                    self.append_value(&builder, place, &spec)?;
                }
            }
        }
        self.declare(
            "baryon_rt_text_finish",
            &format!("declare {TEXT} @baryon_rt_text_finish(ptr) nounwind"),
        );
        Ok(self.emit(&format!(
            "call {TEXT} @baryon_rt_text_finish(ptr {builder})"
        )))
    }

    /// Appends the value at `place` with the specification at `spec`, through the runtime
    /// function for its type: a narrower integer is widened first, as for `print`.
    fn append_value(
        &mut self,
        builder: &str,
        place: &Place,
        spec: &str,
    ) -> Result<(), Unsupported> {
        let (address, ty) = self.place(place)?;
        let value = self
            .load(&address, ty)?
            .ok_or_else(|| Unsupported::new("`None` in an f-string"))?;
        if matches!(ty.kind(self.types.db), TypeKind::Str) {
            let (bytes, length) = self.text_parts(&value);
            self.append(builder, "str", &format!("ptr {bytes}, i64 {length}"), spec);
            return Ok(());
        }
        let number = match ty.kind(self.types.db) {
            TypeKind::Primitive(_) => self.types.number(ty),
            _ => None,
        };
        let (kind, argument) = match number {
            Some(Number::Bool) => ("bool", format!("i1 zeroext {value}")),
            Some(Number::Float(32)) => ("f32", format!("float {value}")),
            Some(Number::Float(_)) => ("f64", format!("double {value}")),
            Some(Number::Integer { bits, signed }) => {
                let widened = match (bits, signed) {
                    (64, _) => value,
                    (_, true) => self.emit(&format!("sext i{bits} {value} to i64")),
                    (_, false) => self.emit(&format!("zext i{bits} {value} to i64")),
                };
                (if signed { "i64" } else { "u64" }, format!("i64 {widened}"))
            }
            None => {
                return Err(Unsupported::new(format!(
                    "values of type `{}` in an f-string",
                    ty.display(self.types.db)
                )));
            }
        };
        self.append(builder, kind, &argument, spec);
        Ok(())
    }

    /// Calls `baryon_rt_text_{kind}` on `builder` with `value`, its typed argument or arguments,
    /// and the specification `spec`.
    fn append(&mut self, builder: &str, kind: &str, value: &str, spec: &str) {
        let parameters = match kind {
            "str" => "ptr, i64",
            "bool" => "i1 zeroext",
            "f32" => "float",
            "f64" => "double",
            _ => "i64",
        };
        let function = format!("baryon_rt_text_{kind}");
        self.declare(
            &function,
            &format!("declare void @{function}(ptr, {parameters}, ptr) nounwind"),
        );
        self.line(&format!(
            "call void @{function}(ptr {builder}, {value}, ptr {spec})"
        ));
    }
}
