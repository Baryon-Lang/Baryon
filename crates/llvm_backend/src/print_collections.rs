//! Printing a `list` or a `dict` as Python's `print` writes it (roadmap 1.13.11): `[1, 2, 3]`,
//! `['a', 'b']`, `{'a': 1}`, and `[]` or `{}` when empty. Each element, key and value prints as
//! Python's `repr` writes it, so a `str` inside is quoted and a nested list prints the same way.
//!
//! Unlike a tuple or a struct, a list or a dict has a length known only at run time, so its
//! printing is a loop the compiled code runs, not calls laid out at compile time. A list loop
//! counts from 0 to the length; a dict loop asks the runtime for each next entry with
//! `baryon_rt_dict_next`, which visits them in insertion order, as Python does. Each pass prints
//! `, ` first unless it is the first pass. The code grows with the element type, not the
//! length: one loop body, however many elements the list holds at run time.
//!
//! Cost: one runtime call per element and per separator, plus one per key for a dict's lookup
//! of its next entry.

use crate::Unsupported;
use crate::function::Function;
use crate::list::HEADER;
use crate::print::Form;
use types::Type;

impl<'db> Function<'_, '_, 'db> {
    /// Prints the list header `value`, whose elements are of type `element`, as
    /// `[1, 2, 3]`. The loop enters from a block of its own and steps in one, so the counter's
    /// `phi` names its two predecessors whatever blocks printing an element adds in between.
    pub(crate) fn print_list(
        &mut self,
        stream: u8,
        value: &str,
        element: Type<'db>,
    ) -> Result<(), Unsupported> {
        let llvm = self.types.value(element)?;
        let data = self.emit(&format!("extractvalue {HEADER} {value}, 0"));
        let length = self.emit(&format!("extractvalue {HEADER} {value}, 1"));
        self.print_text(stream, b"[");
        let [enter, test, body, step, done] = [(); 5].map(|()| self.fresh_block());
        let (index, next) = (self.fresh(), self.fresh());
        self.line(&format!("br label %{enter}"));
        self.line_label(&enter);
        self.line(&format!("br label %{test}"));
        self.line_label(&test);
        self.line(&format!(
            "{index} = phi i64 [ 0, %{enter} ], [ {next}, %{step} ]"
        ));
        let more = self.emit(&format!("icmp slt i64 {index}, {length}"));
        self.line(&format!("br i1 {more}, label %{body}, label %{done}"));
        self.line_label(&body);
        self.print_separator(stream, &index);
        let at = self.emit(&format!(
            "getelementptr inbounds {llvm}, ptr {data}, i64 {index}"
        ));
        self.print_held(stream, &at, element)?;
        self.line(&format!("br label %{step}"));
        self.line_label(&step);
        self.line(&format!("{next} = add i64 {index}, 1"));
        self.line(&format!("br label %{test}"));
        self.line_label(&done);
        self.print_text(stream, b"]");
        Ok(())
    }

    /// Prints the `dict` `table`, of type `ty`, as `{'a': 1, 'b': 2}`, its entries in insertion
    /// order. An empty `dict`'s null table needs no check: the runtime finds no entry in it.
    pub(crate) fn print_dict(
        &mut self,
        stream: u8,
        table: &str,
        ty: Type<'db>,
    ) -> Result<(), Unsupported> {
        let shape = self.dict_shape(ty)?;
        let pair = self.types.value(shape.pair)?;
        self.declare(
            "baryon_rt_dict_next",
            "declare i64 @baryon_rt_dict_next(ptr, ptr, i64) nounwind readonly",
        );
        self.print_text(stream, b"{");
        let [enter, test, body, step, done] = [(); 5].map(|()| self.fresh_block());
        let (from, position, after) = (self.fresh(), self.fresh(), self.fresh());
        self.line(&format!("br label %{enter}"));
        self.line_label(&enter);
        self.line(&format!("br label %{test}"));
        self.line_label(&test);
        self.line(&format!(
            "{from} = phi i64 [ 0, %{enter} ], [ {after}, %{step} ]"
        ));
        self.line(&format!(
            "{position} = call i64 @baryon_rt_dict_next(ptr {table}, ptr {}, i64 {from})",
            shape.layout
        ));
        let more = self.emit(&format!("icmp sge i64 {position}, 0"));
        self.line(&format!("br i1 {more}, label %{body}, label %{done}"));
        self.line_label(&body);
        self.print_separator(stream, &from);
        let entries = self.emit(&format!("load ptr, ptr {table}"));
        let offset = self.emit(&format!("mul i64 {position}, {}", shape.entry_size));
        let offset = self.emit(&format!("add i64 {offset}, 8"));
        let entry = self.emit(&format!(
            "getelementptr inbounds i8, ptr {entries}, i64 {offset}"
        ));
        for (field, part) in [(0, shape.key), (1, shape.value)] {
            if field == 1 {
                self.print_text(stream, b": ");
            }
            let at = self.emit(&format!(
                "getelementptr inbounds {pair}, ptr {entry}, i32 0, i32 {field}"
            ));
            self.print_held(stream, &at, part)?;
        }
        self.line(&format!("br label %{step}"));
        self.line_label(&step);
        self.line(&format!("{after} = add i64 {position}, 1"));
        self.line(&format!("br label %{test}"));
        self.line_label(&done);
        self.print_text(stream, b"}");
        Ok(())
    }

    /// Prints `, ` unless `counter`, which starts at 0 and grows on each pass, is 0: so every
    /// element but the first follows a separator.
    fn print_separator(&mut self, stream: u8, counter: &str) {
        let [separate, joined] = [(); 2].map(|()| self.fresh_block());
        let later = self.emit(&format!("icmp ne i64 {counter}, 0"));
        self.line(&format!(
            "br i1 {later}, label %{separate}, label %{joined}"
        ));
        self.line_label(&separate);
        self.print_text(stream, b", ");
        self.line(&format!("br label %{joined}"));
        self.line_label(&joined);
    }

    /// Prints the value of type `ty` at `address` as Python's `repr` writes it, or `None` for a
    /// type with no value to load.
    fn print_held(&mut self, stream: u8, address: &str, ty: Type<'db>) -> Result<(), Unsupported> {
        if let Some(value) = self.load(address, ty)? {
            return self.print_value(stream, &value, ty, Form::Repr);
        }
        self.print_text(stream, b"None");
        Ok(())
    }
}
