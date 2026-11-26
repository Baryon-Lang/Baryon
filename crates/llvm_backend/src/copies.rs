//! `x.copy()` where the type of `x` is a type parameter: [`Builtin::Copy`](mir::Builtin::Copy)
//! (roadmap 1.9.60, decision 0066). Lowering builds a generic body once and writes this one
//! call where it cannot choose among the copies each type has; the backend, which sees each
//! instance's own type, writes the copy that type needs. Monomorphization has already turned
//! the call into a direct call of a struct's own `copy` method, so none reaches here.
//!
//! | Type | The copy |
//! | --- | --- |
//! | a copy type: a number, `bool`, enum, `Fixed` array or `Copy` struct | the value itself |
//! | `str` | a new `str` with the same bytes, through the runtime's string builder |
//! | `list` of copy types | a new buffer of exactly its elements, one `llvm.memcpy` |
//! | `list` of `str`s | a new buffer, each element copied as a `str` is |
//! | `Array` of copy types | a new array of the same shape, its elements in row-major order |
//!
//! A value of any other type that owns nothing is copied as it is too; the checker has
//! already refused a type parameter fixed to one with no copy, such as a struct with no `copy`
//! of its own. A type that owns memory and is none of these is reported as not built: the
//! checker refuses a call that fixes a type parameter to one of them where it sees the copy
//! (card 70; building them is roadmap idea I.76).
//!
//! Cost: one allocation and one pass over the elements; a `list` of `str`s allocates once more
//! per element.

use crate::Unsupported;
use crate::function::{Function, block_label};
use crate::list::HEADER;
use mir::{Argument, BlockId, Place};
use types::{Type, TypeKind};

impl<'db> Function<'_, '_, 'db> {
    /// Writes `Builtin::Copy`: the value its one argument borrows, copied into `destination`;
    /// then goes to `target`.
    pub(crate) fn copy_through_parameter(
        &mut self,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let [Argument::Borrow(source)] = arguments else {
            return Err(Unsupported::new("a copy with these arguments"));
        };
        let (address, ty) = self.place(source)?;
        let value = self.copy_value(&address, ty)?;
        let (to, _) = self.place(destination)?;
        let llvm = self.types.value(ty)?;
        self.line(&format!("store {llvm} {value}, ptr {to}"));
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }

    /// A copy of the value of type `ty` at `address`, owning memory of its own.
    fn copy_value(&mut self, address: &str, ty: Type<'db>) -> Result<String, Unsupported> {
        let llvm = self.types.value(ty)?;
        match ty.kind(self.types.db) {
            TypeKind::Str => {
                let value = self.emit(&format!("load {llvm}, ptr {address}"));
                Ok(self.copy_text(&value))
            }
            TypeKind::List(element)
                if matches!(element.kind(self.types.db), TypeKind::Str)
                    || !self.owns_memory(*element) =>
            {
                self.copy_list(address, ty)
            }
            TypeKind::Array { element, .. } if !self.owns_memory(*element) => {
                self.copy_array(address, ty)
            }
            _ if !self.owns_memory(ty) => Ok(self.emit(&format!("load {llvm}, ptr {address}"))),
            _ => Err(Unsupported::new(format!(
                "copying `{}` through a type parameter",
                ty.display(self.types.db)
            ))),
        }
    }

    /// A copy of the list of type `ty` at `address`, in a buffer with room for exactly its
    /// elements: one `llvm.memcpy` of numbers or `bool`s, or a copy of each `str`.
    fn copy_list(&mut self, address: &str, ty: Type<'db>) -> Result<String, Unsupported> {
        let (element, layout) = self.list(ty)?;
        let llvm = self.types.value(element)?;
        let data = self.emit(&format!("load ptr, ptr {address}"));
        let length_at = self.emit(&format!(
            "getelementptr inbounds {HEADER}, ptr {address}, i32 0, i32 1"
        ));
        let length = self.emit(&format!("load i64, ptr {length_at}"));
        let buffer = self.allocate_elements(&length, layout);
        if matches!(element.kind(self.types.db), TypeKind::Str) {
            self.counted_loop(&length, &mut |function, index| {
                let from = function.emit(&format!(
                    "getelementptr inbounds {llvm}, ptr {data}, i64 {index}"
                ));
                let value = function.copy_value(&from, element)?;
                let to = function.emit(&format!(
                    "getelementptr inbounds {llvm}, ptr {buffer}, i64 {index}"
                ));
                function.line(&format!("store {llvm} {value}, ptr {to}"));
                Ok(())
            })?;
        } else {
            let bytes = self.emit(&format!("mul i64 {length}, {}", layout.size()));
            self.declare(
                "llvm.memcpy.p0.p0.i64",
                "declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)",
            );
            self.line(&format!(
                "call void @llvm.memcpy.p0.p0.i64(ptr {buffer}, ptr {data}, i64 {bytes}, i1 false)"
            ));
        }
        let value = self.emit(&format!("insertvalue {HEADER} poison, ptr {buffer}, 0"));
        let value = self.emit(&format!("insertvalue {HEADER} {value}, i64 {length}, 1"));
        Ok(self.emit(&format!("insertvalue {HEADER} {value}, i64 {length}, 2")))
    }

    /// A copy of the array of type `ty` at `address`: a new array of its shape whose elements
    /// lie in row-major order, so a view's copy is contiguous whatever its strides. Element `i`
    /// of the copy is found in the source by splitting `i` into one index per dimension,
    /// innermost first, and adding up each index times its stride.
    fn copy_array(&mut self, address: &str, ty: Type<'db>) -> Result<String, Unsupported> {
        let (element, rank) = self.array(ty)?;
        let header = self.types.value(ty)?;
        let llvm = self.types.value(element)?;
        let layout = types::layout(self.types.db, element, self.types.declared)
            .map_err(|_| self.types.unsupported(element))?;
        let data = self.emit(&format!("load ptr, ptr {address}"));
        let mut lengths = Vec::new();
        let mut strides = Vec::new();
        for dimension in 0..rank {
            for (part, into) in [(1, &mut lengths), (2, &mut strides)] {
                let at = self.emit(&format!(
                    "getelementptr inbounds {header}, ptr {address}, i32 0, i32 {part}, i32 {dimension}"
                ));
                into.push(self.emit(&format!("load i64, ptr {at}")));
            }
        }
        // The product cannot overflow: the source's allocation checked it.
        let mut count = "1".to_owned();
        for length in &lengths {
            count = self.emit(&format!("mul i64 {count}, {length}"));
        }
        let buffer = self.allocate_elements(&count, layout);
        self.counted_loop(&count, &mut |function, index| {
            // Every length is above zero here, or the loop would not run.
            let mut rest = index.to_owned();
            let mut offset = "0".to_owned();
            for (length, stride) in lengths.iter().zip(&strides).rev() {
                let position = function.emit(&format!("urem i64 {rest}, {length}"));
                rest = function.emit(&format!("udiv i64 {rest}, {length}"));
                let step = function.emit(&format!("mul i64 {position}, {stride}"));
                offset = function.emit(&format!("add i64 {offset}, {step}"));
            }
            let from = function.emit(&format!(
                "getelementptr inbounds {llvm}, ptr {data}, i64 {offset}"
            ));
            let value = function.emit(&format!("load {llvm}, ptr {from}"));
            let to = function.emit(&format!(
                "getelementptr inbounds {llvm}, ptr {buffer}, i64 {index}"
            ));
            function.line(&format!("store {llvm} {value}, ptr {to}"));
            Ok(())
        })?;
        let mut value = self.emit(&format!("insertvalue {header} poison, ptr {buffer}, 0"));
        let mut stride = "1".to_owned();
        for dimension in (0..rank).rev() {
            let length = &lengths[dimension as usize];
            value = self.emit(&format!(
                "insertvalue {header} {value}, i64 {length}, 1, {dimension}"
            ));
            value = self.emit(&format!(
                "insertvalue {header} {value}, i64 {stride}, 2, {dimension}"
            ));
            stride = self.emit(&format!("mul i64 {stride}, {length}"));
        }
        Ok(value)
    }

    /// Runs `body` once for each index from 0 up to `count`, an `i64`, passing the index.
    fn counted_loop(
        &mut self,
        count: &str,
        body: &mut dyn FnMut(&mut Self, &str) -> Result<(), Unsupported>,
    ) -> Result<(), Unsupported> {
        let [enter, test, inside, step, done] = [(); 5].map(|()| self.fresh_block());
        let (index, next) = (self.fresh(), self.fresh());
        self.line(&format!("br label %{enter}"));
        self.line_label(&enter);
        self.line(&format!("br label %{test}"));
        self.line_label(&test);
        self.line(&format!(
            "{index} = phi i64 [ 0, %{enter} ], [ {next}, %{step} ]"
        ));
        let more = self.emit(&format!("icmp slt i64 {index}, {count}"));
        self.line(&format!("br i1 {more}, label %{inside}, label %{done}"));
        self.line_label(&inside);
        body(self, &index)?;
        self.line(&format!("br label %{step}"));
        self.line_label(&step);
        self.line(&format!("{next} = add i64 {index}, 1"));
        self.line(&format!("br label %{test}"));
        self.line_label(&done);
        Ok(())
    }
}
