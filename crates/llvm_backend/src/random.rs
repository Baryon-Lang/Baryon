//! `random.Philox` (decision 0028): making a generator and drawing from it are calls of the
//! runtime's `baryon_rt_philox_new` and `baryon_rt_philox_uniform`, each given the address of
//! the generator's 88 bytes, which live in the caller's frame like any other local.

use crate::Unsupported;
use crate::function::{Function, block_label};
use mir::{Argument, BlockId, Place};

impl<'db> Function<'_, '_, 'db> {
    /// Writes `destination = Philox(seed, stream)` and goes to `target`. The runtime writes the
    /// generator in place, and panics at this call's location on a negative seed or stream.
    pub(crate) fn philox(
        &mut self,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let [Argument::Owned(seed), Argument::Owned(stream)] = arguments else {
            return Err(Unsupported::new("`random.Philox` with these arguments"));
        };
        let (seed, _) = self.value(seed)?;
        let (stream, _) = self.value(stream)?;
        let (address, _) = self.place(destination)?;
        let location = self.constants.location(self.span);
        self.declare(
            "baryon_rt_philox_new",
            "declare void @baryon_rt_philox_new(ptr, i64, i64, ptr) nounwind",
        );
        self.line(&format!(
            "call void @baryon_rt_philox_new(ptr {address}, i64 {seed}, i64 {stream}, ptr {location})"
        ));
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }

    /// Writes `destination = generator.uniform()`, the generator passed by address, and goes to
    /// `target`.
    pub(crate) fn uniform(
        &mut self,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let [Argument::InOut(generator)] = arguments else {
            return Err(Unsupported::new("`Philox.uniform` with these arguments"));
        };
        let (generator, _) = self.place(generator)?;
        let (address, _) = self.place(destination)?;
        self.declare(
            "baryon_rt_philox_uniform",
            "declare double @baryon_rt_philox_uniform(ptr) nounwind",
        );
        let value = self.emit(&format!(
            "call double @baryon_rt_philox_uniform(ptr {generator})"
        ));
        self.line(&format!("store double {value}, ptr {address}"));
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }
}
