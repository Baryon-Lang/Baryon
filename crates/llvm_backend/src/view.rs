//! Views: the header of a slice of an array, built where a place takes a
//! [`Slice`](mir::Projection::Slice) step.
//!
//! A view is an `Array` header, `{ ptr, [rank x i64], [rank x i64] }`, that points into another
//! array's elements and owns none of them. For each dimension of the base, an index `i` moves
//! the data pointer by `i * stride` and drops the dimension; a range moves it by
//! `start * stride` and keeps the dimension with the range's length and `stride * step` as its
//! stride. The base may be an `Array`, whose pointer and strides are read from its header, or a
//! `Fixed`, whose elements are in place in row-major order, so its strides are constants.
//!
//! The header goes in a stack slot of its own that the entry block makes, so building a view
//! inside a loop does not grow the stack; LLVM keeps its fields in registers once it sees the
//! slot is only stored and loaded.

use crate::Unsupported;
use crate::function::Function;
use mir::SliceIndex;
use std::fmt::Write as _;
use types::{Type, TypeKind};

impl<'db> Function<'_, '_, 'db> {
    /// The address of the header of the view `dimensions` of the array of type `ty` at
    /// `address`, built in a new stack slot, and the view's type: an `Array` of the base's
    /// element type with one dimension per range.
    ///
    /// Cost: a multiply and an add per dimension, and a store of the header.
    pub(crate) fn view(
        &mut self,
        address: &str,
        ty: Type<'db>,
        dimensions: &[SliceIndex],
    ) -> Result<(String, Type<'db>), Unsupported> {
        let (element, data, strides) = match ty.kind(self.types.db) {
            TypeKind::Fixed { element, shape } => {
                // Row-major: each stride is the product of the lengths after it.
                let mut strides = vec![1_u64; shape.rank()];
                for dimension in (0..shape.rank().saturating_sub(1)).rev() {
                    strides[dimension] = strides[dimension + 1] * shape.lengths()[dimension + 1];
                }
                let strides = strides.iter().map(ToString::to_string).collect();
                (*element, address.to_owned(), strides)
            }
            &TypeKind::Array { element, rank } => {
                let header = self.types.value(ty)?;
                let data = self.emit(&format!("load ptr, ptr {address}"));
                let mut strides = Vec::new();
                for dimension in 0..rank {
                    let field = self.emit(&format!(
                        "getelementptr inbounds {header}, ptr {address}, i32 0, i32 2, i32 {dimension}"
                    ));
                    strides.push(self.emit(&format!("load i64, ptr {field}")));
                }
                (element, data, strides)
            }
            _ => return Err(self.types.unsupported(ty)),
        };
        if strides.len() != dimensions.len() {
            return Err(Unsupported::new(
                "a slice with the wrong number of dimensions",
            ));
        }
        let mut offset = "0".to_owned();
        let (mut lengths, mut steps) = (Vec::new(), Vec::new());
        for (dimension, stride) in dimensions.iter().zip(&strides) {
            let first = match dimension {
                SliceIndex::Index(index) => self.index(*index)?,
                SliceIndex::Range {
                    start,
                    length,
                    step,
                } => {
                    lengths.push(self.index(*length)?);
                    let step = self.index(*step)?;
                    steps.push(self.emit(&format!("mul i64 {step}, {stride}")));
                    self.index(*start)?
                }
            };
            let moved = self.emit(&format!("mul i64 {first}, {stride}"));
            offset = if offset == "0" {
                moved
            } else {
                self.emit(&format!("add i64 {offset}, {moved}"))
            };
        }
        let rank = u32::try_from(lengths.len())
            .map_err(|_| Unsupported::new("a slice with too many dimensions"))?;
        let view = Type::new(self.types.db, TypeKind::Array { element, rank });
        let header = self.types.value(view)?;
        let llvm = self.types.value(element)?;
        // Not `inbounds`: an empty view may start one past the end, or before a reversed one.
        let start = self.emit(&format!("getelementptr {llvm}, ptr {data}, i64 {offset}"));
        let mut value = self.emit(&format!("insertvalue {header} poison, ptr {start}, 0"));
        for (part, values) in [(1, &lengths), (2, &steps)] {
            for (dimension, item) in values.iter().enumerate() {
                value = self.emit(&format!(
                    "insertvalue {header} {value}, i64 {item}, {part}, {dimension}"
                ));
            }
        }
        let slot = self.fresh();
        let _ = writeln!(self.slots, "  {slot} = alloca {header}");
        self.line(&format!("store {header} {value}, ptr {slot}"));
        Ok((slot, view))
    }
}
