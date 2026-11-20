// SPDX-License-Identifier: GPL-3.0-or-later
//! Random draws for code that takes its source as an argument: the
//! host's `random` ([`Host`]), or a sequence of its own ([`Seeded`],
//! splitmix64), seeded once from the host or by a test. Pure code that
//! draws from a `&mut impl Dice` gives the same run for a seed.

/// A source of random numbers, and the draws made from it.
pub trait Dice {
    fn next_u64(&mut self) -> u64;

    /// An integer in `0..n`; 0 when `n` is 0.
    fn below(&mut self, n: usize) -> usize {
        self.below_u64(n as u64) as usize
    }

    /// An integer in `0..n`; 0 when `n` is 0.
    fn below_u64(&mut self, n: u64) -> u64 {
        ((u128::from(self.next_u64()) * u128::from(n)) >> 64) as u64
    }

    /// `true` with the chance `p` (`0..=1`).
    fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }

    /// A number in `[0, 1)`.
    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// A number in `[a, b)`.
    fn uniform(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.unit()
    }

    /// An index of `weights`, each drawn by its weight; `None` when
    /// they add up to nothing.
    fn weighted(&mut self, weights: &[f64]) -> Option<usize> {
        let total: f64 = weights.iter().sum();
        if total.is_nan() || total <= 0.0 {
            return None;
        }
        let mut r = self.unit() * total;
        let last = weights.iter().rposition(|&w| w > 0.0)?;
        for (i, &w) in weights.iter().enumerate().take(last) {
            r -= w;
            if r < 0.0 {
                return Some(i);
            }
        }
        Some(last)
    }
}

/// The host's `random`: seeded by `--seed`, else from the system.
pub struct Host;

impl Dice for Host {
    fn next_u64(&mut self) -> u64 {
        let mut b = [0; 8];
        crate::random(&mut b);
        u64::from_le_bytes(b)
    }
}

/// splitmix64.
#[derive(Clone, Debug)]
pub struct Seeded(u64);

impl Seeded {
    pub fn new(seed: u64) -> Seeded {
        Seeded(seed)
    }
}

impl Dice for Seeded {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_in_their_range() {
        let mut d = Seeded::new(1);
        let mut seen = [0usize; 6];
        for _ in 0..6000 {
            seen[d.below(6)] += 1;
            let u = d.uniform(0.5, 1.5);
            assert!((0.5..1.5).contains(&u));
        }
        assert!(seen.iter().all(|&n| (800..1200).contains(&n)), "{seen:?}");
        assert_eq!((d.below(0), d.below_u64(0)), (0, 0));
        let big = 1u64 << 62;
        assert!((0..1000).all(|_| d.below_u64(big) < big));
        assert!((0..1000).any(|_| d.below_u64(big) >= big / 2));
        assert!(!d.chance(0.0) && d.chance(1.0));
        let n = (0..4000).filter(|_| d.chance(0.25)).count();
        assert!((800..1200).contains(&n), "{n}");
    }

    #[test]
    fn weights_are_followed() {
        let mut d = Seeded::new(2);
        assert_eq!(d.weighted(&[]), None);
        assert_eq!(d.weighted(&[0.0, 0.0]), None);
        let mut n = [0usize; 3];
        for _ in 0..4000 {
            n[d.weighted(&[1.0, 0.0, 3.0]).unwrap()] += 1;
        }
        assert_eq!(n[1], 0);
        assert!((800..1200).contains(&n[0]), "{n:?}");
    }
}
