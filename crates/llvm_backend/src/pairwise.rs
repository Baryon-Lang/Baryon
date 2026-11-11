//! The built-ins that sum a whole-array expression one pairwise leaf at a time (decision 0041):
//! [`PairwiseLeaves`](mir::Builtin::PairwiseLeaves) and
//! [`PairwiseLeafEnd`](mir::Builtin::PairwiseLeafEnd) call `baryon_rt_pairwise_leaves` and
//! `baryon_rt_pairwise_leaf_end` with `i64` values, and
//! [`PairwiseCombine`](mir::Builtin::PairwiseCombine) passes the count, the address of the first
//! leaf sum and the number of sums to `baryon_rt_pairwise_combine_f64` or its `f32` form. Array
//! expansion allocates the leaf sums as a new `Array`, so they are one contiguous run.

use crate::Unsupported;
use crate::function::{Function, block_label};
use crate::ty::Number;
use mir::{Argument, BlockId, Place};

impl<'db> Function<'_, '_, 'db> {
    /// Writes `destination = baryon_rt_pairwise_leaves(count)` or, with a `start`,
    /// `baryon_rt_pairwise_leaf_end(count, start)`, and goes to `target`.
    pub(crate) fn pairwise_leaf(
        &mut self,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let mut values = Vec::with_capacity(2);
        for argument in arguments {
            let Argument::Owned(operand) = argument else {
                return Err(Unsupported::new("a pairwise leaf with a borrowed argument"));
            };
            let (value, _) = self.value(operand)?;
            values.push(format!("i64 {value}"));
        }
        let (function, parameters) = match values.len() {
            1 => ("baryon_rt_pairwise_leaves", "i64"),
            2 => ("baryon_rt_pairwise_leaf_end", "i64, i64"),
            _ => return Err(Unsupported::new("a pairwise leaf with these arguments")),
        };
        self.declare(
            function,
            &format!("declare i64 @{function}({parameters}) nounwind"),
        );
        let result = self.emit(&format!("call i64 @{function}({})", values.join(", ")));
        let (address, _) = self.place(destination)?;
        self.line(&format!("store i64 {result}, ptr {address}"));
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }

    /// Writes `destination` as the pairwise sum of `count` values from their leaf sums, and goes
    /// to `target`.
    pub(crate) fn pairwise_combine(
        &mut self,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let [Argument::Owned(count), Argument::Borrow(sums)] = arguments else {
            return Err(Unsupported::new("a pairwise combine with these arguments"));
        };
        let (address, ty) = self.place(destination)?;
        let (llvm, suffix) = match self.types.number(ty) {
            Some(Number::Float(32)) => ("float", "f32"),
            Some(Number::Float(_)) => ("double", "f64"),
            _ => return Err(self.types.unsupported(ty)),
        };
        let (count, _) = self.value(count)?;
        let sums = self.reduced(sums)?;
        let function = format!("baryon_rt_pairwise_combine_{suffix}");
        self.declare(
            &function,
            &format!("declare {llvm} @{function}(i64, ptr, i64) nounwind"),
        );
        let result = self.emit(&format!(
            "call {llvm} @{function}(i64 {count}, ptr {}, i64 {})",
            sums.data, sums.count
        ));
        self.line(&format!("store {llvm} {result}, ptr {address}"));
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }
}
