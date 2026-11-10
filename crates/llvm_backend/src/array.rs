//! Arrays: making one, reading a dimension's length, finding an element, and freeing one.
//!
//! An `Array[T, rank]` value is its header, the layout [`types::layout`] gives it: a pointer to
//! the elements, then each dimension's length, then each dimension's stride in elements, all
//! 64-bit. In LLVM that is `{ ptr, [rank x i64], [rank x i64] }`. A new array is row-major
//! (decision 0008): the last dimension's stride is 1. The elements live on the heap, allocated
//! and freed by the runtime (decision 0021).
//!
//! A `Fixed[T, shape]` holds its elements in place, as nested LLVM arrays, and owns no memory.
//!
//! Element `[i, j]` of an `Array` is at `data + i * stride[0] + j * stride[1]`. The step itself is unchecked:
//! lowering puts a bounds check before it (decision 0010).

use crate::Unsupported;
use crate::function::Function;
use crate::panic::Failure;
use mir::{Local, Operand, Place};
use types::{Type, TypeKind};

impl<'db> Function<'_, '_, 'db> {
    /// A new array of type `ty` with `lengths`, one `i64` per dimension, outermost first: the
    /// elements allocated and the header built. Panics when the shape's element count does not
    /// fit in an `i64`; the runtime panics when a length is negative or memory runs out.
    ///
    /// Cost: one allocation, of the elements only; they are not initialized.
    pub(crate) fn new_array(
        &mut self,
        ty: Type<'db>,
        lengths: &[Operand<'db>],
    ) -> Result<String, Unsupported> {
        let (element, rank) = self.array(ty)?;
        let header = self.types.value(ty)?;
        let layout = types::layout(self.types.db, element, self.types.declared)
            .map_err(|_| self.types.unsupported(element))?;
        let lengths = lengths
            .iter()
            .map(|length| self.value(length).map(|(value, _)| value))
            .collect::<Result<Vec<_>, _>>()?;
        if lengths.len() != rank as usize {
            return Err(Unsupported::new(
                "a new array with the wrong number of lengths",
            ));
        }
        // The stride of each dimension is the product of the lengths after it, and the element
        // count is the first stride times the first length; each product is checked.
        let mut strides = vec!["1".to_owned(); lengths.len()];
        let mut count = "1".to_owned();
        for dimension in (0..lengths.len()).rev() {
            strides[dimension].clone_from(&count);
            count = if count == "1" {
                lengths[dimension].clone()
            } else {
                self.checked_multiply(&count, &lengths[dimension])
            };
        }
        let location = self.constants.location(self.span);
        self.declare(
            "baryon_rt_allocate_array",
            "declare noalias nonnull ptr @baryon_rt_allocate_array(i64, i64, i64, ptr) nounwind",
        );
        let data = self.emit(&format!(
            "call ptr @baryon_rt_allocate_array(i64 {count}, i64 {}, i64 {}, ptr {location})",
            layout.size(),
            layout.align()
        ));
        let mut value = self.emit(&format!("insertvalue {header} poison, ptr {data}, 0"));
        for (part, values) in [(1, &lengths), (2, &strides)] {
            for (dimension, item) in values.iter().enumerate() {
                value = self.emit(&format!(
                    "insertvalue {header} {value}, i64 {item}, {part}, {dimension}"
                ));
            }
        }
        Ok(value)
    }

    /// `left * right` on `i64`, panicking when the product overflows: an array of that shape
    /// could not fit in memory.
    fn checked_multiply(&mut self, left: &str, right: &str) -> String {
        let pair = self.emit(&format!(
            "call {{ i64, i1 }} @llvm.smul.with.overflow.i64(i64 {left}, i64 {right})"
        ));
        self.declare(
            "llvm.smul.with.overflow.i64",
            "declare { i64, i1 } @llvm.smul.with.overflow.i64(i64, i64)",
        );
        let overflowed = self.emit(&format!("extractvalue {{ i64, i1 }} {pair}, 1"));
        let fits = self.emit(&format!("xor i1 {overflowed}, true"));
        self.check(
            &fits,
            &Failure::Message("the array's shape has too many elements for memory".to_owned()),
        );
        self.emit(&format!("extractvalue {{ i64, i1 }} {pair}, 0"))
    }

    /// The length of `dimension` of the array at `place`, or the number of elements of the list
    /// there, an `i64`.
    pub(crate) fn length(&mut self, place: &Place, dimension: u32) -> Result<String, Unsupported> {
        let (address, ty) = self.place(place)?;
        if matches!(ty.kind(self.types.db), TypeKind::List(_)) {
            return Ok(self.list_length(&address));
        }
        if matches!(ty.kind(self.types.db), TypeKind::Dict { .. }) {
            return Ok(self.dict_length(&address));
        }
        let header = self.types.value(ty)?;
        let field = self.emit(&format!(
            "getelementptr inbounds {header}, ptr {address}, i32 0, i32 1, i32 {dimension}"
        ));
        Ok(self.emit(&format!("load i64, ptr {field}")))
    }

    /// The address of the element at `indices` of the array of type `ty` at `address`, and the
    /// element type. A `Fixed` array holds its elements in place, so that is one
    /// `getelementptr` through its nested LLVM arrays; an `Array` or a list goes through its
    /// header (the `list` module). Each index local holds an integer of any width; it is
    /// widened to `i64`.
    pub(crate) fn element(
        &mut self,
        address: &str,
        ty: Type<'db>,
        indices: &[Local],
    ) -> Result<(String, Type<'db>), Unsupported> {
        if let TypeKind::Fixed { element, shape } = ty.kind(self.types.db) {
            if indices.len() != shape.rank() {
                return Err(Unsupported::new(
                    "an index with the wrong number of dimensions",
                ));
            }
            let llvm = self.types.value(ty)?;
            let mut steps = vec!["i64 0".to_owned()];
            for &index in indices {
                steps.push(format!("i64 {}", self.index(index)?));
            }
            let address = self.emit(&format!(
                "getelementptr inbounds {llvm}, ptr {address}, {}",
                steps.join(", ")
            ));
            return Ok((address, *element));
        }
        if matches!(ty.kind(self.types.db), TypeKind::List(_)) {
            return self.list_element(address, ty, indices);
        }
        if matches!(ty.kind(self.types.db), TypeKind::Dict { .. }) {
            return self.dict_entry(address, ty, indices);
        }
        let (element, rank) = self.array(ty)?;
        if indices.len() != rank as usize {
            return Err(Unsupported::new(
                "an index with the wrong number of dimensions",
            ));
        }
        let header = self.types.value(ty)?;
        let data = self.emit(&format!("load ptr, ptr {address}"));
        let mut offset = "0".to_owned();
        for (dimension, &index) in indices.iter().enumerate() {
            let index = self.index(index)?;
            let field = self.emit(&format!(
                "getelementptr inbounds {header}, ptr {address}, i32 0, i32 2, i32 {dimension}"
            ));
            let stride = self.emit(&format!("load i64, ptr {field}"));
            let step = self.emit(&format!("mul i64 {index}, {stride}"));
            offset = if offset == "0" {
                step
            } else {
                self.emit(&format!("add i64 {offset}, {step}"))
            };
        }
        let llvm = self.types.value(element)?;
        let address = self.emit(&format!(
            "getelementptr inbounds {llvm}, ptr {data}, i64 {offset}"
        ));
        Ok((address, element))
    }

    /// The value of the index local `index` as an `i64`, widened by its sign.
    pub(crate) fn index(&mut self, index: Local) -> Result<String, Unsupported> {
        let (value, ty) = self.value(&Operand::Copy(index.into()))?;
        self.widen(&value, ty)
    }

    /// The integer `value` of type `ty` as an `i64`, widened by its sign.
    pub(crate) fn widen(&mut self, value: &str, ty: Type<'db>) -> Result<String, Unsupported> {
        match self.types.number(ty) {
            Some(crate::ty::Number::Integer { bits: 64, .. }) => Ok(value.to_owned()),
            Some(crate::ty::Number::Integer { bits, signed }) => {
                let extend = if signed { "sext" } else { "zext" };
                Ok(self.emit(&format!("{extend} i{bits} {value} to i64")))
            }
            _ => Err(self.types.unsupported(ty)),
        }
    }

    /// Ends the value at `place`: frees an array's elements, and every array inside a tuple or
    /// struct; any other value owns no memory, so dropping it does nothing.
    pub(crate) fn drop_place(&mut self, place: &Place) -> Result<(), Unsupported> {
        let (address, ty) = self.place(place)?;
        self.drop_value(&address, ty)
    }

    /// Frees what the value of type `ty` at `address` owns.
    pub(crate) fn drop_value(&mut self, address: &str, ty: Type<'db>) -> Result<(), Unsupported> {
        match ty.kind(self.types.db) {
            TypeKind::Array { .. } => self.free_array(address, ty),
            TypeKind::List(_) => self.free_list(address, ty),
            TypeKind::Dict { .. } => self.free_dict(address, ty),
            TypeKind::Str => {
                self.free_text(address);
                Ok(())
            }
            TypeKind::Tuple(_) | TypeKind::Struct(_) => {
                let container = self.types.value(ty)?;
                let mut position = 0;
                while let Ok(field) = self.field(ty, position) {
                    if self.owns_memory(field) {
                        let at = self.emit(&format!(
                            "getelementptr inbounds {container}, ptr {address}, i32 0, i32 {position}"
                        ));
                        self.drop_value(&at, field)?;
                    }
                    position += 1;
                }
                Ok(())
            }
            TypeKind::Optional(value) if self.owns_memory(*value) => {
                let container = self.types.value(ty)?;
                let flag = self.emit(&format!(
                    "getelementptr inbounds {container}, ptr {address}, i32 0, i32 1"
                ));
                let present = self.emit(&format!("load i1, ptr {flag}"));
                let (drop, done) = (self.fresh_block(), self.fresh_block());
                self.line(&format!("br i1 {present}, label %{drop}, label %{done}"));
                self.line_label(&drop);
                let inside = self.emit(&format!(
                    "getelementptr inbounds {container}, ptr {address}, i32 0, i32 0"
                ));
                self.drop_value(&inside, *value)?;
                self.line(&format!("br label %{done}"));
                self.line_label(&done);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Whether a value of `ty` owns memory a drop frees: an array, a list, a `dict` or a `str`, or a
    /// tuple, struct or optional holding one.
    pub(crate) fn owns_memory(&self, ty: Type<'db>) -> bool {
        match ty.kind(self.types.db) {
            TypeKind::Array { .. } | TypeKind::List(_) | TypeKind::Dict { .. } | TypeKind::Str => {
                true
            }
            TypeKind::Optional(value) => self.owns_memory(*value),
            TypeKind::Tuple(_) | TypeKind::Struct(_) => (0..)
                .map_while(|position| self.field(ty, position).ok())
                .any(|field| self.owns_memory(field)),
            _ => false,
        }
    }

    /// Frees the elements of the array of type `ty` at `address`, with the size they were
    /// allocated with: the product of the lengths times the element size.
    fn free_array(&mut self, address: &str, ty: Type<'db>) -> Result<(), Unsupported> {
        let (element, rank) = self.array(ty)?;
        let header = self.types.value(ty)?;
        let layout = types::layout(self.types.db, element, self.types.declared)
            .map_err(|_| self.types.unsupported(element))?;
        let data = self.emit(&format!("load ptr, ptr {address}"));
        // The product cannot overflow: the allocation checked it.
        let mut bytes = layout.size().to_string();
        for dimension in 0..rank {
            let field = self.emit(&format!(
                "getelementptr inbounds {header}, ptr {address}, i32 0, i32 1, i32 {dimension}"
            ));
            let length = self.emit(&format!("load i64, ptr {field}"));
            bytes = self.emit(&format!("mul i64 {bytes}, {length}"));
        }
        self.declare(
            "baryon_rt_deallocate",
            "declare void @baryon_rt_deallocate(ptr, i64, i64) nounwind",
        );
        self.line(&format!(
            "call void @baryon_rt_deallocate(ptr {data}, i64 {bytes}, i64 {})",
            layout.align()
        ));
        Ok(())
    }

    /// The element type and rank of the `Array` type `ty`.
    pub(crate) fn array(&self, ty: Type<'db>) -> Result<(Type<'db>, u32), Unsupported> {
        match ty.kind(self.types.db) {
            TypeKind::Array { element, rank } => Ok((*element, *rank)),
            _ => Err(self.types.unsupported(ty)),
        }
    }
}
