//! A small seeded PRNG (SplitMix64), so runs are repeatable without a
//! dependency.

#[derive(Debug, Clone)]
pub(crate) struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix(self.0)
    }

    /// Uniform in (0, 1].
    pub fn unit(&mut self) -> f64 {
        ((self.next_u64() >> 11) + 1) as f64 / (1u64 << 53) as f64
    }

    /// Exponentially distributed with this mean.
    pub fn exp(&mut self, mean: f64) -> f64 {
        -mean * self.unit().ln()
    }
}

/// SplitMix64's finaliser: spreads any input over all 64 bits.
pub(crate) fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeatable_and_roughly_exponential() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        assert_eq!(a.next_u64(), b.next_u64());
        let n = 100_000;
        let mean = (0..n).map(|_| a.exp(10.0)).sum::<f64>() / n as f64;
        assert!((mean - 10.0).abs() < 0.2, "mean {mean}");
        assert!((0..n).all(|_| {
            let u = a.unit();
            u > 0.0 && u <= 1.0
        }));
    }
}
