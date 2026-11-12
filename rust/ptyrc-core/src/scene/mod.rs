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

pub use effect::{Effect, EffectFactory, Looks, Rng, Under, Warmth, confetti, sparkle};
pub use raster::Canvas;
pub use spec::*;

use crate::frames::HeatView;
use crate::types::{
    FrameId, FramePos, FrameSel, OwnerId, RAW, SpriteId, SurfaceInfo, Surfaces, TextQuery,
};
use ptyrc_vt::palette::{self, Rgb};
use ptyrc_vt::{Attrs, Cell, Color, Grid, Rect, Style};
use raster::Slot;
use std::collections::BTreeMap;
use std::time::Duration;

/// Auto-placed sprites ask for a spot this often.
const PLACE_EVERY: Duration = Duration::from_millis(500);
/// A better spot must hold this long before the sprite moves.
const REPLACE_AFTER: Duration = Duration::from_millis(2000);
/// The heat shown to an effect is read again after this long.
const HEAT_EVERY: Duration = Duration::from_millis(200);
/// Step of a moving sprite.
const MOTION_STEP: Duration = Duration::from_millis(33);
/// A text anchor keeps its match when one is found this close (rows).
const TEXT_STICKY_ROWS: i64 = 3;

const FRAME_NAMES: [&str; 5] = ["raw", "active", "largest", "quietest", "idle"];

struct Anim {
    frames: Vec<Canvas>,
    ms: Vec<u64>,
    cur: usize,
    since: Duration,
    /// Plays left; `None`: forever.
    plays: Option<u32>,
    /// From `frames`: ends, and is removed, when finite.
    timed: bool,
    /// Style of the first content, for `draw.list`.
    style: Option<StyleSpec>,
}

/// The style a content is given: its `style`, else that of its first run.
fn first_style(content: &Content) -> Option<StyleSpec> {
    let run = || content.cells.iter().flatten().flatten().next();
    content.style.or_else(|| run().map(|r| r.style))
}

struct Fx {
    name: String,
    effect: Box<dyn Effect>,
    canvas: Canvas,
    size: Option<[u16; 2]>,
    next: Duration,
    /// Frame position of the canvas at the last look.
    origin: Option<(i64, i64)>,
    /// The heat given at the last look, and when it was read.
    heat: Option<(Duration, HeatView)>,
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
    /// Opacity, `0..=1`; 1: colours as given.
    alpha: f32,
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
            Anchor::Text {
                side, gap, slide, ..
            } => {
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
                let (mut x, mut y) = of_frame(x, y);
                let row = of_frame(pos.x, pos.y).1;
                let seen = row >= rect.y as i64 && row < rect.bottom() as i64;
                if *slide && seen {
                    // Along the centred axis only: never onto the match.
                    match side {
                        Side::Right | Side::Left if h <= rect.h as i64 => {
                            y = y.clamp(rect.y as i64, rect.bottom() as i64 - h);
                        }
                        Side::Above | Side::Below if w <= rect.w as i64 => {
                            x = x.clamp(rect.x as i64, rect.right() as i64 - w);
                        }
                        _ => {}
                    }
                }
                (x, y)
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
    host: HostColors,
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
            host: HostColors::default(),
        };
        scene.register_effect("confetti", Box::new(confetti));
        scene.register_effect("sparkle", Box::new(sparkle));
        scene
    }

    /// The default colours of the host: what a sprite with `alpha` is
    /// blended with where the app set no colour.
    pub fn set_host_colors(&mut self, host: HostColors) {
        if self.host != host {
            self.host = host;
            self.dirty = true;
        }
    }

    pub fn host_colors(&self) -> HostColors {
        self.host
    }

    fn check_alpha(alpha: Option<f32>) -> Result<f32, SceneError> {
        match alpha {
            None => Ok(1.0),
            Some(a) if (0.0..=1.0).contains(&a) => Ok(a),
            Some(a) => Err(SceneError::BadParams(format!("alpha: 0 to 1, not {a}"))),
        }
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
                style: first_style(content),
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
                style: first_style(&list[0].content),
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
        let alpha = Scene::check_alpha(spec.alpha)?;
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
                    origin: None,
                    heat: None,
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
            alpha,
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
        let alpha = patch
            .alpha
            .map(|a| Scene::check_alpha(Some(a)))
            .transpose()?;
        if patch.effect.is_some() && (patch.content.is_some() || patch.frames.is_some()) {
            return Err(SceneError::BadParams(
                "effect excludes content and frames".into(),
            ));
        }
        // An effect of the same name is tuned: it keeps its state.
        // Another one, or an effect in place of a content, starts.
        let mut started = None;
        if let Some(fx) = &patch.effect {
            match &mut self.sprites[index].body {
                Body::Fx(old) if old.name == fx.name => {
                    old.effect.tune(&fx.params).map_err(SceneError::BadParams)?;
                    old.next = now;
                }
                _ => {
                    let make = self.effects.get(&fx.name).ok_or_else(|| {
                        SceneError::BadParams(format!("unknown effect: {}", fx.name))
                    })?;
                    started = Some((
                        fx.name.clone(),
                        make(&fx.params).map_err(SceneError::BadParams)?,
                    ));
                }
            }
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
        if let Some((name, effect)) = started {
            let size = match &sprite.body {
                Body::Fx(old) => old.size,
                Body::Anim(_) => None,
            };
            let [w, h] = size.unwrap_or([0, 0]);
            sprite.body = Body::Fx(Fx {
                name,
                effect,
                canvas: Canvas::new(w, h),
                size,
                next: now,
                origin: None,
                heat: None,
            });
            sprite.shown = false;
            sprite.born = now;
        }
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
        if let Some(alpha) = alpha {
            sprite.alpha = alpha;
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

    /// Rows `from..=to` of `frame` are forgotten: the sprites fixed on
    /// them are removed, their ids returned; a text anchor held there
    /// loses its match.
    pub fn forget_rows(&mut self, frame: FrameId, from: i64, to: i64) -> Vec<SpriteId> {
        let rows = from..=to;
        let mut gone = Vec::new();
        for sprite in self.sprites.iter_mut().filter(|s| s.frame == Some(frame)) {
            if let Anchor::At {
                at,
                space: Space::Frame,
            } = &sprite.anchor
                && rows.contains(&at[1])
            {
                gone.push(sprite.id);
                self.dirty = true;
            }
            if let Some((pos, _)) = &sprite.text_at
                && rows.contains(&pos.y)
            {
                sprite.text_at = None;
                self.dirty = true;
            }
        }
        self.sprites.retain(|s| !gone.contains(&s.id));
        gone
    }

    /// `frame` is forgotten: remove the sprites on it; their ids.
    pub fn drop_frame(&mut self, frame: FrameId) -> Vec<SpriteId> {
        let gone: Vec<SpriteId> = self
            .sprites
            .iter()
            .filter(|s| s.frame == Some(frame))
            .map(|s| s.id)
            .collect();
        self.sprites.retain(|s| s.frame != Some(frame));
        self.dirty |= !gone.is_empty();
        gone
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
                    style: match &s.body {
                        Body::Anim(a) => a.style,
                        Body::Fx(_) => None,
                    },
                    layer: s.layer,
                    blend: s.blend,
                    alpha: s.alpha,
                    z: s.z,
                    persist: s.persist,
                    screen: s.screen.map(|(x, y)| [x, y]),
                    visible: false,
                    covered: 0.0,
                }
            })
            .collect()
    }

    /// `list`, with what shows on the app grid `base`: `visible` and
    /// `covered`, as `compose` would draw now.
    pub fn list_on(&self, base: &Grid, surfaces: &dyn Surfaces) -> Vec<SpriteInfo> {
        let cursor = surfaces.cursor();
        let mut out = self.list();
        for (sprite, info) in self.sprites.iter().zip(&mut out) {
            let (mut glyphs, mut under) = (0u32, 0u32);
            each_glyph(sprite, base, surfaces, cursor, |x, y, glyph| {
                glyphs += 1;
                let width = glyph.cell.width.max(1) as u16;
                let ink = (x..x + width).any(|x| base.cell(x, y).has_ink());
                under += ink as u32;
            });
            info.visible = glyphs > 0;
            if sprite.layer < 0 && glyphs > 0 {
                info.covered = under as f32 / glyphs as f32;
            }
        }
        out
    }

    /// Advance to `now`: lifetimes, animations, effects, motion, anchors.
    /// Effects see nothing of the app.
    pub fn tick(&mut self, now: Duration, surfaces: &dyn Surfaces) {
        self.advance(now, None, surfaces);
    }

    /// `tick`, over the app's grid `base`: effects that look are shown it.
    pub fn tick_on(&mut self, now: Duration, base: &Grid, surfaces: &dyn Surfaces) {
        self.advance(now, Some(base), surfaces);
    }

    fn advance(&mut self, now: Duration, base: Option<&Grid>, surfaces: &dyn Surfaces) {
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

            // Where an effect that looks stands: screen cell, frame position.
            let stand = match (&sprite.body, info.as_ref().filter(|i| i.usable)) {
                (Body::Fx(fx), Some(i)) if base.is_some() && fx.effect.looks() != Looks::No => {
                    sprite.locate(i, cursor).map(|(x, y)| {
                        let origin = (
                            i.origin.x + x - i.rect.x as i64,
                            i.origin.y + y - i.rect.y as i64,
                        );
                        ((x, y), origin, *i)
                    })
                }
                _ => None,
            };

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
                    // The app scrolled under it: grains fixed on the text
                    // move in the same repaint.
                    let moved = stand.is_some_and(|(_, origin, _)| fx.origin != Some(origin));
                    if now >= fx.next || moved {
                        let (w, h) = match (fx.size, &info) {
                            (Some([w, h]), _) => (w, h),
                            (None, Some(info)) => (info.rect.w, info.rect.h),
                            (None, None) => (0, 0),
                        };
                        fx.canvas.resize(w, h);
                        fx.canvas.clear();
                        if let (Some(base), Some(((x, y), origin, info))) = (base, stand) {
                            if fx.effect.looks() == Looks::Heat {
                                let fresh = fx.heat.as_ref().is_some_and(|(at, heat)| {
                                    now.saturating_sub(*at) < HEAT_EVERY
                                        && (heat.cols, heat.rows) == (info.rect.w, info.rect.h)
                                });
                                if !fresh {
                                    fx.heat = surfaces.heat(info.id, now).map(|h| (now, h));
                                }
                            }
                            let (cx, cy) = (cursor.0 as i64 - x, cursor.1 as i64 - y);
                            let inside = cx >= 0 && cy >= 0 && cx < w as i64 && cy < h as i64;
                            let mut under = Under::new(base, (x, y), info.rect)
                                .with_heat(fx.heat.as_ref().map(|(_, heat)| heat));
                            under.size = (w, h);
                            under.cursor = inside.then_some((cx as u16, cy as u16));
                            under.origin = origin;
                            fx.effect.look(&under);
                            fx.origin = Some(origin);
                        }
                        if !fx
                            .effect
                            .render(now.saturating_sub(sprite.born), &mut fx.canvas)
                        {
                            dead.push(sprite.id);
                            continue;
                        }
                        if now >= fx.next {
                            fx.next = now + fx.effect.period().max(Duration::from_millis(1));
                        }
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
        let mut back = Back::new(base, self.host);
        for sprite in &self.sprites[..front] {
            let tint = sprite.blend == Blend::Tint;
            each_glyph(sprite, base, surfaces, cursor, |x, y, glyph| {
                back.paint(x, y, glyph, tint, sprite.alpha * glyph.alpha())
            });
        }
        back.resolve(base, &mut out);
        for sprite in &self.sprites[front..] {
            let tint = sprite.blend == Blend::Tint;
            each_glyph(sprite, base, surfaces, cursor, |x, y, glyph| {
                let alpha = sprite.alpha * glyph.alpha();
                if alpha < 1.0 {
                    paint_alpha(&mut out, x, y, glyph, tint, alpha, &self.host)
                } else {
                    paint(&mut out, x, y, glyph, tint)
                }
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
    /// Opacity of `fg`, when the sprite has an `alpha`.
    fade: Option<f32>,
    /// Opaque colour.
    bg: Option<Color>,
    /// Translucent colours laid over `bg`: their sum, premultiplied, and
    /// its opacity.
    glaze: Option<([f32; 3], f32)>,
    attrs: Attrs,
}

impl BackCell {
    /// Lay the colour `bg` at opacity `alpha` over what is there.
    fn lay(&mut self, bg: Color, alpha: f32, host: &HostColors) {
        if alpha >= 1.0 {
            self.bg = Some(bg);
            self.glaze = None;
            return;
        }
        let over = palette::rgb(bg, host.bg);
        let (sum, opacity) = self.glaze.unwrap_or(([0.0; 3], 0.0));
        let rest = 1.0 - alpha;
        self.glaze = Some((
            [0, 1, 2].map(|i| over[i] as f32 * alpha + sum[i] * rest),
            alpha + opacity * rest,
        ));
    }

    /// The glaze seen on `under`.
    fn glazed(&self, under: Rgb) -> Option<Color> {
        let (sum, opacity) = self.glaze?;
        let ch = |i: usize| (under[i] as f32 * (1.0 - opacity) + sum[i]).round() as u8;
        Some(Color::Rgb(ch(0), ch(1), ch(2)))
    }
}

/// The back layers, flattened. Allocated at the first glyph.
struct Back {
    cols: u16,
    rows: u16,
    cells: Vec<Option<BackCell>>,
    host: HostColors,
}

impl Back {
    fn new(base: &Grid, host: HostColors) -> Back {
        Back {
            cols: base.cols(),
            rows: base.rows(),
            cells: Vec::new(),
            host,
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
            cell.fade = None;
            cell.attrs = Attrs::default();
        }
    }

    fn paint(&mut self, x: u16, y: u16, glyph: &raster::Glyph, tint: bool, alpha: f32) {
        let wide = glyph.cell.width == 2;
        let host = self.host;
        if !tint {
            self.cut(x, y);
            let cell = self.at(x, y);
            cell.glyph = Some(glyph.cell.clone());
            cell.fg = glyph.fg;
            cell.fade = (alpha < 1.0).then_some(alpha);
            cell.attrs = glyph.attrs;
            if wide {
                self.cut(x + 1, y);
                self.at(x + 1, y).spacer = true;
            }
        }
        // Behind the app a tint is a colour of background.
        if let Some(bg) = glyph.bg {
            for x in x..=x + wide as u16 {
                self.at(x, y).lay(bg, alpha, &host);
            }
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
    /// - A back colour with an `alpha` is mixed with the backdrop the
    ///   app chose, or with the opaque back colour or the host's default
    ///   where it chose none. Inverse cells stay the app's.
    fn resolve(&self, base: &Grid, out: &mut Grid) {
        if self.cells.is_empty() {
            return;
        }
        let host = self.host;
        let no_ink = |x: u16, y: u16| !base.cell(x, y).has_ink();
        for y in 0..self.rows {
            for x in 0..self.cols {
                let i = y as usize * self.cols as usize + x as usize;
                let Some(back) = &self.cells[i] else {
                    continue;
                };
                // Background the back layers give the cell `(x, y)`;
                // `None`: the app's.
                let ground = |x: u16| -> Option<Color> {
                    let own = self.cells[y as usize * self.cols as usize + x as usize].as_ref()?;
                    let app = base.cell(x, y);
                    if app.style.attrs.contains(Attrs::INVERSE) {
                        return None;
                    }
                    if app.style.bg == Color::Default {
                        let under = palette::rgb(own.bg.unwrap_or(Color::Default), host.bg);
                        own.glazed(under).or(own.bg)
                    } else {
                        own.glazed(palette::rgb(app.style.bg, host.bg))
                    }
                };
                // Style of the back glyph `ink` seen in the cell `(x, y)`.
                let seen = |x: u16, ink: &BackCell| {
                    let app = base.cell(x, y);
                    let mut style = if is_open(app) {
                        Style {
                            fg: ink.fg.unwrap_or(app.style.fg),
                            bg: ground(x).unwrap_or(Color::Default),
                            attrs: ink.attrs,
                            ..Style::default()
                        }
                    } else {
                        let below = Style {
                            bg: ground(x).unwrap_or(app.style.bg),
                            ..app.style
                        };
                        on_backdrop(below, ink.fg, None, ink.attrs)
                    };
                    if let (Some(alpha), Some(fg)) = (ink.fade, ink.fg)
                        && !style.attrs.contains(Attrs::INVERSE)
                    {
                        style.fg = faded(style.bg, fg, alpha, &host);
                    }
                    style
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
                        if let (true, Some(bg)) = (app.width != 0, ground(x)) {
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

/// A wide glyph without a colour of its own takes the background below
/// it, and has one style: on two backdrops it is not drawn, by halves
/// neither.
fn splits(out: &Grid, x: u16, y: u16, glyph: &raster::Glyph) -> bool {
    glyph.cell.width == 2
        && glyph.bg.is_none()
        && x + 1 < out.cols()
        && out.cell(x, y).backdrop() != out.cell(x + 1, y).backdrop()
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
    if splits(out, x, y, glyph) {
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

/// The colour `fg` at opacity `alpha` on the background `bg`.
fn faded(bg: Color, fg: Color, alpha: f32, host: &HostColors) -> Color {
    let [r, g, b] = palette::mix(palette::rgb(bg, host.bg), palette::rgb(fg, host.fg), alpha);
    Color::Rgb(r, g, b)
}

/// `style` with the colours it shows as RGB, not inverse.
fn flat(style: Style, host: &HostColors) -> (Rgb, Rgb) {
    let fg = palette::rgb(style.fg, host.fg);
    let bg = palette::rgb(style.bg, host.bg);
    if style.attrs.contains(Attrs::INVERSE) {
        (bg, fg)
    } else {
        (fg, bg)
    }
}

/// A front layer with an `alpha` under 1: its colours are mixed with the
/// colours below.
///
/// - A glyph replaces the glyph below; its `fg` is mixed with the
///   background it stands on, its `bg` with the background below.
/// - A blank with a `bg` is glass: the glyph below stays, both its
///   colours mixed with the `bg`.
/// - A tint mixes `fg` with the foreground below, `bg` with the
///   background below.
fn paint_alpha(
    out: &mut Grid,
    x: u16,
    y: u16,
    glyph: &raster::Glyph,
    tint_only: bool,
    alpha: f32,
    host: &HostColors,
) {
    let color = |[r, g, b]: Rgb| Color::Rgb(r, g, b);
    // Mix the colours of the cell `(x, y)` with `fg` and `bg`.
    let mut veil = |x: u16, fg: Option<Rgb>, bg: Option<Rgb>, attrs: Attrs| {
        let style = &mut out.cell_mut(x, y).style;
        let (below_fg, below_bg) = flat(*style, host);
        if style.attrs.contains(Attrs::INVERSE) {
            style.attrs.remove(Attrs::INVERSE);
            style.fg = color(below_fg);
            style.bg = color(below_bg);
        }
        if let Some(fg) = fg {
            style.fg = color(palette::mix(below_fg, fg, alpha));
        }
        if let Some(bg) = bg {
            style.bg = color(palette::mix(below_bg, bg, alpha));
        }
        style.attrs.insert(attrs.0);
    };
    let fg = glyph.fg.map(|c| palette::rgb(c, host.fg));
    let bg = glyph.bg.map(|c| palette::rgb(c, host.bg));
    if tint_only {
        for x in x..x + glyph.cell.width as u16 {
            veil(x, fg, bg, glyph.attrs);
        }
        return;
    }
    if glyph.cell.is_blank() {
        match bg {
            Some(bg) => veil(x, Some(bg), Some(bg), Attrs::default()),
            None => paint(out, x, y, glyph, false),
        }
        return;
    }
    if splits(out, x, y, glyph) {
        return;
    }
    let below = out.cell(x, y).style;
    let (below_fg, below_bg) = flat(below, host);
    let ground = bg.map_or(below_bg, |bg| palette::mix(below_bg, bg, alpha));
    // Colours unchanged stay as the app wrote them.
    let keep = !below.attrs.contains(Attrs::INVERSE);
    let style = Style {
        fg: match fg {
            Some(fg) => color(palette::mix(ground, fg, alpha)),
            None if keep => below.fg,
            None => color(below_fg),
        },
        bg: match bg {
            None if keep => below.bg,
            _ => color(ground),
        },
        attrs: glyph.attrs,
        ..Style::default()
    };
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
