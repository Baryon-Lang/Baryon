//! The functions compiled code calls for `round(x, digits)` (roadmap 1.13.13): the number nearest
//! `x` that is a multiple of `10 ** -digits`, with ties to the even multiple, as Python's `round`
//! gives.
//!
//! A float rounds its exact binary value, as Python does, not the shorter text that prints for
//! it: `0.125` is exactly a tie and rounds to `0.12`, while `2.675` is a little below
//! `2.675` and rounds to `2.67`. The decimal digits come from Rust's exact float formatting and
//! go back through its correctly rounded parsing, so the result is the float nearest the
//! rounded decimal. `round` of an integer to a negative number of digits rounds it to a
//! multiple of a power of ten; to zero or more digits, it is the integer itself.

use crate::Location;
use crate::panic::{Failure, panic};

/// `round(x, digits)` of an `f64`. NaN, the infinities and the zeros come back unchanged, as in
/// Python; a sum too large to hold comes back infinite, where Python raises `OverflowError`.
///
/// # Example
///
/// ```
/// use runtime::baryon_rt_round_f64;
///
/// assert_eq!(baryon_rt_round_f64(2.675, 2), 2.67);
/// assert_eq!(baryon_rt_round_f64(0.125, 2), 0.12);
/// assert_eq!(baryon_rt_round_f64(1250.0, -2), 1200.0);
/// assert_eq!(baryon_rt_round_f64(1250.5, -2), 1300.0);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub extern "C" fn baryon_rt_round_f64(x: f64, digits: i64) -> f64 {
    if !x.is_finite() || x == 0.0 || digits > 400 {
        return x;
    }
    if digits < -400 {
        return 0.0 * x;
    }
    let text = if digits >= 0 {
        format!("{:.*}", usize::try_from(digits).unwrap_or(0), x)
    } else {
        let whole = x.abs().trunc();
        let rounded = round_whole(
            &format!("{whole:.0}"),
            whole.to_bits() != x.abs().to_bits(),
            digits.unsigned_abs(),
        );
        if x < 0.0 {
            format!("-{rounded}")
        } else {
            rounded
        }
    };
    text.parse().unwrap_or(x)
}

/// `round(x, digits)` of an `f32`, rounding the `f32`'s own exact value and giving the `f32`
/// nearest the result, as [`baryon_rt_round_f64`] does for an `f64`.
///
/// # Example
///
/// ```
/// assert_eq!(runtime::baryon_rt_round_f32(0.125, 2), 0.12);
/// assert_eq!(runtime::baryon_rt_round_f32(-1250.0, -2), -1200.0);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub extern "C" fn baryon_rt_round_f32(x: f32, digits: i64) -> f32 {
    if !x.is_finite() || x == 0.0 || digits > 400 {
        return x;
    }
    if digits < -400 {
        return 0.0 * x;
    }
    let text = if digits >= 0 {
        format!("{:.*}", usize::try_from(digits).unwrap_or(0), x)
    } else {
        let whole = x.abs().trunc();
        let rounded = round_whole(
            &format!("{whole:.0}"),
            whole.to_bits() != x.abs().to_bits(),
            digits.unsigned_abs(),
        );
        if x < 0.0 {
            format!("-{rounded}")
        } else {
            rounded
        }
    };
    text.parse().unwrap_or(x)
}

/// `round(n, digits)` of an `i64`: `n` itself for zero or more digits, else the nearest multiple
/// of `10 ** -digits`, with ties to the even multiple, as Python's `round` of an `int` gives.
///
/// # Panics
///
/// With ``integer overflow in `round` `` at `location` when the result does not fit in an
/// `i64`, which Python's integers never overflow.
///
/// # Safety
///
/// `location` is null or points to a valid [`Location`].
///
/// # Example
///
/// ```
/// use runtime::baryon_rt_round_i64;
///
/// let null = std::ptr::null();
/// // SAFETY: a null location is allowed.
/// unsafe {
///     assert_eq!(baryon_rt_round_i64(1250, -2, null), 1200);
///     assert_eq!(baryon_rt_round_i64(1350, -2, null), 1400);
///     assert_eq!(baryon_rt_round_i64(-1251, -2, null), -1300);
///     assert_eq!(baryon_rt_round_i64(7, 3, null), 7);
/// }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_round_i64(
    n: i64,
    digits: i64,
    location: *const Location,
) -> i64 {
    if digits >= 0 {
        return n;
    }
    let Some(power) = u32::try_from(digits.unsigned_abs())
        .ok()
        .and_then(|exponent| 10_i128.checked_pow(exponent))
    else {
        // A power past 10**38 is more than twice any `i64`, so the nearest multiple is 0.
        return 0;
    };
    let value = i128::from(n);
    let (quotient, remainder) = (value.div_euclid(power), value.rem_euclid(power));
    let up = match (2 * remainder).cmp(&power) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Equal => quotient % 2 != 0,
        std::cmp::Ordering::Less => false,
    };
    let rounded = (quotient + i128::from(up)) * power;
    i64::try_from(rounded).unwrap_or_else(|_| {
        // SAFETY: the caller's promise about `location`, passed on.
        panic(Failure::Message("integer overflow in `round`"), unsafe {
            super::location(location)
        })
    })
}

/// The digits of the whole number `whole`, rounded to a multiple of `10 ** places` with ties to
/// even; `fraction` says whether the number had a nonzero fraction after `whole`, which breaks a
/// tie upward. The result is digits followed by `places` zeros, or `"0"`.
fn round_whole(whole: &str, fraction: bool, places: u64) -> String {
    let places = usize::try_from(places).unwrap_or(usize::MAX);
    let digits = whole.as_bytes();
    if places > digits.len() {
        // Below `10 ** (places - 1)`, so less than half the multiple.
        return "0".to_owned();
    }
    let (kept, dropped) = digits.split_at(digits.len() - places);
    let (first, rest) = dropped
        .split_first()
        .map_or((b'0', &[][..]), |(&first, rest)| (first, rest));
    let beyond_half = rest.iter().any(|&digit| digit != b'0') || fraction;
    let odd = kept.last().is_some_and(|&digit| (digit - b'0') % 2 == 1);
    let up = first > b'5' || (first == b'5' && (beyond_half || odd));
    let mut result: Vec<u8> = kept.to_vec();
    if up {
        let mut carry = true;
        for digit in result.iter_mut().rev() {
            if *digit == b'9' {
                *digit = b'0';
            } else {
                *digit += 1;
                carry = false;
                break;
            }
        }
        if carry {
            result.insert(0, b'1');
        }
    }
    if result.iter().all(|&digit| digit == b'0') {
        return "0".to_owned();
    }
    result.extend(std::iter::repeat_n(b'0', places));
    String::from_utf8(result).unwrap_or_else(|_| "0".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each case is what CPython 3.12's `round` gives.
    #[test]
    fn floats_round_their_exact_value_with_ties_to_even() {
        let cases = [
            (2.675, 2, 2.67),
            (0.125, 2, 0.12),
            (0.375, 2, 0.38),
            (2.5, 0, 2.0),
            (3.5, 0, 4.0),
            (-0.5, 0, -0.0),
            (1234.5678, 1, 1234.6),
            (1250.0, -2, 1200.0),
            (1350.0, -2, 1400.0),
            (1250.0001, -2, 1300.0),
            (49.0, -2, 0.0),
            (950.0, -3, 1000.0),
            (99_999.0, -2, 100_000.0),
            (1e300, 2, 1e300),
            (0.1, 400, 0.1),
        ];
        for (x, digits, expected) in cases {
            let rounded = baryon_rt_round_f64(x, digits);
            assert_eq!(
                rounded.to_bits(),
                f64::to_bits(expected),
                "round({x}, {digits})"
            );
        }
        assert!(baryon_rt_round_f64(f64::NAN, 2).is_nan());
        assert_eq!(
            baryon_rt_round_f64(-1.0, -2).to_bits(),
            (-0.0_f64).to_bits()
        );
    }

    #[test]
    #[allow(unsafe_code, reason = "calls the exported function")]
    fn integers_round_to_negative_digits() {
        let round = |n, digits| {
            // SAFETY: a null location is allowed.
            unsafe { baryon_rt_round_i64(n, digits, std::ptr::null()) }
        };
        assert_eq!(round(150, -2), 200);
        assert_eq!(round(-150, -2), -200);
        assert_eq!(round(-250, -2), -200);
        assert_eq!(round(i64::MAX, -40), 0);
        assert_eq!(round(4_999_999_999_999_999_999, -19), 0);
    }
}
