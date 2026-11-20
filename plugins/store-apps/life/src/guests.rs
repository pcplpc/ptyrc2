// SPDX-License-Identifier: GPL-3.0-or-later
//! Patterns of the library dropped where the world has room (`-g`;
//! the twin's `Host`).
//!
//! The world is mapped in blocks (`room`); a pattern goes to the middle
//! of a hole, the widest holes first, the wider the hole the larger
//! the pattern. A ship is turned to the way with the most room ahead.
//! A pattern dropped is not dropped again soon. A kind that fills the
//! world waits for it to be calm. A pattern too large for any hole
//! replaces the cells of a place that has few; so does a small one when
//! no hole is left.

use crate::dice::Dice;
use crate::patterns::{Art, Kind, LIBRARY, Pattern, heading, turn};
use crate::world::{BLOCK, World, room};
use std::collections::VecDeque;

/// Patterns a second dropped in an empty world.
pub const RATE: f64 = 3.0;
/// Times fewer in a world with no hole.
pub const CROWD: f64 = 4.0;
/// Cells kept empty around a pattern dropped.
pub const MARGIN: usize = 2;
/// Blocks from any cell: a block of a hole.
pub const VOID: u32 = 3;
/// Places tried for a pattern.
pub const SITES: usize = 8;
/// Patterns tried at a place.
pub const PICKS: usize = 4;
/// Blocks a ship looks ahead.
pub const AHEAD: usize = 16;
/// Patterns remembered, not to drop one again soon.
pub const RECENT: usize = 40;
/// The weight of one remembered.
pub const STALE: f64 = 0.1;
/// Blocks 2 away from any cell for one of the map: a calm world.
pub const CALM: f64 = 4.0;
/// Patterns dropped at once in a new world, at most.
pub const OPENING: usize = 24;
/// Cells a side, at most, of a pattern dropped in a full world.
pub const SMALL: usize = 8;
/// Cells of a place for one alive, at least, to drop over them.
pub const SPARSE: usize = 16;

/// How often a kind is dropped, of 100.
pub fn share(kind: Kind) -> f64 {
    match kind {
        Kind::Ship => 30.0,
        Kind::Seed => 22.0,
        Kind::Oscillator => 26.0,
        Kind::Puffer => 7.0,
        Kind::Gun => 5.0,
        Kind::Still => 6.0,
        Kind::Filler => 4.0,
    }
}

/// The kinds that fill the world: held back until it is calm.
pub fn lively(kind: Kind) -> bool {
    matches!(kind, Kind::Seed | Kind::Puffer | Kind::Gun | Kind::Filler)
}

/// A pattern dropped: its name, the cell of its middle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Landing {
    pub name: &'static str,
    pub x: usize,
    pub y: usize,
}

struct Guest {
    p: &'static Pattern,
    art: Art,
    /// The share of its kind split between its like.
    share: f64,
}

impl Guest {
    fn side(&self) -> usize {
        self.art.len().max(self.art[0].len())
    }
}

/// `void`: how empty the world was at the last drop, 0 to 1; 1 when
/// half of its blocks were `VOID` away from any cell. `calm`: how
/// settled, 0 in a soup, 1 in its ashes. `recent`: the names of the
/// last ones dropped.
pub struct Guests<D: Dice> {
    pub dice: D,
    pub void: f64,
    pub calm: f64,
    pub recent: VecDeque<&'static str>,
    keep: usize,
    guests: Vec<Guest>,
}

/// The cells of the box `(x, y, w, h)`.
type Box4 = (i64, i64, usize, usize);

impl<D: Dice> Guests<D> {
    pub fn new(dice: D, kinds: &[Kind]) -> Guests<D> {
        let of = |k: Kind| LIBRARY.iter().filter(|p| p.kind == k).count() as f64;
        let guests: Vec<Guest> = (LIBRARY.iter().filter(|p| kinds.contains(&p.kind)))
            .map(|p| Guest {
                p,
                art: p.art(),
                share: share(p.kind) / of(p.kind),
            })
            .collect();
        let keep = RECENT.min(guests.len() / 2);
        Guests {
            dice,
            void: 0.0,
            calm: 0.0,
            recent: VecDeque::new(),
            keep,
            guests,
        }
    }

    /// Seconds to the next drop: `1 / rate` in an empty world, `CROWD`
    /// times that in a full one, half as much more or less.
    pub fn wait(&mut self, rate: f64) -> f64 {
        if rate <= 0.0 {
            return f64::INFINITY;
        }
        let crowd = 1.0 + (CROWD - 1.0) * (1.0 - self.void);
        self.dice.uniform(0.5, 1.5) * crowd / rate
    }

    /// Drops patterns in a new world, up to `OPENING`, until half of it
    /// is no hole any more. How many.
    pub fn open(&mut self, world: &mut World, open: Option<&[u64]>, seen: Option<&[u64]>) -> usize {
        let mut n = 0;
        while n < OPENING && self.drop(world, open, seen).is_some() && self.void > 0.5 {
            n += 1;
        }
        n
    }

    /// The chance of `p` now: less when dropped of late, and for a
    /// lively one when the world is not calm.
    pub fn weight(&self, p: &Pattern, share: f64) -> f64 {
        let mut share = share;
        if self.recent.contains(&p.name) {
            share *= STALE;
        }
        if lively(p.kind) {
            share *= self.calm * self.calm;
        }
        share
    }

    /// Puts a pattern in a hole of the world, `MARGIN` cells empty
    /// around it. `open`: the cells where a pattern may go. `seen`:
    /// those where it had better go; elsewhere when they have no room.
    /// With no room at all, a small pattern over the fewest cells.
    /// `None`: no place found, the world as it was.
    pub fn drop(
        &mut self,
        world: &mut World,
        open: Option<&[u64]>,
        seen: Option<&[u64]>,
    ) -> Option<Landing> {
        if world.width() == 0 || world.height() == 0 || self.guests.is_empty() {
            return None;
        }
        let best: Option<Vec<u64>> = seen.filter(|s| Some(*s) != open).map(|s| match open {
            Some(o) => s.iter().zip(o).map(|(a, b)| a & b).collect(),
            None => s.to_vec(),
        });
        let masks: Vec<Option<&[u64]>> = match &best {
            Some(b) => vec![Some(b), open],
            None => vec![open],
        };
        for (n, shut) in masks.into_iter().enumerate() {
            let far = room(world, shut);
            if n == 0 {
                let blocks = (far.len() * far[0].len()) as f64;
                let near = far.iter().flatten().filter(|d| **d >= 2).count() as f64;
                let holes = far.iter().flatten().filter(|d| **d >= VOID).count() as f64;
                self.calm = (CALM * near / blocks).min(1.0);
                self.void = (2.0 * holes / blocks).min(1.0);
            }
            if let Some(landing) = self.settle(world, &far, shut) {
                return Some(landing);
            }
        }
        let small: Vec<(usize, f64)> = (self.guests.iter().enumerate())
            .filter(|(_, g)| g.side() <= SMALL && matches!(g.p.kind, Kind::Ship | Kind::Seed))
            .map(|(i, g)| (i, self.weight(g.p, g.share)))
            .collect();
        let (w, h) = (world.width(), world.height());
        let cells: Vec<(i64, i64)> = (0..SITES)
            .map(|_| (self.dice.below(w) as i64, self.dice.below(h) as i64))
            .collect();
        self.squeeze(world, open, &small, &cells, 1)
    }

    /// A pattern in the middle of a hole of the map `far`.
    fn settle(
        &mut self,
        world: &mut World,
        far: &[Vec<u32>],
        open: Option<&[u64]>,
    ) -> Option<Landing> {
        let all: Vec<(usize, usize, u32)> = (far.iter().enumerate())
            .flat_map(|(by, row)| row.iter().enumerate().map(move |(bx, &d)| (bx, by, d)))
            .filter(|s| s.2 > 0)
            .collect();
        if all.is_empty() {
            return None;
        }
        let cubes: Vec<f64> = all.iter().map(|s| f64::from(s.2).powi(3)).collect();
        let sites: Vec<(usize, usize, u32)> = (0..SITES)
            .map(|_| all[self.dice.weighted(&cubes).expect("a site")])
            .collect();
        let weights: Vec<f64> = self
            .guests
            .iter()
            .map(|g| self.weight(g.p, g.share))
            .collect();
        // The patterns too large for the widest hole: by their share.
        let widest = (2 * far.iter().flatten().max().copied().unwrap_or(0) as usize + 1) * BLOCK
            - 2 * MARGIN;
        let large: Vec<(usize, f64)> = (0..self.guests.len())
            .filter(|&i| self.guests[i].side() > widest)
            .map(|i| (i, weights[i]))
            .collect();
        let share: f64 = large.iter().map(|l| l.1).sum();
        if self.dice.unit() * weights.iter().sum::<f64>() < share {
            let cells: Vec<(i64, i64)> = (sites.iter())
                .map(|&(bx, by, _)| {
                    (
                        (bx * BLOCK + BLOCK / 2) as i64,
                        (by * BLOCK + BLOCK / 2) as i64,
                    )
                })
                .collect();
            if let Some(landing) = self.squeeze(world, open, &large, &cells, MARGIN) {
                return Some(landing);
            }
        }
        for &(bx, by, d) in &sites {
            // Empty for sure: `2 * d - 1` blocks across. Tried: 2 more.
            let d = d as usize;
            let reach = (2 * d + 1) * BLOCK - 2 * MARGIN;
            let sure = ((2 * d - 1) * BLOCK).saturating_sub(2 * MARGIN).max(1);
            let mut fits = Vec::new();
            let mut by_fit = Vec::new();
            for (i, g) in self.guests.iter().enumerate() {
                if weights[i] > 0.0 && g.side() <= reach {
                    let fill = ((g.art.len() * g.art[0].len()) as f64).sqrt() / sure as f64;
                    fits.push(i);
                    by_fit.push(weights[i] * fill.clamp(0.1, 1.0));
                }
            }
            for _ in 0..PICKS {
                let Some(k) = self.dice.weighted(&by_fit) else {
                    break;
                };
                let i = fits[k];
                let way = self.aim(self.guests[i].p, far, bx, by);
                let art = turn(&self.guests[i].art, way);
                let (w, h) = (art[0].len(), art.len());
                let x = (bx * BLOCK) as i64 + (BLOCK as i64 - w as i64).div_euclid(2);
                let y = (by * BLOCK) as i64 + (BLOCK as i64 - h as i64).div_euclid(2);
                let m = MARGIN as i64;
                let b: Box4 = (x - m, y - m, w + 2 * MARGIN, h + 2 * MARGIN);
                if world.empty(b.0, b.1, b.2, b.3) && world.free(open, b.0, b.1, b.2, b.3) {
                    return Some(self.land(world, i, &art, x, y));
                }
            }
        }
        None
    }

    /// An orientation for `p` at the block `(bx, by)`: for a ship one
    /// with the most empty blocks ahead, up to `AHEAD`.
    pub fn aim(&mut self, p: &Pattern, far: &[Vec<u32>], bx: usize, by: usize) -> u32 {
        if p.step == (0, 0) {
            return self.dice.below(8) as u32;
        }
        let (gw, gh) = (far[0].len() as i64, far.len() as i64);
        let runs: Vec<usize> = (0..8)
            .map(|k| {
                let (dx, dy) = heading(p.step, k);
                let (dx, dy) = (i64::from(dx.signum()), i64::from(dy.signum()));
                let mut n = 0;
                while n < AHEAD {
                    let step = n as i64 + 1;
                    let (x, y) = (
                        (bx as i64 + step * dx).rem_euclid(gw),
                        (by as i64 + step * dy).rem_euclid(gh),
                    );
                    if far[y as usize][x as usize] == 0 {
                        break;
                    }
                    n += 1;
                }
                n
            })
            .collect();
        let most = runs.iter().max().copied().unwrap_or(0);
        let best: Vec<u32> = (0..8).filter(|&k| runs[k as usize] == most).collect();
        best[self.dice.below(best.len())]
    }

    /// One of `guests`, `(index, weight)`, around the one of `cells`
    /// with the fewest cells alive: they are removed, with those
    /// `margin` around. `None` when they are more than one in `SPARSE`.
    fn squeeze(
        &mut self,
        world: &mut World,
        open: Option<&[u64]>,
        guests: &[(usize, f64)],
        cells: &[(i64, i64)],
        margin: usize,
    ) -> Option<Landing> {
        let weights: Vec<f64> = guests.iter().map(|g| g.1).collect();
        let i = guests[self.dice.weighted(&weights)?].0;
        let way = self.dice.below(8) as u32;
        let art = turn(&self.guests[i].art, way);
        let (w, h) = (art[0].len() + 2 * margin, art.len() + 2 * margin);
        if w > world.width() || h > world.height() {
            return None;
        }
        let places = (cells.iter())
            .map(|&(cx, cy)| (cx - (w / 2) as i64, cy - (h / 2) as i64))
            .filter(|&(x, y)| world.free(open, x, y, w, h))
            .map(|(x, y)| (world.count(x, y, w, h), x, y));
        let (n, x, y) = places.min()?;
        if n as usize * SPARSE > w * h {
            return None;
        }
        world.put(&vec![vec![false; w]; h], x, y);
        let m = margin as i64;
        Some(self.land(world, i, &art, x + m, y + m))
    }

    fn land(&mut self, world: &mut World, i: usize, art: &Art, x: i64, y: i64) -> Landing {
        world.put(art, x, y);
        let name = self.guests[i].p.name;
        if self.keep > 0 {
            if self.recent.len() == self.keep {
                self.recent.pop_front();
            }
            self.recent.push_back(name);
        }
        let (w, h) = (world.width() as i64, world.height() as i64);
        Landing {
            name,
            x: (x + art[0].len() as i64 / 2).rem_euclid(w) as usize,
            y: (y + art.len() as i64 / 2).rem_euclid(h) as usize,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dice::Seeded;
    use crate::patterns::named;
    use std::collections::BTreeSet;

    fn block() -> Art {
        vec![vec![true; 2]; 2]
    }

    fn cells(world: &World) -> BTreeSet<(usize, usize)> {
        world.cells().into_iter().collect()
    }

    fn cells_of(name: &str) -> usize {
        named(name)
            .unwrap()
            .art()
            .iter()
            .flatten()
            .filter(|c| **c)
            .count()
    }

    fn bounds(world: &World) -> (usize, usize, usize, usize) {
        let c = world.cells();
        let xs = c.iter().map(|c| c.0);
        let ys = c.iter().map(|c| c.1);
        (
            xs.clone().min().unwrap(),
            ys.clone().min().unwrap(),
            xs.max().unwrap(),
            ys.max().unwrap(),
        )
    }

    /// `test_life_host`.
    #[test]
    fn a_pattern_goes_to_the_hole() {
        // A hole in a world of blocks: the pattern goes there, whole.
        for seed in 0..12 {
            let mut world = World::new(160, 96, 1, 1);
            for y in (0..96).step_by(8) {
                for x in (0..160).step_by(8) {
                    if !((64..120).contains(&x) && (24..72).contains(&y)) {
                        world.put(&block(), x, y);
                    }
                }
            }
            let blocks = cells(&world);
            let mut guests = Guests::new(Seeded::new(seed), &[Kind::Oscillator, Kind::Ship]);
            let landed = guests.drop(&mut world, None, None).expect("landed");
            assert_eq!(guests.recent, [landed.name]);
            let now = cells(&world);
            assert!(blocks.is_subset(&now));
            let new: Vec<_> = now.difference(&blocks).collect();
            assert_eq!(new.len(), cells_of(landed.name));
            assert!(
                new.iter()
                    .all(|&&(x, y)| (60..118).contains(&x) && (20..70).contains(&y)),
                "{landed:?}"
            );
            assert!(
                0.0 < guests.void && guests.void < 0.5 && 0.0 < guests.calm && guests.calm <= 1.0
            );
        }
        // An empty world: all of it a hole; large patterns first.
        let mut world = World::new(160, 96, 1, 1);
        let mut guests = Guests::new(Seeded::new(5), &Kind::ALL);
        assert!(guests.wait(2.0) > 1.0); // nothing known of the world: as if full
        let n = guests.open(&mut world, None, None);
        let distinct: BTreeSet<_> = guests.recent.iter().collect();
        // The drop that finds the world half full lands, uncounted.
        assert!(3 < n && n <= OPENING && guests.recent.len() == n + 1, "{n}");
        assert!(distinct.len() >= n, "{:?}", guests.recent);
        let sides = guests.recent.iter().map(|name| {
            let a = named(name).unwrap().art();
            a.len().max(a[0].len())
        });
        assert!(sides.filter(|s| *s > 12).count() * 2 > n);
        let wait = guests.wait(2.0);
        assert!((0.25..=2.0).contains(&wait) && guests.wait(0.0) == f64::INFINITY);
        guests.void = 1.0;
        assert!((0.25..=0.75).contains(&guests.wait(2.0)));
        guests.void = 0.0;
        assert!((1.0..=3.0).contains(&guests.wait(2.0)));
        // The kinds asked, no other; the same pattern not twice in a row.
        let mut guns = Guests::new(Seeded::new(1), &[Kind::Gun]);
        let mut names = BTreeSet::new();
        for _ in 0..8 {
            let mut world = World::new(200, 120, 1, 1);
            let landed = guns.drop(&mut world, None, None).expect("a gun");
            assert_eq!(named(landed.name).unwrap().kind, Kind::Gun);
            names.insert(landed.name);
        }
        assert!(names.len() > 4);
        assert_eq!(
            Guests::new(Seeded::new(1), &[]).drop(&mut World::new(60, 40, 1, 1), None, None),
            None
        );
        assert_eq!(
            Guests::new(Seeded::new(1), &Kind::ALL).drop(&mut World::new(0, 0, 1, 1), None, None),
            None
        );
    }

    /// `test_life_host_aim_and_seen`.
    #[test]
    fn a_ship_aimed_and_where_it_is_seen() {
        // A ship is sent where the way is free: blocks to the right of
        // the hole, above and under it, none to the left.
        for seed in 0..8 {
            let mut world = World::new(160, 96, 1, 1);
            for y in (0..96).step_by(4) {
                world.put(&block(), 100, y);
            }
            for x in (0..160).step_by(8) {
                world.put(&block(), x, 20);
                world.put(&block(), x, 76);
            }
            let mut guests = Guests::new(Seeded::new(seed), &[Kind::Ship]);
            let far = room(&world, None);
            for name in ["lwss", "glider", "copperhead"] {
                let p = named(name).unwrap();
                let (dx, _) = heading(p.step, guests.aim(p, &far, 22, 12));
                assert!(dx < 0, "{name} {seed}");
            }
        }
        // Where it is seen rather than under the text; under it when
        // there is no room elsewhere; never where it is closed.
        let left: Vec<u64> = [u64::MAX, 0xffff, 0].repeat(96); // the cells x < 80
        let shut = vec![0u64; 3 * 96];
        for seed in 0..8 {
            let mut world = World::new(160, 96, 1, 1);
            let landed =
                Guests::new(Seeded::new(seed), &Kind::ALL).drop(&mut world, None, Some(&left));
            assert!(landed.is_some() && bounds(&world).2 < 80);
            let mut world = World::new(160, 96, 1, 1);
            let mut guests = Guests::new(Seeded::new(seed), &Kind::ALL);
            assert!(
                guests.drop(&mut world, Some(&left), Some(&shut)).is_some()
                    && bounds(&world).2 < 80
            );
            assert_eq!(guests.drop(&mut world, Some(&shut), Some(&left)), None);
        }
    }

    /// `test_life_host_squeeze`.
    #[test]
    fn squeezed_in_when_no_hole_is_left() {
        // Sparse ashes, no hole for a gun: it takes the place of a few.
        let mut world = World::new(160, 96, 1, 1);
        for y in (0..96).step_by(16) {
            for x in (0..160).step_by(16) {
                world.put(&block(), x, y);
            }
        }
        let n = world.population() as usize;
        let mut guns = Guests::new(Seeded::new(3), &[Kind::Gun]);
        let landed = guns.drop(&mut world, None, None).expect("a gun");
        assert!(guns.calm > 0.0);
        let p = cells_of(landed.name);
        let now = world.population() as usize;
        assert!(n + p - 4 * 30 < now && now < n + p, "{n} {p} {now}");
        // No hole at all, few cells: a small one over them.
        let mut world = World::new(64, 64, 1, 1);
        for y in (0..64).step_by(4) {
            for x in (0..64).step_by(4) {
                world.put(&vec![vec![true]], x + 1, y + 1);
            }
        }
        assert_eq!(room(&world, None).iter().flatten().max(), Some(&0));
        let mut guests = Guests::new(Seeded::new(3), &Kind::ALL);
        let landed = guests.drop(&mut world, None, None).expect("a small one");
        let p = named(landed.name).unwrap();
        let a = p.art();
        assert!(a.len().max(a[0].len()) <= SMALL && matches!(p.kind, Kind::Ship | Kind::Seed));
        assert_eq!((guests.void, guests.calm), (0.0, 0.0));
        // A world not calm: the lively kinds wait.
        assert_eq!(guests.weight(named("acorn").unwrap(), 1.0), 0.0);
        assert_eq!(guests.weight(named("pulsar").unwrap(), 1.0), 1.0);
        guests.recent.push_back("pulsar");
        assert_eq!(guests.weight(named("pulsar").unwrap(), 1.0), STALE);
    }

    /// `test_life_resize_and_room`, the drops.
    #[test]
    fn no_room_no_drop() {
        let mut dice = Seeded::new(2);
        let mut world = World::new(60, 40, 1, 1);
        assert!(
            Guests::new(Seeded::new(2), &Kind::ALL)
                .drop(&mut world, None, None)
                .is_some()
        );
        assert!(world.population() >= 5);
        // No room in a full world (but a rare place of one cell alive in
        // 16, `SPARSE`), the world then as it was; none in a closed one.
        let mut none = 0;
        for seed in 0..20 {
            world.soup(&mut dice);
            let was = world.cells();
            if Guests::new(Seeded::new(seed), &Kind::ALL)
                .drop(&mut world, None, None)
                .is_none()
            {
                assert_eq!(world.cells(), was);
                none += 1;
            }
        }
        assert!(none >= 15, "{none}");
        let shut = vec![0u64; 40];
        assert_eq!(
            Guests::new(Seeded::new(2), &Kind::ALL).drop(
                &mut World::new(60, 40, 1, 1),
                Some(&shut),
                None
            ),
            None
        );
        let lwss = named("lwss").unwrap().art();
        let turns: BTreeSet<Art> = (0..60).map(|_| turn(&lwss, dice.below(8) as u32)).collect();
        assert_eq!(turns.len(), 8);
        assert!(
            turns
                .iter()
                .all(|t| t.iter().flatten().filter(|c| **c).count() == 9)
        );
    }

    #[test]
    fn the_shares() {
        assert_eq!(Kind::ALL.iter().map(|k| share(*k)).sum::<f64>(), 100.0);
        assert_eq!(RATE, 3.0);
    }
}
