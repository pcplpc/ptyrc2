// SPDX-License-Identifier: GPL-3.0-or-later
//! Python's `random.Random(n)`: the same seed, the same draws as the
//! twins. MT19937 seeded by `init_by_array` of the 32-bit words of `n`;
//! the draws as CPython 3.13 makes them. The twin of
//! `_shared/js/twister.js`, with `gauss`.

#![allow(dead_code)]

const N: usize = 624;

#[derive(Clone, Debug)]
pub struct Twister {
    mt: [u32; N],
    at: usize,
    gauss_next: Option<f64>,
}

impl Twister {
    pub fn new(n: u64) -> Twister {
        let mut key = vec![n as u32];
        if n >> 32 != 0 {
            key.push((n >> 32) as u32);
        }
        let mut mt = [0u32; N];
        mt[0] = 19650218;
        for i in 1..N {
            mt[i] = 1812433253u32
                .wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        let (mut i, mut j) = (1, 0);
        for _ in 0..N.max(key.len()) {
            mt[i] = (mt[i] ^ (mt[i - 1] ^ (mt[i - 1] >> 30)).wrapping_mul(1664525))
                .wrapping_add(key[j])
                .wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= N {
                mt[0] = mt[N - 1];
                i = 1;
            }
            if j >= key.len() {
                j = 0;
            }
        }
        for _ in 0..N - 1 {
            mt[i] = (mt[i] ^ (mt[i - 1] ^ (mt[i - 1] >> 30)).wrapping_mul(1566083941))
                .wrapping_sub(i as u32);
            i += 1;
            if i >= N {
                mt[0] = mt[N - 1];
                i = 1;
            }
        }
        mt[0] = 0x8000_0000;
        Twister {
            mt,
            at: N,
            gauss_next: None,
        }
    }

    /// The next word.
    pub fn word(&mut self) -> u32 {
        let mt = &mut self.mt;
        if self.at >= N {
            for k in 0..N {
                let y = (mt[k] & 0x8000_0000) | (mt[(k + 1) % N] & 0x7fff_ffff);
                mt[k] = mt[(k + 397) % N] ^ (y >> 1) ^ if y & 1 != 0 { 0x9908_b0df } else { 0 };
            }
            self.at = 0;
        }
        let mut y = mt[self.at];
        self.at += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^ (y >> 18)
    }

    /// `random()`: [0, 1), 53 bits.
    pub fn random(&mut self) -> f64 {
        let a = u64::from(self.word() >> 5);
        let b = u64::from(self.word() >> 6);
        (a * 67108864 + b) as f64 / 9007199254740992.0
    }

    /// `getrandbits(k)`, `k` 1 to 32.
    pub fn bits(&mut self, k: u32) -> u32 {
        self.word() >> (32 - k)
    }

    /// `_randbelow(n)`: 0 to `n - 1`, `n` 1 to 2**32 - 1.
    pub fn below(&mut self, n: u32) -> u32 {
        assert!(n > 0, "below(0)");
        let k = 32 - n.leading_zeros();
        loop {
            let r = self.bits(k);
            if r < n {
                return r;
            }
        }
    }

    /// `randrange(a, b)`: `a` to `b - 1`.
    pub fn randrange(&mut self, a: i64, b: i64) -> i64 {
        assert!(b > a, "randrange({a}, {b}): empty");
        a + i64::from(self.below((b - a) as u32))
    }

    /// `randint(a, b)`: `b` included.
    pub fn randint(&mut self, a: i64, b: i64) -> i64 {
        self.randrange(a, b + 1)
    }

    pub fn uniform(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.random()
    }

    /// `choice(seq)`.
    pub fn choice<T: Copy>(&mut self, seq: &[T]) -> T {
        assert!(!seq.is_empty(), "choice of an empty sequence");
        seq[self.below(seq.len() as u32) as usize]
    }

    /// `sample(seq, k)`.
    pub fn sample<T: Copy>(&mut self, seq: &[T], k: usize) -> Vec<T> {
        let n = seq.len();
        assert!(k <= n, "sample of {k} from {n}");
        let mut setsize = 21;
        if k > 5 {
            setsize += 4usize.pow(((k * 3) as f64).log(4.0).ceil() as u32);
        }
        let mut out = Vec::with_capacity(k);
        if n <= setsize {
            let mut pool = seq.to_vec();
            for i in 0..k {
                let j = self.below((n - i) as u32) as usize;
                out.push(pool[j]);
                pool[j] = pool[n - i - 1];
            }
        } else {
            let mut picked = std::collections::HashSet::new();
            for _ in 0..k {
                let mut j = self.below(n as u32) as usize;
                while picked.contains(&j) {
                    j = self.below(n as u32) as usize;
                }
                picked.insert(j);
                out.push(seq[j]);
            }
        }
        out
    }

    /// `choices(seq, weights)[0]`: one, by weight.
    pub fn weighted<T: Copy>(&mut self, seq: &[T], weights: &[f64]) -> T {
        let mut cum = Vec::with_capacity(weights.len());
        let mut sum = 0.0;
        for w in weights {
            sum += w;
            cum.push(sum);
        }
        let x = self.random() * sum;
        // `bisect_right(cum, x, 0, n - 1)`.
        let (mut lo, mut hi) = (0, seq.len() - 1);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if x < cum[mid] {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        seq[lo]
    }

    /// `gauss(mu, sigma)`: two by two, the second kept.
    pub fn gauss(&mut self, mu: f64, sigma: f64) -> f64 {
        let z = match self.gauss_next.take() {
            Some(z) => z,
            None => {
                let x2pi = self.random() * std::f64::consts::TAU;
                let g2rad = (-2.0 * (1.0 - self.random()).ln()).sqrt();
                self.gauss_next = Some(x2pi.sin() * g2rad);
                x2pi.cos() * g2rad
            }
        };
        mu + z * sigma
    }
}

#[cfg(test)]
mod tests {
    use super::Twister;

    /// Draws of CPython 3.13, as `twister.tests.js`.
    #[test]
    fn draws() {
        let mut r = Twister::new(3);
        let range: Vec<i64> = (0..3).map(|_| r.randrange(10, 30)).collect();
        assert_eq!(range, [17, 28, 27]);
        assert_eq!(r.randint(20, 50), 24);
        assert_eq!(r.choice(&['a', 'b', 'c', 'd', 'e', 'f', 'g']), 'c');
        assert_eq!(r.sample(&[true, false], 2), [false, true]);
        assert_eq!(r.weighted(&['a', 'b', 'c'], &[5.0, 1.0, 3.0]), 'b');
        assert_eq!(r.random(), 0.9088184001853248);
        let mut r = Twister::new((1 << 32) + 7);
        let all: Vec<u32> = (0..100).collect();
        assert_eq!(r.sample(&all, 8), [28, 97, 45, 43, 7, 49, 83, 6]);
        assert_eq!(r.uniform(4.0, 14.0), 4.6486019194073025);
    }

    #[test]
    fn gauss_bits_and_weights() {
        let mut r = Twister::new(3);
        let g: Vec<f64> = (0..3).map(|_| r.gauss(0.0, 1.0)).collect();
        let want = [0.09470803828730423, 1.2500243810835503, -0.931378367720707];
        assert!(
            g.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-15),
            "{g:?}"
        );
        assert_eq!(r.random(), 0.625720304108054);
        let mut r = Twister::new((1 << 32) + 7);
        assert_eq!(r.bits(30), 242138325);
        assert_eq!(r.bits(32), 3287823873);
        assert_eq!(r.weighted(&[0, 1, 2, 3], &[58.0, 27.0, 11.0, 4.0]), 0);
        assert_eq!(r.randrange(900, 3200), 1155);
    }

    #[test]
    #[should_panic(expected = "empty")]
    fn an_empty_range() {
        Twister::new(1).randrange(3, 3);
    }
}
