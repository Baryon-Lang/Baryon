//! The functions compiled code calls for complex numbers (the `complex` module): `print` of one,
//! and `**`. Each takes the parts as separate floats, which every C calling convention passes the
//! same way, and `**` writes its result through a pointer to `{ real, imaginary }`, the layout of
//! the complex type.

use crate::{complex, print};

/// Prints a `complex128` as Python's `repr` writes it: `(1+2j)`, `2j`, `(-0-2j)`.
///
/// # Example
///
/// ```
/// runtime::baryon_rt_print_complex128(1, 1.0, 2.0); // (1+2j)
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub extern "C" fn baryon_rt_print_complex128(stream: i32, real: f64, imaginary: f64) {
    let positive_zero = real == 0.0 && real.is_sign_positive();
    let text = complex::repr(
        &format!("{real:e}"),
        &format!("{imaginary:e}"),
        positive_zero,
    );
    print::write(stream, &text);
}

/// Prints a `complex64` as `NumPy`'s `repr` writes one: Python's form, with each part's fewest
/// digits that read back as the same `f32`, so `0.1` prints `0.1`.
///
/// # Example
///
/// ```
/// runtime::baryon_rt_print_complex64(1, 0.1, 0.2); // (0.1+0.2j)
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub extern "C" fn baryon_rt_print_complex64(stream: i32, real: f32, imaginary: f32) {
    let positive_zero = real == 0.0 && real.is_sign_positive();
    let text = complex::repr(
        &format!("{real:e}"),
        &format!("{imaginary:e}"),
        positive_zero,
    );
    print::write(stream, &text);
}

/// Writes `(base_real + base_imaginary j) ** (exponent_real + exponent_imaginary j)` to
/// `result`, computed as CPython's `complex.__pow__` computes it, with `nan+nanj` where Python
/// raises `ZeroDivisionError`.
///
/// # Safety
///
/// `result` points to two writable `f64`s, the real part then the imaginary part.
///
/// # Example
///
/// ```
/// let mut result = [0.0_f64; 2];
/// // SAFETY: `result` is two writable `f64`s.
/// unsafe { runtime::baryon_rt_power_complex128(result.as_mut_ptr(), 1.0, 2.0, 2.0, 0.0) };
/// assert_eq!(result, [-3.0, 4.0]);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_power_complex128(
    result: *mut f64,
    base_real: f64,
    base_imaginary: f64,
    exponent_real: f64,
    exponent_imaginary: f64,
) {
    let (real, imaginary) = complex::power(
        (base_real, base_imaginary),
        (exponent_real, exponent_imaginary),
    );
    // SAFETY: the caller guarantees two writable `f64`s at `result`.
    unsafe { result.cast::<[f64; 2]>().write([real, imaginary]) };
}

/// Writes a `complex64` power to `result`: the parts are widened to `f64`, the power computed
/// as [`baryon_rt_power_complex128`] computes it, and each part rounded to the nearest `f32`,
/// which is at least as accurate as computing in `f32`.
///
/// # Safety
///
/// `result` points to two writable `f32`s, the real part then the imaginary part.
///
/// # Example
///
/// ```
/// let mut result = [0.0_f32; 2];
/// // SAFETY: `result` is two writable `f32`s.
/// unsafe { runtime::baryon_rt_power_complex64(result.as_mut_ptr(), 0.0, 1.0, 2.0, 0.0) };
/// assert_eq!(result, [-1.0, 0.0]);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_power_complex64(
    result: *mut f32,
    base_real: f32,
    base_imaginary: f32,
    exponent_real: f32,
    exponent_imaginary: f32,
) {
    let (real, imaginary) = complex::power(
        (f64::from(base_real), f64::from(base_imaginary)),
        (f64::from(exponent_real), f64::from(exponent_imaginary)),
    );
    #[expect(
        clippy::cast_possible_truncation,
        reason = "rounding to the nearest f32 is the point"
    )]
    let parts = [real as f32, imaginary as f32];
    // SAFETY: the caller guarantees two writable `f32`s at `result`.
    unsafe { result.cast::<[f32; 2]>().write(parts) };
}
