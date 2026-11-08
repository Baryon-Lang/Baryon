//! Lists: making one from its elements, appending, reading its length and an element, and
//! freeing one.
//!
//! A `list[T]` value is its header, the layout [`types::layout`] gives it: a pointer to the
//! elements, the number of elements, and the number the buffer has room for, all 64-bit. In
//! LLVM that is `{ ptr, i64, i64 }`. The elements live on the heap, allocated and freed by the
//! runtime (decision 0021) with the capacity's size, so a list of `n` elements in a buffer of
//! room for `c` frees `c` times the element size.
//!
//! An append into a full buffer allocates one twice as large, at least 4 elements, copies the
//! elements across with `llvm.memcpy` and frees the old one, so a run of appends costs
//! amortized constant time per element. The runtime panics when memory runs out.

use crate::Unsupported;
use crate::function::{Function, block_label};
use mir::{Argument, BlockId, Local, Operand};
use types::{Layout, Type, TypeKind};

/// The LLVM type of every list's header.
pub(crate) const HEADER: &str = "{ ptr, i64, i64 }";

impl<'db> Function<'_, '_, 'db> {
    /// A new list of type `ty` holding `elements`, in a buffer with room for exactly them: one
    /// allocation, then a store of each element.
    pub(crate) fn new_list(
        &mut self,
        ty: Type<'db>,
        elements: &[Operand<'db>],
    ) -> Result<String, Unsupported> {
        let (element, layout) = self.list(ty)?;
        let llvm = self.types.value(element)?;
        let count = elements.len();
        let data = self.allocate_elements(&count.to_string(), layout);
        for (position, operand) in elements.iter().enumerate() {
            let (value, _) = self.value(operand)?;
            let at = self.emit(&format!(
                "getelementptr inbounds {llvm}, ptr {data}, i64 {position}"
            ));
            self.line(&format!("store {llvm} {value}, ptr {at}"));
        }
        let value = self.emit(&format!("insertvalue {HEADER} poison, ptr {data}, 0"));
        let value = self.emit(&format!("insertvalue {HEADER} {value}, i64 {count}, 1"));
        Ok(self.emit(&format!("insertvalue {HEADER} {value}, i64 {count}, 2")))
    }

    /// Writes `list.append(value)`, the list `InOut` and the value `Owned`, and goes to `target`.
    pub(crate) fn list_append(
        &mut self,
        arguments: &[Argument<'db>],
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let [Argument::InOut(list), Argument::Owned(value)] = arguments else {
            return Err(Unsupported::new("`list.append` with these arguments"));
        };
        let (header, ty) = self.place(list)?;
        let (element, layout) = self.list(ty)?;
        let llvm = self.types.value(element)?;
        let (value, _) = self.value(value)?;
        let length_at = self.emit(&format!(
            "getelementptr inbounds {HEADER}, ptr {header}, i32 0, i32 1"
        ));
        let capacity_at = self.emit(&format!(
            "getelementptr inbounds {HEADER}, ptr {header}, i32 0, i32 2"
        ));
        let length = self.emit(&format!("load i64, ptr {length_at}"));
        let capacity = self.emit(&format!("load i64, ptr {capacity_at}"));
        let full = self.emit(&format!("icmp eq i64 {length}, {capacity}"));
        let (grow, store) = (self.fresh_block(), self.fresh_block());
        self.line(&format!("br i1 {full}, label %{grow}, label %{store}"));

        self.line_label(&grow);
        let doubled = self.emit(&format!("shl i64 {capacity}, 1"));
        let small = self.emit(&format!("icmp slt i64 {doubled}, 4"));
        let grown = self.emit(&format!("select i1 {small}, i64 4, i64 {doubled}"));
        let buffer = self.allocate_elements(&grown, layout);
        let old = self.emit(&format!("load ptr, ptr {header}"));
        let bytes = self.emit(&format!("mul i64 {length}, {}", layout.size()));
        self.declare(
            "llvm.memcpy.p0.p0.i64",
            "declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)",
        );
        self.line(&format!(
            "call void @llvm.memcpy.p0.p0.i64(ptr {buffer}, ptr {old}, i64 {bytes}, i1 false)"
        ));
        let old_bytes = self.emit(&format!("mul i64 {capacity}, {}", layout.size()));
        self.deallocate(&old, &old_bytes, layout);
        self.line(&format!("store ptr {buffer}, ptr {header}"));
        self.line(&format!("store i64 {grown}, ptr {capacity_at}"));
        self.line(&format!("br label %{store}"));

        self.line_label(&store);
        let data = self.emit(&format!("load ptr, ptr {header}"));
        let at = self.emit(&format!(
            "getelementptr inbounds {llvm}, ptr {data}, i64 {length}"
        ));
        self.line(&format!("store {llvm} {value}, ptr {at}"));
        let longer = self.emit(&format!("add i64 {length}, 1"));
        self.line(&format!("store i64 {longer}, ptr {length_at}"));
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }

    /// The number of elements of the list at `address`, an `i64`.
    pub(crate) fn list_length(&mut self, address: &str) -> String {
        let field = self.emit(&format!(
            "getelementptr inbounds {HEADER}, ptr {address}, i32 0, i32 1"
        ));
        self.emit(&format!("load i64, ptr {field}"))
    }

    /// The address of the element at `index` of the list of type `ty` at `address`, and the
    /// element type.
    pub(crate) fn list_element(
        &mut self,
        address: &str,
        ty: Type<'db>,
        indices: &[Local],
    ) -> Result<(String, Type<'db>), Unsupported> {
        let (element, _) = self.list(ty)?;
        let &[index] = indices else {
            return Err(Unsupported::new(
                "a list index with more than one dimension",
            ));
        };
        let index = self.index(index)?;
        let llvm = self.types.value(element)?;
        let data = self.emit(&format!("load ptr, ptr {address}"));
        let at = self.emit(&format!(
            "getelementptr inbounds {llvm}, ptr {data}, i64 {index}"
        ));
        Ok((at, element))
    }

    /// Frees the buffer of the list of type `ty` at `address`, with the size it was allocated
    /// with: the capacity times the element size. Its elements are copy types and own nothing.
    pub(crate) fn free_list(&mut self, address: &str, ty: Type<'db>) -> Result<(), Unsupported> {
        let (_, layout) = self.list(ty)?;
        let data = self.emit(&format!("load ptr, ptr {address}"));
        let field = self.emit(&format!(
            "getelementptr inbounds {HEADER}, ptr {address}, i32 0, i32 2"
        ));
        let capacity = self.emit(&format!("load i64, ptr {field}"));
        let bytes = self.emit(&format!("mul i64 {capacity}, {}", layout.size()));
        self.deallocate(&data, &bytes, layout);
        Ok(())
    }

    /// A new uninitialized buffer of `count` elements laid out as `layout`. The runtime panics
    /// when the size does not fit in memory or memory runs out.
    fn allocate_elements(&mut self, count: &str, layout: Layout) -> String {
        let location = self.constants.location(self.span);
        self.declare(
            "baryon_rt_allocate_array",
            "declare noalias nonnull ptr @baryon_rt_allocate_array(i64, i64, i64, ptr) nounwind",
        );
        self.emit(&format!(
            "call ptr @baryon_rt_allocate_array(i64 {count}, i64 {}, i64 {}, ptr {location})",
            layout.size(),
            layout.align()
        ))
    }

    /// Frees the `bytes` bytes at `data`, allocated for elements laid out as `layout`.
    pub(crate) fn deallocate(&mut self, data: &str, bytes: &str, layout: Layout) {
        self.declare(
            "baryon_rt_deallocate",
            "declare void @baryon_rt_deallocate(ptr, i64, i64) nounwind",
        );
        self.line(&format!(
            "call void @baryon_rt_deallocate(ptr {data}, i64 {bytes}, i64 {})",
            layout.align()
        ));
    }

    /// The element type of the list type `ty`, and the element's layout.
    fn list(&self, ty: Type<'db>) -> Result<(Type<'db>, Layout), Unsupported> {
        let TypeKind::List(element) = ty.kind(self.types.db) else {
            return Err(self.types.unsupported(ty));
        };
        let layout = types::layout(self.types.db, *element, self.types.declared)
            .map_err(|_| self.types.unsupported(*element))?;
        Ok((*element, layout))
    }
}
