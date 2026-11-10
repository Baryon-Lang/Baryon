//! Printing an `Array` or a `Fixed` array as `NumPy`'s `print` writes an `ndarray` (roadmap
//! 1.10.15, decision 0033): `[1. 2. 3.]`, a matrix one row per line, and only the corners of an
//! array of more than 1000 elements.
//!
//! Every element's text depends on all the others, since they share one width and one notation,
//! so the formatting cannot be laid out element by element as a list's is. The compiled code
//! makes one call, `baryon_rt_print_array`, passing the array's header parts and a number for its
//! element type, and the runtime does the rest:
//!
//! | Argument | `Array[T, rank]` | `Fixed[T, shape]` |
//! | --- | --- | --- |
//! | first element | the header's pointer | the value, stored in a stack slot |
//! | rank | the type's rank | the shape's rank |
//! | lengths | the header's lengths, in a stack slot | a constant array in a stack slot |
//! | strides | the header's strides, in elements | row-major strides, as constants |
//!
//! A view prints the elements it sees, since its strides go with it. The element type is one of
//! the runtime's `ELEMENT_*` numbers, so arrays of `bool`, integers, floats and complex numbers
//! print; an array of any other element type, such as `str` or a struct, is reported as not
//! built.
//!
//! Cost: one call, however large the array; the runtime reads at most 1000 elements.

use crate::Unsupported;
use crate::function::Function;
use crate::ty::Number;
use types::{Float, Type, TypeKind};

impl<'db> Function<'_, '_, 'db> {
    /// Prints the `Array` or `Fixed` array `value`, of type `ty`, to `stream`.
    pub(crate) fn print_array(
        &mut self,
        stream: u8,
        value: &str,
        ty: Type<'db>,
    ) -> Result<(), Unsupported> {
        let llvm = self.types.value(ty)?;
        let slot = self.slot(&llvm);
        self.line(&format!("store {llvm} {value}, ptr {slot}"));
        let (element, rank, data, lengths, strides) = match ty.kind(self.types.db) {
            &TypeKind::Array { element, rank } => {
                let data = self.emit(&format!("extractvalue {llvm} {value}, 0"));
                let [lengths, strides] = [1, 2].map(|part| {
                    self.emit(&format!(
                        "getelementptr inbounds {llvm}, ptr {slot}, i32 0, i32 {part}, i32 0"
                    ))
                });
                (element, rank, data, lengths, strides)
            }
            TypeKind::Fixed { element, shape } => {
                // Row-major: each stride is the product of the lengths after it.
                let lengths = shape.lengths();
                let mut strides = vec![1_u64; lengths.len()];
                for dimension in (0..lengths.len().saturating_sub(1)).rev() {
                    strides[dimension] = strides[dimension + 1] * lengths[dimension + 1];
                }
                let rank = u32::try_from(lengths.len())
                    .map_err(|_| Unsupported::new("an array with too many dimensions"))?;
                let lengths = self.constant_indices(lengths);
                let strides = self.constant_indices(&strides);
                (*element, rank, slot, lengths, strides)
            }
            _ => return Err(self.printing(ty)),
        };
        let code = self
            .element_code(element)
            .ok_or_else(|| self.printing(ty))?;
        self.declare(
            "baryon_rt_print_array",
            "declare void @baryon_rt_print_array(i32, i32, ptr, i32, ptr, ptr) nounwind",
        );
        self.line(&format!(
            "call void @baryon_rt_print_array(i32 {stream}, i32 {code}, ptr {data}, i32 {rank}, \
             ptr {lengths}, ptr {strides})"
        ));
        Ok(())
    }

    /// A stack slot holding the `i64`s `values`, and its address.
    fn constant_indices(&mut self, values: &[u64]) -> String {
        let llvm = format!("[{} x i64]", values.len());
        let items: Vec<String> = values.iter().map(|value| format!("i64 {value}")).collect();
        let slot = self.slot(&llvm);
        self.line(&format!("store {llvm} [{}], ptr {slot}", items.join(", ")));
        slot
    }

    /// The runtime's number for arrays of `element`, one of its `ELEMENT_*` constants, or `None`
    /// for an element type it cannot print.
    fn element_code(&self, element: Type<'db>) -> Option<u32> {
        if let Some(part) = self.complex_part(element) {
            return Some(if part == Float::F32 { 11 } else { 12 });
        }
        if !matches!(element.kind(self.types.db), TypeKind::Primitive(_)) {
            return None;
        }
        Some(match self.types.number(element)? {
            Number::Bool => 0,
            Number::Integer { bits, signed } => {
                let width = match bits {
                    8 => 1,
                    16 => 2,
                    32 => 3,
                    64 => 4,
                    _ => return None,
                };
                if signed { width } else { width + 4 }
            }
            Number::Float(32) => 9,
            Number::Float(_) => 10,
        })
    }
}
