# SPDX-License-Identifier: GPL-3.0-or-later
"""The twin's Tank, its draws those of reef.js (mulberry32 and its
helpers), rows after steps: to compare with twin.mjs."""
import json, math
from ptyrc_apps import fishes

def mulberry(n):
    a = n & 0xFFFFFFFF
    def imul(x, y):
        r = (x * y) & 0xFFFFFFFF
        return r
    def rnd():
        nonlocal a
        a = (a + 0x6D2B79F5) & 0xFFFFFFFF
        t = imul(a ^ (a >> 15), 1 | a)
        t = ((t + imul(t ^ (t >> 7), 61 | t)) & 0xFFFFFFFF) ^ t
        return ((t ^ (t >> 14)) & 0xFFFFFFFF) / 4294967296
    return rnd

class Reefish:
    def __init__(self, n): self.random = mulberry(n)
    def uniform(self, a, b): return a + (b - a) * self.random()
    def randrange(self, n): return math.floor(self.random() * n)
    def randint(self, a, b): return a + self.randrange(b - a + 1)
    def choice(self, seq): seq = list(seq); return seq[self.randrange(len(seq))]
    def sample(self, seq, k):
        pool, out = list(seq), []
        for _ in range(k): out.append(pool.pop(self.randrange(len(pool))))
        return out
    def choices(self, seq, weights):
        r = self.random() * sum(weights)
        for s, w in zip(seq, weights):
            r -= w
            if r < 0: return [s]
        return [seq[-1]]

NAMED = {"cyan": 6, "bright-cyan": 14, "bright-yellow": 11, "bright-magenta": 13, "bright-green": 10}
def norm(rows):
    return [[{k: NAMED.get(v, v) if k == "fg" else v for k, v in run.items()} for run in row] for row in rows]

out = []
for more in ({}, {"decorate": True, "bubbles": True, "species": True}):
    for w, h in (*fishes.TANKS, (30, 9)):
        for seed in (1, 2, 3):
            tank = fishes.Tank(w, h, Reefish(seed), **more)
            frames = []
            for i in range(80):
                if i == 30: tank.on_move(w // 2, h // 2)
                if i == 60: tank.on_leave()
                tank.step(fishes.TICK)
                if i % 10 == 9: frames.append(norm(tank.rows()))
            out.append(frames)
print(json.dumps(out, ensure_ascii=False))
