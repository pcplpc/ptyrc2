// SPDX-License-Identifier: GPL-3.0-or-later
//! Scene: sprites, anchors, animations, effects, composition.
//!
//! The driver calls `tick` then `compose` after every screen update, and
//! `tick` alone at `next_wake`; a `compose` is then due when `take_dirty`.

mod effect;
mod raster;
mod spec;
#[cfg(test)]
mod tests;

pub use effect::{Effect, EffectFactory, Rng, confetti, sparkle};
pub use raster::Canvas;
pub use spec::*;

use crate::types::{
    FrameId, FramePos, FrameSel, OwnerId, RAW, SpriteId, SurfaceInfo, Surfaces, TextQuery,
};
use ptyrc_vt::{Attrs, Cell, Color, Grid, Rect, Style};
use raster::Slot;
use std::collections::BTreeMap;
use std::time::Duration;

/// Auto-placed sprites ask for a spot this often.
const PLACE_EVERY: Duration = Duration::from_millis(500);
/// A better spot must hold this long before the sprite moves.
const REPLACE_AFTER: Duration = Duration::from_millis(2000);
/// Step of a moving sprite.
const MOTION_STEP: Duration = Duration::from_millis(33);
/// A text anchor keeps its match when one is found this close (rows).
const TEXT_STICKY_ROWS: i64 = 3;

const FRAME_NAMES: [&str; 4] = ["raw", "active", "largest", "quietest"];

struct Anim {
    frames: Vec<Canvas>,
    ms: Vec<u64>,
    cur: usize,
    since: Duration,
    /// Plays left; `None`: forever.
    plays: Option<u32>,
    /// From `frames`: ends, and is removed, when finite.
    timed: bool,
}

struct Fx {
    name: String,
    effect: Box<dyn Effect>,
    canvas: Canvas,
    size: Option<[u16; 2]>,
    next: Duration,
}

enum Body {
    Anim(Anim),
    Fx(Fx),
}

#[derive(Default)]
struct AutoState {
    spot: Option<(u16, u16)>,
    next_check: Duration,
    /// A different spot `place` proposed, and since when.
    pending: Option<((u16, u16), Duration)>,
}

struct Mover {
    motion: Motion,
    vel: (f64, f64),
    off: (f64, f64),
    last: Duration,
}

struct Sprite {
    id: SpriteId,
    owner: OwnerId,
    sel: FrameSel,
    frame: Option<FrameId>,
    anchor: Anchor,
    body: Body,
    /// Never 0: the app's.
    layer: i32,
    /// Given by the client; otherwise the layer follows the blend.
    layer_set: bool,
    blend: Blend,
    z: i32,
    persist: bool,
    /// Drawn at least once. Until then the clocks of an animation and of
    /// an effect are held.
    shown: bool,
    born: Duration,
    expires: Option<Duration>,
    mover: Option<Mover>,
    auto: AutoState,
    /// Frame position of the match a text anchor holds.
    text_at: Option<(FramePos, u16)>,
    /// Top-left screen cell at the last tick; `None`: hidden.
    screen: Option<(i64, i64)>,
}

impl Sprite {
    fn canvas(&self) -> &Canvas {
        match &self.body {
            Body::Anim(a) => &a.frames[a.cur],
            Body::Fx(f) => &f.canvas,
        }
    }

    /// Largest extent over the frames: what placement reserves.
    fn size(&self) -> (u16, u16) {
        match &self.body {
            Body::Anim(a) => (
                a.frames.iter().map(Canvas::width).max().unwrap_or(0),
                a.frames.iter().map(Canvas::height).max().unwrap_or(0),
            ),
            Body::Fx(f) => (f.canvas.width(), f.canvas.height()),
        }
    }

    /// Top-left screen cell, from the anchor state held and the surface.
    fn locate(&self, info: &SurfaceInfo, cursor: (u16, u16)) -> Option<(i64, i64)> {
        let (w, h) = self.size();
        let (w, h) = (w as i64, h as i64);
        let rect = info.rect;
        let of_frame = |x: i64, y: i64| {
            (
                rect.x as i64 + x - info.origin.x,
                rect.y as i64 + y - info.origin.y,
            )
        };
        let of_view = |x: i64, y: i64| (rect.x as i64 + x, rect.y as i64 + y);
        let (x, y) = match &self.anchor {
            Anchor::At {
                at,
                space: Space::Frame,
            } => of_frame(at[0], at[1]),
            Anchor::At {
                at,
                space: Space::View,
            } => {
                let x = if at[0] < 0 {
                    rect.w as i64 - w + at[0] + 1
                } else {
                    at[0]
                };
                let y = if at[1] < 0 {
                    rect.h as i64 - h + at[1] + 1
                } else {
                    at[1]
                };
                of_view(x, y)
            }
            Anchor::Text { side, gap, .. } => {
                let (pos, len) = self.text_at?;
                let len = len as i64;
                let side_gap = gap.unwrap_or(1) as i64;
                let stack_gap = gap.unwrap_or(0) as i64;
                let mid_y = pos.y - (h - 1) / 2;
                let mid_x = pos.x + (len - w) / 2;
                let (x, y) = match side {
                    Side::Right => (pos.x + len + side_gap, mid_y),
                    Side::Left => (pos.x - side_gap - w, mid_y),
                    Side::Above => (mid_x, pos.y - stack_gap - h),
                    Side::Below => (mid_x, pos.y + 1 + stack_gap),
                    Side::Over => (pos.x, pos.y),
                };
                of_frame(x, y)
            }
            Anchor::Auto { .. } => {
                let (x, y) = self.auto.spot?;
                of_view(x as i64, y as i64)
            }
            Anchor::Cursor { cursor: c } => {
                (cursor.0 as i64 + c.dx as i64, cursor.1 as i64 + c.dy as i64)
            }
        };
        let (ox, oy) = match &self.mover {
            Some(m) => (m.off.0.round() as i64, m.off.1.round() as i64),
            None => (0, 0),
        };
        Some((x + ox, y + oy))
    }
}

/// Every sprite, and what they add to the screen.
pub struct Scene {
    /// Sorted by `(layer, z, id)`: paint order.
    sprites: Vec<Sprite>,
    next_id: SpriteId,
    effects: BTreeMap<String, EffectFactory>,
    dirty: bool,
    last_tick: Duration,
}

impl Default for Scene {
    fn default() -> Scene {
        Scene::new()
    }
}

impl Scene {
    /// An empty scene with the built-in effects.
    pub fn new() -> Scene {
        let mut scene = Scene {
            sprites: Vec::new(),
            next_id: 1,
            effects: BTreeMap::new(),
            dirty: false,
            last_tick: Duration::ZERO,
        };
        scene.register_effect("confetti", Box::new(confetti));
        scene.register_effect("sparkle", Box::new(sparkle));
        scene
    }

    pub fn register_effect(&mut self, name: &str, make: EffectFactory) {
        self.effects.insert(name.to_string(), make);
    }

    /// Names of the registered effects, sorted.
    pub fn effects(&self) -> Vec<String> {
        self.effects.keys().cloned().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.sprites.is_empty()
    }

    /// Something visible may have changed since the last `take_dirty`.
    pub fn dirty(&self) -> bool {
        self.dirty
    }

    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    fn body(
        &self,
        content: &Option<Content>,
        frames: &Option<Vec<AnimFrame>>,
        looping: LoopSpec,
    ) -> Result<Anim, SceneError> {
        match (content, frames) {
            (Some(content), None) => Ok(Anim {
                frames: vec![Canvas::from_content(content)?],
                ms: vec![0],
                cur: 0,
                since: Duration::ZERO,
                plays: None,
                timed: false,
            }),
            (None, Some(list)) if !list.is_empty() => Ok(Anim {
                frames: list
                    .iter()
                    .map(|f| Canvas::from_content(&f.content))
                    .collect::<Result<_, _>>()?,
                ms: list.iter().map(|f| f.ms.max(1)).collect(),
                cur: 0,
                since: Duration::ZERO,
                plays: looping.plays(),
                timed: true,
            }),
            (None, Some(_)) => Err(SceneError::BadParams("frames is empty".into())),
            _ => Err(SceneError::BadParams(
                "one of content, frames, effect required".into(),
            )),
        }
    }

    fn check_sel(sel: &FrameSel) -> Result<Option<FrameId>, SceneError> {
        match sel {
            FrameSel::Id(id) => Ok(Some(*id)),
            FrameSel::Name(name) if name == "raw" => Ok(Some(RAW)),
            FrameSel::Name(name) if FRAME_NAMES.contains(&name.as_str()) => Ok(None),
            FrameSel::Name(name) => Err(SceneError::NoFrame(name.clone())),
        }
    }

    fn check_anchor(anchor: &Anchor) -> Result<(), SceneError> {
        if let Anchor::Text { text, regex, .. } = anchor {
            if text.is_empty() {
                return Err(SceneError::BadParams("anchor: text is empty".into()));
            }
            if *regex && let Err(e) = regex::Regex::new(text) {
                return Err(SceneError::BadParams(format!("anchor: {e}")));
            }
        }
        Ok(())
    }

    fn check_motion(motion: &Motion) -> Result<(), SceneError> {
        if [motion.vx, motion.vy, motion.ax, motion.ay]
            .iter()
            .all(|v| v.is_finite())
        {
            Ok(())
        } else {
            Err(SceneError::BadParams("motion: not finite".into()))
        }
    }

    /// Add a sprite. A frame name other than `raw` is resolved at the next
    /// `tick`; a frame id that does not exist keeps the sprite hidden.
    pub fn add(
        &mut self,
        owner: OwnerId,
        spec: SpriteSpec,
        now: Duration,
    ) -> Result<SpriteId, SceneError> {
        let frame = Scene::check_sel(&spec.frame)?;
        Scene::check_anchor(&spec.anchor)?;
        if let Some(motion) = &spec.motion {
            Scene::check_motion(motion)?;
        }
        let body = match &spec.effect {
            Some(fx) => {
                if spec.content.is_some() || spec.frames.is_some() {
                    return Err(SceneError::BadParams(
                        "effect excludes content and frames".into(),
                    ));
                }
                let make = self
                    .effects
                    .get(&fx.name)
                    .ok_or_else(|| SceneError::BadParams(format!("unknown effect: {}", fx.name)))?;
                let effect = make(&fx.params).map_err(SceneError::BadParams)?;
                let [w, h] = spec.size.unwrap_or([0, 0]);
                Body::Fx(Fx {
                    name: fx.name.clone(),
                    effect,
                    canvas: Canvas::new(w, h),
                    size: spec.size,
                    next: now,
                })
            }
            None => {
                let mut anim = self.body(&spec.content, &spec.frames, spec.looping)?;
                anim.since = now;
                Body::Anim(anim)
            }
        };
        let id = self.next_id;
        self.next_id += 1;
        let sprite = Sprite {
            id,
            owner,
            sel: spec.frame,
            frame,
            anchor: spec.anchor,
            body,
            layer: spec.layer.map_or(spec.blend.default_layer(), |l| l.0),
            layer_set: spec.layer.is_some(),
            blend: spec.blend.canonical(),
            z: spec.z,
            persist: spec.persist,
            shown: false,
            born: now,
            expires: spec.ttl_ms.map(|ms| now + Duration::from_millis(ms)),
            mover: spec.motion.map(|motion| Mover {
                motion,
                vel: (motion.vx, motion.vy),
                off: (0.0, 0.0),
                last: now,
            }),
            auto: AutoState::default(),
            text_at: None,
            screen: None,
        };
        let at = self
            .sprites
            .partition_point(|s| (s.layer, s.z, s.id) < (sprite.layer, sprite.z, sprite.id));
        self.sprites.insert(at, sprite);
        self.dirty = true;
        Ok(id)
    }

    /// Change a sprite. Nothing is applied on error.
    pub fn update(
        &mut self,
        id: SpriteId,
        patch: SpritePatch,
        now: Duration,
    ) -> Result<(), SceneError> {
        let index = self
            .sprites
            .iter()
            .position(|s| s.id == id)
            .ok_or(SceneError::NoSprite(id))?;
        let frame = patch.frame.as_ref().map(Scene::check_sel).transpose()?;
        if let Some(anchor) = &patch.anchor {
            Scene::check_anchor(anchor)?;
        }
        if let Some(motion) = &patch.motion {
            Scene::check_motion(motion)?;
        }
        let body = if patch.content.is_some() || patch.frames.is_some() {
            let looping = patch.looping.unwrap_or_default();
            let mut anim = self.body(&patch.content, &patch.frames, looping)?;
            anim.since = now;
            Some(anim)
        } else {
            None
        };
        let sprite = &mut self.sprites[index];
        if let (Some(frame), Some(sel)) = (frame, patch.frame) {
            sprite.frame = frame;
            sprite.sel = sel;
            sprite.auto = AutoState::default();
            sprite.text_at = None;
        }
        if let Some(anchor) = patch.anchor {
            if anchor != sprite.anchor {
                sprite.auto = AutoState::default();
                sprite.text_at = None;
            }
            sprite.anchor = anchor;
        }
        match (body, &mut sprite.body) {
            (Some(anim), _) => sprite.body = Body::Anim(anim),
            (None, Body::Anim(anim)) => {
                if let Some(looping) = patch.looping {
                    anim.plays = looping.plays();
                }
            }
            _ => {}
        }
        let mut order = false;
        if let Some(blend) = patch.blend {
            sprite.blend = blend.canonical();
            if !sprite.layer_set && patch.layer.is_none() {
                order |= sprite.layer != blend.default_layer();
                sprite.layer = blend.default_layer();
            }
        }
        if let Some(layer) = patch.layer {
            order |= sprite.layer != layer.0;
            sprite.layer = layer.0;
            sprite.layer_set = true;
        }
        if let Some(persist) = patch.persist {
            sprite.persist = persist;
        }
        if let Some(ms) = patch.ttl_ms {
            sprite.expires = Some(now + Duration::from_millis(ms));
        }
        if let Some(motion) = patch.motion {
            let off = sprite.mover.as_ref().map(|m| m.off).unwrap_or((0.0, 0.0));
            sprite.mover = Some(Mover {
                motion,
                vel: (motion.vx, motion.vy),
                off,
                last: now,
            });
        }
        if let Some(z) = patch.z {
            order |= z != sprite.z;
            sprite.z = z;
        }
        if order {
            self.sprites.sort_by_key(|s| (s.layer, s.z, s.id));
        }
        self.dirty = true;
        Ok(())
    }

    pub fn remove(&mut self, id: SpriteId) -> bool {
        let before = self.sprites.len();
        self.sprites.retain(|s| s.id != id);
        self.note_removed(before) > 0
    }

    /// Remove the sprites of `owner`, or all of them. Returns the count.
    pub fn clear(&mut self, owner: Option<OwnerId>) -> usize {
        let before = self.sprites.len();
        self.sprites.retain(|s| owner.is_some_and(|o| s.owner != o));
        self.note_removed(before)
    }

    /// The owner is gone: remove its sprites but the `persist` ones.
    pub fn drop_owner(&mut self, owner: OwnerId) {
        let before = self.sprites.len();
        self.sprites.retain(|s| s.owner != owner || s.persist);
        self.note_removed(before);
    }

    /// Rows `from..=to` of `frame` moved by `dy` in frame space (a splice,
    /// an island merge): sprites fixed on those rows move with them.
    pub fn shift(&mut self, frame: FrameId, from: i64, to: i64, dy: i64) {
        let rows = from..=to;
        for sprite in self.sprites.iter_mut().filter(|s| s.frame == Some(frame)) {
            if let Anchor::At {
                at,
                space: Space::Frame,
            } = &mut sprite.anchor
                && rows.contains(&at[1])
            {
                at[1] += dy;
                self.dirty = true;
            }
            if let Some((pos, _)) = &mut sprite.text_at
                && rows.contains(&pos.y)
            {
                pos.y += dy;
                self.dirty = true;
            }
        }
    }

    fn note_removed(&mut self, before: usize) -> usize {
        let removed = before - self.sprites.len();
        self.dirty |= removed > 0;
        removed
    }

    /// Sprites in paint order.
    pub fn list(&self) -> Vec<SpriteInfo> {
        self.sprites
            .iter()
            .map(|s| {
                let (w, h) = s.size();
                SpriteInfo {
                    id: s.id,
                    owner: s.owner,
                    frame: s.frame,
                    anchor: s.anchor.clone(),
                    kind: match &s.body {
                        Body::Anim(a) if a.timed => "animation".into(),
                        Body::Anim(_) => "content".into(),
                        Body::Fx(f) => format!("effect:{}", f.name),
                    },
                    size: [w, h],
                    layer: s.layer,
                    blend: s.blend,
                    z: s.z,
                    persist: s.persist,
                    screen: s.screen.map(|(x, y)| [x, y]),
                }
            })
            .collect()
    }

    /// Advance to `now`: lifetimes, animations, effects, motion, anchors.
    pub fn tick(&mut self, now: Duration, surfaces: &dyn Surfaces) {
        self.last_tick = now;
        let cursor = surfaces.cursor();
        let mut dirty = false;
        let mut dead: Vec<SpriteId> = Vec::new();

        // Spots held by auto sprites, for `taken`.
        let mut spots: Vec<(SpriteId, FrameId, Rect)> = self
            .sprites
            .iter()
            .filter_map(|s| {
                let (x, y) = s.auto.spot?;
                let (w, h) = s.size();
                Some((s.id, s.frame?, Rect::new(x, y, w, h)))
            })
            .collect();

        for sprite in &mut self.sprites {
            if sprite.expires.is_some_and(|at| now >= at) {
                dead.push(sprite.id);
                continue;
            }
            if sprite.frame.is_none() {
                sprite.frame = surfaces.resolve(&sprite.sel);
            }
            let info = sprite.frame.and_then(|id| surfaces.info(id));

            if !sprite.shown {
                // Nothing was seen yet: the show starts when it is.
                match &mut sprite.body {
                    Body::Anim(anim) => anim.since = now,
                    Body::Fx(_) => sprite.born = now,
                }
            }

            match &mut sprite.body {
                Body::Anim(anim) if anim.timed => {
                    let mut ended = false;
                    loop {
                        let end = anim.since + Duration::from_millis(anim.ms[anim.cur]);
                        if now < end {
                            break;
                        }
                        if anim.cur + 1 == anim.frames.len() {
                            match &mut anim.plays {
                                Some(left) if *left <= 1 => {
                                    ended = true;
                                    break;
                                }
                                Some(left) => *left -= 1,
                                None => {}
                            }
                            anim.cur = 0;
                        } else {
                            anim.cur += 1;
                        }
                        anim.since = end;
                        dirty = true;
                    }
                    if ended {
                        dead.push(sprite.id);
                        continue;
                    }
                }
                Body::Anim(_) => {}
                Body::Fx(fx) => {
                    if now >= fx.next {
                        let (w, h) = match (fx.size, &info) {
                            (Some([w, h]), _) => (w, h),
                            (None, Some(info)) => (info.rect.w, info.rect.h),
                            (None, None) => (0, 0),
                        };
                        fx.canvas.resize(w, h);
                        fx.canvas.clear();
                        if !fx
                            .effect
                            .render(now.saturating_sub(sprite.born), &mut fx.canvas)
                        {
                            dead.push(sprite.id);
                            continue;
                        }
                        fx.next = now + fx.effect.period().max(Duration::from_millis(1));
                        dirty = true;
                    }
                }
            }

            if let Some(m) = &mut sprite.mover {
                let dt = now.saturating_sub(m.last).as_secs_f64();
                m.last = now;
                m.vel.0 += m.motion.ax * dt;
                m.vel.1 += m.motion.ay * dt;
                m.off.0 += m.vel.0 * dt;
                m.off.1 += m.vel.1 * dt;
            }

            match &sprite.anchor {
                Anchor::Text {
                    text,
                    regex,
                    ignore_case,
                    nth,
                    ..
                } => {
                    sprite.text_at = sprite.frame.and_then(|id| {
                        let query = TextQuery {
                            pattern: text.clone(),
                            regex: *regex,
                            ignore_case: *ignore_case,
                        };
                        let matches = surfaces.find(id, &query, true);
                        let near = sprite.text_at.and_then(|(old, _)| {
                            matches
                                .iter()
                                .filter(|m| (m.y - old.y).abs() <= TEXT_STICKY_ROWS)
                                .min_by_key(|m| ((m.y - old.y).abs(), (m.x - old.x).abs()))
                        });
                        near.or_else(|| matches.get(*nth))
                            .map(|m| (FramePos { x: m.x, y: m.y }, m.len))
                    });
                }
                Anchor::Auto { auto } => {
                    if let Some(info) = info.as_ref().filter(|i| i.usable) {
                        let (w, h) = sprite.size();
                        let fits =
                            |(x, y): (u16, u16)| x + w <= info.rect.w && y + h <= info.rect.h;
                        let state = &mut sprite.auto;
                        if state.spot.is_some_and(|s| !fits(s)) {
                            state.spot = None;
                        }
                        if state.spot.is_none() || now >= state.next_check {
                            state.next_check = now + PLACE_EVERY;
                            let taken: Vec<Rect> = spots
                                .iter()
                                .filter(|(id, frame, _)| *id != sprite.id && *frame == info.id)
                                .map(|(_, _, r)| *r)
                                .collect();
                            let found = surfaces
                                .place(info.id, w, h, auto.prefer, &taken)
                                .filter(|s| fits(*s));
                            match (state.spot, found) {
                                (None, found) => state.spot = found,
                                (Some(spot), Some(new)) if new != spot => match state.pending {
                                    Some((cand, since)) if cand == new => {
                                        if now.saturating_sub(since) >= REPLACE_AFTER {
                                            state.spot = Some(new);
                                            state.pending = None;
                                        }
                                    }
                                    _ => state.pending = Some((new, now)),
                                },
                                _ => state.pending = None,
                            }
                            spots.retain(|(id, _, _)| *id != sprite.id);
                            if let Some((x, y)) = state.spot {
                                spots.push((sprite.id, info.id, Rect::new(x, y, w, h)));
                            }
                        }
                    }
                }
                _ => {}
            }

            let mut screen = info
                .as_ref()
                .filter(|i| i.usable)
                .and_then(|i| sprite.locate(i, cursor));

            // Bounce on the edges of the view.
            if let (Some((x, y)), Some(info), Some(m)) = (screen, &info, &mut sprite.mover)
                && m.motion.bounce
            {
                let (w, h) = match &sprite.body {
                    Body::Anim(a) => (a.frames[a.cur].width(), a.frames[a.cur].height()),
                    Body::Fx(f) => (f.canvas.width(), f.canvas.height()),
                };
                let r = info.rect;
                let (mut nx, mut ny) = (x, y);
                if w <= r.w {
                    if x < r.x as i64 {
                        nx = r.x as i64;
                        m.vel.0 = m.vel.0.abs();
                    } else if x + w as i64 > r.right() as i64 {
                        nx = r.right() as i64 - w as i64;
                        m.vel.0 = -m.vel.0.abs();
                    }
                }
                if h <= r.h {
                    if y < r.y as i64 {
                        ny = r.y as i64;
                        m.vel.1 = m.vel.1.abs();
                    } else if y + h as i64 > r.bottom() as i64 {
                        ny = r.bottom() as i64 - h as i64;
                        m.vel.1 = -m.vel.1.abs();
                    }
                }
                m.off.0 += (nx - x) as f64;
                m.off.1 += (ny - y) as f64;
                screen = Some((nx, ny));
            }

            sprite.shown |= screen.is_some();
            if screen != sprite.screen {
                sprite.screen = screen;
                dirty = true;
            }
        }

        if !dead.is_empty() {
            self.sprites.retain(|s| !dead.contains(&s.id));
            dirty = true;
        }
        self.dirty |= dirty;
    }

    /// When `tick` is next due; `None`: nothing changes by itself.
    pub fn next_wake(&self) -> Option<Duration> {
        let mut wake: Option<Duration> = None;
        let mut at = |t: Duration| wake = Some(wake.map_or(t, |w| w.min(t)));
        for s in &self.sprites {
            if let Some(t) = s.expires {
                at(t);
            }
            match &s.body {
                Body::Anim(a) if a.timed && s.shown => {
                    at(a.since + Duration::from_millis(a.ms[a.cur]))
                }
                Body::Anim(_) => {}
                Body::Fx(f) => at(f.next),
            }
            if let Some(m) = &s.mover
                && (m.vel != (0.0, 0.0) || m.motion.ax != 0.0 || m.motion.ay != 0.0)
            {
                at(self.last_tick.max(m.last) + MOTION_STEP);
            }
            if matches!(s.anchor, Anchor::Auto { .. }) {
                at(s.auto.next_check.max(self.last_tick));
            }
            if s.frame.is_none() {
                at(self.last_tick + PLACE_EVERY);
            }
        }
        wake
    }

    /// The app grid with the sprites on it: the back layers seen through
    /// the cells the app left without ink, the app, the front layers.
    /// Uses the anchor state of the last `tick` and the surfaces as they
    /// are now: a scroll between the two moves frame-anchored sprites.
    pub fn compose(&self, base: &Grid, surfaces: &dyn Surfaces) -> Grid {
        let mut out = base.clone();
        if self.sprites.is_empty() {
            return out;
        }
        let cursor = surfaces.cursor();
        let front = self.sprites.partition_point(|s| s.layer < 0);
        let mut back = Back::new(base);
        for sprite in &self.sprites[..front] {
            let tint = sprite.blend == Blend::Tint;
            each_glyph(sprite, base, surfaces, cursor, |x, y, glyph| {
                back.paint(x, y, glyph, tint)
            });
        }
        back.resolve(base, &mut out);
        for sprite in &self.sprites[front..] {
            let tint = sprite.blend == Blend::Tint;
            each_glyph(sprite, base, surfaces, cursor, |x, y, glyph| {
                paint(&mut out, x, y, glyph, tint)
            });
        }
        out
    }
}

/// Call `f` with every glyph of `sprite` that is inside its view, at its
/// screen cell.
fn each_glyph(
    sprite: &Sprite,
    base: &Grid,
    surfaces: &dyn Surfaces,
    cursor: (u16, u16),
    mut f: impl FnMut(u16, u16, &raster::Glyph),
) {
    let Some(info) = sprite.frame.and_then(|id| surfaces.info(id)) else {
        return;
    };
    if !info.usable {
        return;
    }
    let Some((left, top)) = sprite.locate(&info, cursor) else {
        return;
    };
    let clip = info.rect.intersect(&base.rect());
    if clip.is_empty() {
        return;
    }
    let canvas = sprite.canvas();
    for cy in 0..canvas.height() {
        let sy = top + cy as i64;
        if sy < clip.y as i64 || sy >= clip.bottom() as i64 {
            continue;
        }
        for cx in 0..canvas.width() {
            let Slot::Glyph(glyph) = canvas.slot(cx, cy) else {
                continue;
            };
            let sx = left + cx as i64;
            let width = glyph.cell.width as i64;
            if sx < clip.x as i64 || sx + width > clip.right() as i64 {
                continue;
            }
            f(sx as u16, sy as u16, glyph);
        }
    }
}

/// What the back layers put in one cell.
#[derive(Clone, Default)]
struct BackCell {
    /// Text and width of the glyph; `None`: a colour only.
    glyph: Option<Cell>,
    /// Right half of the wide glyph before it.
    spacer: bool,
    fg: Option<Color>,
    bg: Option<Color>,
    attrs: Attrs,
}

/// The back layers, flattened. Allocated at the first glyph.
struct Back {
    cols: u16,
    rows: u16,
    cells: Vec<Option<BackCell>>,
}

impl Back {
    fn new(base: &Grid) -> Back {
        Back {
            cols: base.cols(),
            rows: base.rows(),
            cells: Vec::new(),
        }
    }

    fn at(&mut self, x: u16, y: u16) -> &mut BackCell {
        if self.cells.is_empty() {
            self.cells = vec![None; self.cols as usize * self.rows as usize];
        }
        self.cells[y as usize * self.cols as usize + x as usize].get_or_insert_default()
    }

    /// Take the glyph out of `(x, y)` and out of its other half; the
    /// colour stays.
    fn cut(&mut self, x: u16, y: u16) {
        let cell = self.at(x, y);
        let other = if cell.spacer && x > 0 {
            Some(x - 1)
        } else if cell.glyph.as_ref().is_some_and(|g| g.width == 2) && x + 1 < self.cols {
            Some(x + 1)
        } else {
            None
        };
        for x in other.into_iter().chain([x]) {
            let cell = self.at(x, y);
            cell.glyph = None;
            cell.spacer = false;
            cell.fg = None;
            cell.attrs = Attrs::default();
        }
    }

    fn paint(&mut self, x: u16, y: u16, glyph: &raster::Glyph, tint: bool) {
        let wide = glyph.cell.width == 2;
        if tint {
            // Behind the app a tint is a colour of background.
            for x in x..=x + wide as u16 {
                let cell = self.at(x, y);
                cell.bg = glyph.bg.or(cell.bg);
            }
            return;
        }
        self.cut(x, y);
        let cell = self.at(x, y);
        cell.glyph = Some(glyph.cell.clone());
        cell.fg = glyph.fg;
        cell.bg = glyph.bg.or(cell.bg);
        cell.attrs = glyph.attrs;
        if wide {
            self.cut(x + 1, y);
            let cell = self.at(x + 1, y);
            cell.spacer = true;
            cell.bg = glyph.bg.or(cell.bg);
        }
    }

    /// Put the app on top: `out` is `base` with the back layers seen
    /// through it.
    ///
    /// - A back glyph shows where the app cell has no ink (both cells of
    ///   a wide glyph).
    /// - A back colour shows where the app's backdrop is the default
    ///   one. Another backdrop (a panel, a bar, inverse video) is the
    ///   app's: it stays, and a back glyph is seen on it.
    /// - An app cell with ink keeps its glyph and its style; only its
    ///   default background gives way to a back colour. A wide glyph
    ///   takes the colour behind its head.
    fn resolve(&self, base: &Grid, out: &mut Grid) {
        if self.cells.is_empty() {
            return;
        }
        let no_ink = |x: u16, y: u16| !base.cell(x, y).has_ink();
        for y in 0..self.rows {
            for x in 0..self.cols {
                let i = y as usize * self.cols as usize + x as usize;
                let Some(back) = &self.cells[i] else {
                    continue;
                };
                // Style of the back glyph `ink` seen in the cell `(x, y)`.
                let seen = |x: u16, ink: &BackCell| {
                    let app = base.cell(x, y);
                    if is_open(app) {
                        let own = &self.cells[y as usize * self.cols as usize + x as usize];
                        Style {
                            fg: ink.fg.unwrap_or(app.style.fg),
                            bg: own.as_ref().and_then(|c| c.bg).unwrap_or(Color::Default),
                            attrs: ink.attrs,
                            ..Style::default()
                        }
                    } else {
                        on_backdrop(app.style, ink.fg, None, ink.attrs)
                    }
                };
                // A wide glyph needs two cells without ink, and one
                // style: both halves on the same backdrop.
                let glyph = back.glyph.as_ref().filter(|g| {
                    no_ink(x, y)
                        && (g.width != 2
                            || (x + 1 < self.cols
                                && no_ink(x + 1, y)
                                && seen(x, back) == seen(x + 1, back)))
                });
                match glyph {
                    Some(g) => {
                        out.put(
                            x,
                            y,
                            Cell {
                                text: g.text.clone(),
                                width: g.width,
                                style: seen(x, back),
                            },
                        );
                    }
                    // The half of a wide glyph just put: `Grid::put`
                    // made it.
                    None if back.spacer && out.cell(x, y).width == 0 && no_ink(x, y) => {}
                    // A colour only. A wide glyph of the app has one
                    // style: its head decides for both halves.
                    None => {
                        let app = base.cell(x, y);
                        if let (true, Some(bg)) = (is_open(app) && app.width != 0, back.bg) {
                            out.cell_mut(x, y).style.bg = bg;
                            if app.width == 2 && x + 1 < self.cols {
                                out.cell_mut(x + 1, y).style.bg = bg;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The app left the background of this cell to whoever is behind: the
/// default colour, not inverse.
fn is_open(app: &Cell) -> bool {
    app.style.bg == Color::Default && !app.style.attrs.contains(Attrs::INVERSE)
}

/// Style of a glyph drawn on a cell of style `below`, whose colours it
/// inherits where it sets none.
fn on_backdrop(below: Style, fg: Option<Color>, bg: Option<Color>, attrs: Attrs) -> Style {
    if bg.is_none() && below.attrs.contains(Attrs::INVERSE) {
        // The background seen is the foreground of the cell below: stay
        // inverse, colours swapped.
        let mut attrs = attrs;
        attrs.insert(Attrs::INVERSE);
        Style {
            fg: below.fg,
            bg: fg.unwrap_or(below.bg),
            attrs,
            ..Style::default()
        }
    } else {
        Style {
            fg: fg.unwrap_or(below.fg),
            bg: bg.unwrap_or(below.bg),
            attrs,
            ..Style::default()
        }
    }
}

/// A front layer: over whatever is there.
fn paint(out: &mut Grid, x: u16, y: u16, glyph: &raster::Glyph, tint_only: bool) {
    if tint_only {
        // A wide glyph below has one style: tinted from its head.
        for x in x..x + glyph.cell.width as u16 {
            match out.cell(x, y).width {
                0 => {}
                2 if x + 1 < out.cols() => {
                    tint(&mut out.cell_mut(x, y).style, glyph);
                    tint(&mut out.cell_mut(x + 1, y).style, glyph);
                }
                _ => tint(&mut out.cell_mut(x, y).style, glyph),
            }
        }
        return;
    }
    let style = on_backdrop(out.cell(x, y).style, glyph.fg, glyph.bg, glyph.attrs);
    out.put(
        x,
        y,
        Cell {
            text: glyph.cell.text.clone(),
            width: glyph.cell.width,
            style,
        },
    );
}

fn tint(style: &mut Style, glyph: &raster::Glyph) {
    if let Some(fg) = glyph.fg {
        style.fg = fg;
    }
    if let Some(bg) = glyph.bg {
        style.bg = bg;
    }
    style.attrs.insert(glyph.attrs.0);
}
