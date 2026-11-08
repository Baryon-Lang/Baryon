//! The Philox4x64-10 random number generator behind `random.Philox` (decision 0028), giving the
//! same numbers as `numpy.random.Philox`, bit for bit.
//!
//! Philox is counter-based: block `c` of the stream of key `k` is a fixed function of `c` and
//! `k`, ten rounds of multiplying, swapping and mixing in the key (Salmon et al., "Parallel random
//! numbers: as easy as 1, 2, 3", SC 2011). So a generator needs no shared state, and stream `s`
//! of a seed can start anywhere: its counter starts at `(0, 0, s, 0)`, as `NumPy`'s
//! `Philox(key=seed).jumped(s)` does. The counter goes up by one before each block, and each
//! block gives four 64-bit outputs, used in order.

/// The multipliers of the two Philox4x64 lanes.
const MULTIPLIERS: [u64; 2] = [0xD2E7_470E_E14C_6C93, 0xCA5A_8263_9512_1157];

/// What the key's two words grow by after each round (the golden ratio and `sqrt(3) - 1`).
const WEYL: [u64; 2] = [0x9E37_79B9_7F4A_7C15, 0xBB67_AE85_84CA_A73B];

/// The number of rounds; 10 is the variant `NumPy` and Random123 use by default.
const ROUNDS: usize = 10;

/// A generator's whole state, in the order compiled code lays it out: 11 words, 88 bytes, as
/// `types::layout` gives a `Philox`. Compiled code never reads the fields; it only passes the
/// address to the runtime.
#[repr(C)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Philox {
    /// The key: the seed's low and high 64 bits.
    key: [u64; 2],
    /// The counter of the block in `outputs`.
    counter: [u64; 4],
    /// The outputs of the current block.
    outputs: [u64; 4],
    /// How many of `outputs` are used; 4 when a new block is needed.
    used: u64,
}

impl Philox {
    /// The generator for `seed` at `stream`, before its first draw.
    ///
    /// ```
    /// let mut generator = runtime::Philox::new(2026, 0);
    /// assert!((0.0..1.0).contains(&generator.uniform()));
    /// ```
    #[must_use]
    pub fn new(seed: u64, stream: u64) -> Self {
        Self {
            key: [seed, 0],
            counter: [0, 0, stream, 0],
            outputs: [0; 4],
            used: 4,
        }
    }

    /// The next 64-bit output.
    pub fn next_u64(&mut self) -> u64 {
        if self.used >= 4 {
            self.advance();
            self.outputs = block(self.counter, self.key);
            self.used = 0;
        }
        // `used` is below 4 here, so the index is in range.
        let output = self.outputs[usize::try_from(self.used).unwrap_or(0)];
        self.used += 1;
        output
    }

    /// The next float in `[0, 1)`: the top 53 bits of the next output, scaled, as `NumPy`'s
    /// `Generator.random()` makes it.
    pub fn uniform(&mut self) -> f64 {
        /// `2**-53`, the gap between floats just below 1.
        const SCALE: f64 = 1.0 / 9_007_199_254_740_992.0;
        #[allow(
            clippy::cast_precision_loss,
            reason = "a 53-bit value converts to f64 exactly"
        )]
        let top = (self.next_u64() >> 11) as f64;
        top * SCALE
    }

    /// Adds one to the 256-bit counter, carrying from the first word up.
    fn advance(&mut self) {
        for word in &mut self.counter {
            *word = word.wrapping_add(1);
            if *word != 0 {
                return;
            }
        }
    }
}

/// The four outputs of the block at `counter` for `key`: ten Philox4x64 rounds.
fn block(mut counter: [u64; 4], mut key: [u64; 2]) -> [u64; 4] {
    for _ in 0..ROUNDS {
        let (high0, low0) = multiply(MULTIPLIERS[0], counter[0]);
        let (high1, low1) = multiply(MULTIPLIERS[1], counter[2]);
        counter = [
            high1 ^ counter[1] ^ key[0],
            low1,
            high0 ^ counter[3] ^ key[1],
            low0,
        ];
        key = [key[0].wrapping_add(WEYL[0]), key[1].wrapping_add(WEYL[1])];
    }
    counter
}

/// The high and low 64 bits of the 128-bit product `a * b`.
fn multiply(a: u64, b: u64) -> (u64, u64) {
    let product = u128::from(a) * u128::from(b);
    #[allow(
        clippy::cast_possible_truncation,
        reason = "each half is taken on purpose"
    )]
    ((product >> 64) as u64, product as u64)
}

#[cfg(test)]
mod tests {
    use super::Philox;

    /// The first outputs for key 2026 match `numpy.random.Philox(key=2026).random_raw(2)`.
    #[test]
    fn raw_outputs_match_numpy() {
        let mut generator = Philox::new(2026, 0);
        assert_eq!(generator.next_u64(), 7_785_156_515_201_790_007);
        assert_eq!(generator.next_u64(), 6_317_963_904_203_345_008);
    }

    /// Floats match `Generator(Philox(key=2026).jumped(s)).random(…)` for streams 0 and 5,
    /// across the end of the first block.
    #[test]
    fn uniform_matches_numpy() {
        let draw = |stream, count| {
            let mut generator = Philox::new(2026, stream);
            (0..count).map(|_| generator.uniform()).collect::<Vec<_>>()
        };
        assert_eq!(
            draw(0, 6),
            [
                0.422_034_180_346_073,
                0.342_497_509_531_113_24,
                0.111_816_061_791_389_92,
                0.956_212_079_298_963,
                0.551_728_683_867_008_5,
                0.704_031_938_356_143_3,
            ]
        );
        assert_eq!(
            draw(5, 3),
            [
                0.145_247_462_994_125_5,
                0.952_780_187_899_714_7,
                0.873_749_333_977_605
            ]
        );
    }
}
