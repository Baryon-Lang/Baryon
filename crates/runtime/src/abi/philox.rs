//! The functions compiled code calls for `random.Philox` (the `philox` module, decision 0028):
//! one to make a generator in place and one to draw a float from it. A generator is 88 bytes in
//! the caller's memory, laid out as [`Philox`]; the runtime keeps nothing of its own.

use crate::location::Location;
use crate::panic::{Failure, panic};
use crate::philox::Philox;

/// Writes the generator for `seed` at `stream` to `state`: `Philox(seed, stream=stream)`.
///
/// # Panics
///
/// Ends the program with a panic, reporting `location`, when `seed` or `stream` is negative, as
/// `NumPy` rejects a negative key.
///
/// # Safety
///
/// `state` points to 88 writable bytes aligned to 8, and `location` is null or points to a valid
/// [`Location`] that lives as long as the program.
///
/// # Example
///
/// ```
/// use runtime::{Philox, baryon_rt_philox_new, baryon_rt_philox_uniform};
///
/// let mut state = std::mem::MaybeUninit::<Philox>::uninit();
/// // SAFETY: the state is a `Philox`'s memory; a null location is allowed.
/// let first = unsafe {
///     baryon_rt_philox_new(state.as_mut_ptr(), 2026, 0, std::ptr::null());
///     baryon_rt_philox_uniform(state.as_mut_ptr())
/// };
/// assert_eq!(first, 0.422_034_180_346_073);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_philox_new(
    state: *mut Philox,
    seed: i64,
    stream: i64,
    location: *const Location,
) {
    let (Ok(seed), Ok(stream)) = (u64::try_from(seed), u64::try_from(stream)) else {
        let message = "a `Philox` seed and stream must not be negative";
        // SAFETY: the caller's promise about `location`, passed on.
        panic(Failure::Message(message), unsafe {
            super::location(location)
        });
    };
    // SAFETY: the caller guarantees `state` is writable and aligned for a `Philox`.
    unsafe { state.write(Philox::new(seed, stream)) };
}

/// The next float in `[0, 1)` from the generator at `state`: `generator.uniform()`.
///
/// # Safety
///
/// `state` points to a generator [`baryon_rt_philox_new`] wrote, which nothing else reads or
/// writes during the call.
///
/// # Example
///
/// See [`baryon_rt_philox_new`].
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_philox_uniform(state: *mut Philox) -> f64 {
    // SAFETY: the caller guarantees an initialized generator that only this call uses.
    let generator = unsafe { &mut *state };
    generator.uniform()
}
