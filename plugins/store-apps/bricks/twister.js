// SPDX-License-Identifier: GPL-3.0-or-later
// Python's `random.Random(n)`: the same seed, the same draws as the
// twins. MT19937 seeded by `init_by_array` of the 32-bit words of `n`;
// the draws as CPython 3.13 makes them.

export class Twister {
  /** `n`: a whole number, 0 to 2**53. */
  constructor(n) {
    const key = [];
    do {
      key.push(n % 0x100000000);
      n = Math.floor(n / 0x100000000);
    } while (n > 0);
    const mt = new Uint32Array(624);
    mt[0] = 19650218;
    for (let i = 1; i < 624; i++) mt[i] = Math.imul(1812433253, mt[i - 1] ^ (mt[i - 1] >>> 30)) + i;
    let i = 1;
    let j = 0;
    for (let k = Math.max(624, key.length); k; k--) {
      mt[i] = (mt[i] ^ Math.imul(mt[i - 1] ^ (mt[i - 1] >>> 30), 1664525)) + key[j] + j;
      i++;
      j++;
      if (i >= 624) {
        mt[0] = mt[623];
        i = 1;
      }
      if (j >= key.length) j = 0;
    }
    for (let k = 623; k; k--) {
      mt[i] = (mt[i] ^ Math.imul(mt[i - 1] ^ (mt[i - 1] >>> 30), 1566083941)) - i;
      i++;
      if (i >= 624) {
        mt[0] = mt[623];
        i = 1;
      }
    }
    mt[0] = 0x80000000;
    this.mt = mt;
    this.at = 624;
  }

  /** The next word, 0 to 2**32 - 1. */
  word() {
    const mt = this.mt;
    if (this.at >= 624) {
      for (let k = 0; k < 624; k++) {
        const y = (mt[k] & 0x80000000) | (mt[(k + 1) % 624] & 0x7fffffff);
        mt[k] = mt[(k + 397) % 624] ^ (y >>> 1) ^ (y & 1 ? 0x9908b0df : 0);
      }
      this.at = 0;
    }
    let y = mt[this.at++];
    y ^= y >>> 11;
    y ^= (y << 7) & 0x9d2c5680;
    y ^= (y << 15) & 0xefc60000;
    return (y ^ (y >>> 18)) >>> 0;
  }

  /** `random()`: [0, 1), 53 bits. */
  random() {
    const a = this.word() >>> 5;
    const b = this.word() >>> 6;
    return (a * 67108864 + b) / 9007199254740992;
  }

  /** `getrandbits(k)`, `k` 1 to 32. */
  bits(k) {
    return this.word() >>> (32 - k);
  }

  /** `_randbelow(n)`: 0 to `n - 1`, `n` 1 to 2**32 - 1. */
  below(n) {
    const k = 32 - Math.clz32(n);
    for (;;) {
      const r = this.bits(k);
      if (r < n) return r;
    }
  }

  /** `randrange(a, b)`, or `randrange(a)`: from 0. */
  randrange(a, b) {
    if (b === undefined) [a, b] = [0, a];
    if (b <= a) throw new RangeError(`randrange(${a}, ${b}): empty`);
    return a + this.below(b - a);
  }

  /** `randint(a, b)`: `b` included. */
  randint(a, b) {
    return this.randrange(a, b + 1);
  }

  uniform(a, b) {
    return a + (b - a) * this.random();
  }

  /** `choice(seq)`. */
  choice(seq) {
    if (!seq.length) throw new RangeError("choice of an empty sequence");
    return seq[this.below(seq.length)];
  }

  /** `sample(seq, k)`. */
  sample(seq, k) {
    const n = seq.length;
    if (k < 0 || k > n) throw new RangeError(`sample of ${k} from ${n}`);
    const out = new Array(k);
    let setsize = 21;
    if (k > 5) setsize += 4 ** Math.ceil(Math.log(k * 3) / Math.log(4));
    if (n <= setsize) {
      const pool = [...seq];
      for (let i = 0; i < k; i++) {
        const j = this.below(n - i);
        out[i] = pool[j];
        pool[j] = pool[n - i - 1];
      }
    } else {
      const picked = new Set();
      for (let i = 0; i < k; i++) {
        let j = this.below(n);
        while (picked.has(j)) j = this.below(n);
        picked.add(j);
        out[i] = seq[j];
      }
    }
    return out;
  }

  /** `choices(seq, weights)[0]`: one, by weight. */
  weighted(seq, weights) {
    const cum = [];
    let sum = 0;
    for (const w of weights) cum.push((sum += w));
    const x = this.random() * sum;
    // `bisect_right(cum, x, 0, n - 1)`.
    let lo = 0;
    let hi = seq.length - 1;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (x < cum[mid]) hi = mid;
      else lo = mid + 1;
    }
    return seq[lo];
  }
}
