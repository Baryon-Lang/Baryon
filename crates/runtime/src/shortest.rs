//! The fewest digits that read back as a float, chosen as Python's `repr` chooses them.
//!
//! Rust's `{:e}` gives the fewest significant digits that read back as the same value, and every
//! place the runtime writes a float as text starts from it: `print`, f-strings and printed
//! arrays. When two strings of that length both read back and lie equally far from
//! the value, Rust takes the upper one, while Python's `repr` and `NumPy` take the one whose last
//! digit is even. `1125899906842624.25` lies halfway between `...4.2` and `...4.3`: Rust writes
//! `...4.3`, Python `...4.2`. The same happens to an `f32` such as `45635.3125`, often enough
//! (about 1 in 300 random values) that printed `f32` arrays disagreed with `NumPy`.
//!
//! [`shortest`] keeps Rust's length and takes the correctly rounded digits of that length, which
//! Rust's `{:.Ne}` rounds halves to even, whenever they read back. They are then the closest
//! string of the fewest digits, with ties to even, which is exactly Python's choice. When they do
//! not read back, which happens only next to a power of two where the values below are closer
//! together, the other candidate is the only one that does, and it is Rust's.

use std::fmt::LowerExp;
use std::str::FromStr;

/// `value` in Rust's `{:e}` form, with the fewest digits that read back as `value`, a tie between
/// two such strings going to the even last digit as in Python's `repr`. `inf` and `NaN` are
/// written as `{:e}` writes them.
///
/// It formats `value` twice and parses it once, so it costs about three times `{:e}`.
pub(crate) fn shortest<T>(value: T) -> String
where
    T: Copy + LowerExp + FromStr + PartialEq,
{
    let fewest = format!("{value:e}");
    let Some((mantissa, _)) = fewest.split_once('e') else {
        return fewest;
    };
    let digits = mantissa.bytes().filter(u8::is_ascii_digit).count();
    let nearest = format!("{value:.*e}", digits.saturating_sub(1));
    if nearest != fewest && nearest.parse::<T>().is_ok_and(|read| read == value) {
        nearest
    } else {
        fewest
    }
}

#[cfg(test)]
mod tests {
    use super::shortest;

    #[test]
    fn a_tie_between_the_fewest_digits_goes_to_the_even_one() {
        // Built by arithmetic, since a literal of these values has more digits than clippy
        // allows: 2^50 + 0.25, 45635.3125 and 2425435.25, each exact.
        let large = 2.0_f64.powi(50) + 0.25;
        let single = 730_165.0_f32 / 16.0;
        let wide = 9_701_741.0_f32 / 4.0;
        assert_eq!(shortest(large), "1.1258999068426242e15");
        assert_eq!(shortest(single), "4.5635312e4");
        assert_eq!(shortest(wide), "2.4254352e6");
        assert_eq!(shortest(-single), "-4.5635312e4");
    }

    #[test]
    fn values_without_a_tie_keep_rusts_digits() {
        for value in [
            0.1,
            0.3,
            0.1 + 0.2,
            1e16,
            5e-324,
            f64::MAX,
            2.0_f64.powi(60),
            -0.0,
        ] {
            assert_eq!(shortest(value), format!("{value:e}"));
        }
        for value in [0.1_f32, 16_777_216.0, f32::MIN_POSITIVE, f32::MAX] {
            assert_eq!(shortest(value), format!("{value:e}"));
        }
        assert_eq!(shortest(f64::NAN), "NaN");
        assert_eq!(shortest(f64::NEG_INFINITY), "-inf");
    }
}
