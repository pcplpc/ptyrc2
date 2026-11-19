// SPDX-License-Identifier: GPL-3.0-or-later
//! Composition: the app grid with the sprites on it.
//!
//! A `Composer` keeps the last composition and its buffers, and composes
//! again only the rows that may differ: rows of the app grid written
//! since (`Grid::row_stamp`), rows a sprite covered or covers when it
//! moved or changed (`Sprite::rev`, its place), rows a `rows` patch
//! replaced. A row is composed from that row of the app grid and the
//! sprites on it alone. A new size, other host colours: every row.

use super::raster::{self, Slot};
use super::{Blend, HostColors, Scene, Sprite, faded, is_open, on_backdrop, paint, paint_alpha};
use crate::types::{SpriteId, Surfaces};
use ptyrc_vt::palette::{self, Rgb};
use ptyrc_vt::{Attrs, Cell, Color, Grid, Rect, Style};

/// Where a sprite is drawn: top-left screen cell, the view it is clipped
/// to (inside the screen, not empty), its size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Place {
    left: i64,
    top: i64,
    clip: Rect,
    w: u16,
    h: u16,
}

impl Place {
    /// Screen rows it may draw on.
    fn rows(&self) -> std::ops::Range<usize> {
        let top = self.top.max(self.clip.y as i64);
        let bottom = (self.top + self.h as i64).min(self.clip.bottom() as i64);
        if bottom <= top {
            return 0..0;
        }
        top as usize..bottom as usize
    }
}

/// Where `sprite` is drawn now; `None`: nowhere.
pub(super) fn place_of(
    sprite: &Sprite,
    base: &Grid,
    surfaces: &dyn Surfaces,
    cursor: (u16, u16),
) -> Option<Place> {
    if sprite.hidden.is_some() {
        return None;
    }
    let info = sprite.frame.and_then(|id| surfaces.info(id))?;
    if !info.usable {
        return None;
    }
    let (left, top) = sprite.locate(&info, cursor)?;
    let clip = info.rect.intersect(&base.rect());
    if clip.is_empty() {
        return None;
    }
    let canvas = sprite.canvas();
    Some(Place {
        left,
        top,
        clip,
        w: canvas.width(),
        h: canvas.height(),
    })
}

/// Call `f` with every glyph of `sprite` that is inside its view, at its
/// screen cell.
pub(super) fn each_glyph<'a>(
    sprite: &'a Sprite,
    base: &Grid,
    surfaces: &dyn Surfaces,
    cursor: (u16, u16),
    mut f: impl FnMut(u16, u16, &'a raster::Glyph),
) {
    if let Some(place) = place_of(sprite, base, surfaces, cursor) {
        glyphs_in(sprite, &place, base, None, |x, y, glyph, _| f(x, y, glyph));
    }
}

/// `each_glyph` at `place`, on the screen rows `rows` marks (all when
/// `None`); `f` is also given the index of the slot in the canvas.
fn glyphs_in<'a>(
    sprite: &'a Sprite,
    place: &Place,
    base: &Grid,
    rows: Option<&[bool]>,
    mut f: impl FnMut(u16, u16, &'a raster::Glyph, u32),
) {
    let Place {
        left, top, clip, ..
    } = *place;
    let canvas = sprite.canvas();
    for sy in place.rows() {
        if rows.is_some_and(|rows| !rows[sy]) {
            continue;
        }
        let cy = (sy as i64 - top) as u16;
        for cx in 0..canvas.width() {
            let Slot::Glyph(glyph) = canvas.slot(cx, cy) else {
                continue;
            };
            let sx = left + cx as i64;
            let width = glyph.cell.width as i64;
            if sx < clip.x as i64 || sx + width > clip.right() as i64 {
                continue;
            }
            if let Some(panel) = sprite.panel
                && !(sx..sx + width.max(1))
                    .all(|x| base.cell(x as u16, sy as u16).backdrop() == panel)
            {
                continue;
            }
            let slot = cy as u32 * canvas.width() as u32 + cx as u32;
            f(sx as u16, sy as u16, glyph, slot);
        }
    }
}

/// The last composition, and what it was made of.
pub struct Composer {
    out: Grid,
    back: Back,
    /// `Grid::row_stamp` of each row of the app grid composed.
    stamps: Vec<(u64, u64)>,
    host: Option<HostColors>,
    /// `Scene::rev` at the last composition.
    seen: u64,
    /// Each sprite drawn, by id, and where.
    placed: Vec<(SpriteId, Option<Place>)>,
    /// Rows to compose again.
    dirty: Vec<bool>,
    /// Rows composed again at the last `compose`.
    pub(super) recomposed: usize,
}

impl Default for Composer {
    fn default() -> Composer {
        Composer::new()
    }
}

impl Composer {
    pub fn new() -> Composer {
        Composer {
            out: Grid::new(0, 0),
            back: Back::default(),
            stamps: Vec::new(),
            host: None,
            seen: 0,
            placed: Vec::new(),
            dirty: Vec::new(),
            recomposed: 0,
        }
    }

    /// The last composition.
    pub fn grid(&self) -> &Grid {
        &self.out
    }

    /// Rows composed again at the last `Scene::compose_with`.
    pub fn recomposed(&self) -> usize {
        self.recomposed
    }

    /// Mark the rows a sprite covers at `place`.
    fn mark(&mut self, place: &Option<Place>) {
        if let Some(place) = place {
            for row in place.rows() {
                self.dirty[row] = true;
            }
        }
    }
}

impl Scene {
    /// The app grid with the sprites on it: the back layers seen through
    /// the cells the app left without ink, the app, the front layers.
    /// Uses the anchor state of the last `tick` and the surfaces as they
    /// are now: a scroll between the two moves frame-anchored sprites.
    pub fn compose(&self, base: &Grid, surfaces: &dyn Surfaces) -> Grid {
        let mut composer = Composer::new();
        self.compose_with(base, surfaces, &mut composer);
        composer.out
    }

    /// `compose` into `composer`, again only where the result may have
    /// changed since it last composed.
    pub fn compose_with<'c>(
        &self,
        base: &Grid,
        surfaces: &dyn Surfaces,
        c: &'c mut Composer,
    ) -> &'c Grid {
        let (cols, rows) = (base.cols(), base.rows());
        let cursor = surfaces.cursor();
        let whole = (c.out.cols(), c.out.rows()) != (cols, rows) || c.host != Some(self.host);
        if whole {
            c.out = base.clone();
            c.back = Back::new(cols, rows, self.host);
            c.stamps = vec![(0, 0); rows as usize];
            c.placed.clear();
            c.host = Some(self.host);
        }
        c.dirty.clear();
        c.dirty.resize(rows as usize, whole);
        for (row, stamp) in c.stamps.iter_mut().enumerate() {
            let now = base.row_stamp(row as u16);
            if *stamp != now {
                *stamp = now;
                c.dirty[row] = true;
            }
        }

        // Where each sprite is now; the rows of those that moved or
        // changed, where they were and where they are.
        let places: Vec<Option<Place>> = self
            .sprites
            .iter()
            .map(|sprite| place_of(sprite, base, surfaces, cursor))
            .collect();
        let mut by_id: Vec<usize> = (0..self.sprites.len()).collect();
        by_id.sort_unstable_by_key(|&i| self.sprites[i].id);
        let old = std::mem::take(&mut c.placed);
        let mut old_iter = old.iter().peekable();
        for &index in &by_id {
            let (sprite, place) = (&self.sprites[index], places[index]);
            let id = sprite.id;
            // Gone since.
            while let Some((_, was)) = old_iter.next_if(|(o, _)| *o < id) {
                c.mark(was);
            }
            match old_iter.next_if(|(o, _)| *o == id) {
                Some((_, was)) if *was == place && sprite.rev <= c.seen => {
                    // In place, unchanged: the rows a `rows` patch
                    // replaced since.
                    if let (Some(place), super::Body::Anim(anim)) = (&place, &sprite.body) {
                        for (cy, _) in anim
                            .row_rev
                            .iter()
                            .enumerate()
                            .filter(|(_, rev)| **rev > c.seen)
                        {
                            let sy = place.top + cy as i64;
                            if sy >= 0 && place.rows().contains(&(sy as usize)) {
                                c.dirty[sy as usize] = true;
                            }
                        }
                    }
                }
                Some((_, was)) => {
                    c.mark(was);
                    c.mark(&place);
                }
                None => c.mark(&place),
            }
        }
        for (_, was) in old_iter {
            c.mark(was);
        }
        c.placed = by_id
            .iter()
            .map(|&i| (self.sprites[i].id, places[i]))
            .collect();
        c.seen = self.rev;

        let dirty = std::mem::take(&mut c.dirty);
        let mut recomposed = 0;
        for (row, _) in dirty.iter().enumerate().filter(|(_, d)| **d) {
            recomposed += 1;
            c.out
                .row_mut(row as u16)
                .clone_from_slice(base.row(row as u16));
            c.back.clear_row(row as u16);
        }
        c.recomposed = recomposed;
        if recomposed > 0 {
            let front = self.sprites.partition_point(|s| s.layer < 0);
            let back = &mut c.back;
            for (index, sprite) in self.sprites[..front].iter().enumerate() {
                let Some(place) = &places[index] else {
                    continue;
                };
                let tint = sprite.blend == Blend::Tint;
                let cover = sprite.panel.is_some();
                glyphs_in(sprite, place, base, Some(&dirty), |x, y, glyph, slot| {
                    let at = GlyphAt {
                        sprite: index as u32,
                        slot,
                        width: glyph.cell.width,
                    };
                    back.paint(x, y, glyph, at, tint, sprite.alpha * glyph.alpha(), cover)
                });
            }
            back.resolve(base, &mut c.out, &dirty, |at| {
                match self.sprites[at.sprite as usize].canvas().slot_at(at.slot) {
                    Slot::Glyph(glyph) => &glyph.cell,
                    _ => unreachable!("a back glyph is a glyph"),
                }
            });
            for (index, sprite) in self.sprites.iter().enumerate().skip(front) {
                let Some(place) = &places[index] else {
                    continue;
                };
                let tint = sprite.blend == Blend::Tint;
                let out = &mut c.out;
                glyphs_in(sprite, place, base, Some(&dirty), |x, y, glyph, _| {
                    let alpha = sprite.alpha * glyph.alpha();
                    if alpha < 1.0 {
                        paint_alpha(out, x, y, glyph, tint, alpha, &self.host)
                    } else {
                        paint(out, x, y, glyph, tint)
                    }
                });
            }
        }
        c.dirty = dirty;
        &c.out
    }
}

/// A glyph of a back sprite: sprite index in paint order, slot index in
/// its canvas, width.
#[derive(Clone, Copy)]
struct GlyphAt {
    sprite: u32,
    slot: u32,
    width: u8,
}

/// What the back layers put in one cell.
#[derive(Clone, Copy, Default)]
struct BackCell {
    /// The glyph, in its sprite; `None`: a colour only.
    glyph: Option<GlyphAt>,
    /// Right half of the wide glyph before it.
    spacer: bool,
    fg: Option<Color>,
    /// Opacity of `fg`, when the sprite has an `alpha`.
    fade: Option<f32>,
    /// Opaque colour.
    bg: Option<Color>,
    /// `bg` is laid on the app's panel: it replaces the backdrop.
    cover: bool,
    /// Translucent colours laid over `bg`: their sum, premultiplied, and
    /// its opacity.
    glaze: Option<([f32; 3], f32)>,
    attrs: Attrs,
}

impl BackCell {
    /// Lay the colour `bg` at opacity `alpha` over what is there.
    fn lay(&mut self, bg: Color, alpha: f32, cover: bool, host: &HostColors) {
        if alpha >= 1.0 {
            self.bg = Some(bg);
            self.cover = cover;
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
#[derive(Default)]
struct Back {
    cols: u16,
    rows: u16,
    cells: Vec<Option<BackCell>>,
    host: HostColors,
    /// Per row: a cell was laid since the row was cleared.
    laid: Vec<bool>,
}

impl Back {
    fn new(cols: u16, rows: u16, host: HostColors) -> Back {
        Back {
            cols,
            rows,
            cells: Vec::new(),
            host,
            laid: vec![false; rows as usize],
        }
    }

    fn clear_row(&mut self, y: u16) {
        if !std::mem::take(&mut self.laid[y as usize]) {
            return;
        }
        let start = y as usize * self.cols as usize;
        self.cells[start..start + self.cols as usize].fill(None);
    }

    fn at(&mut self, x: u16, y: u16) -> &mut BackCell {
        if self.cells.is_empty() {
            self.cells = vec![None; self.cols as usize * self.rows as usize];
        }
        self.laid[y as usize] = true;
        self.cells[y as usize * self.cols as usize + x as usize].get_or_insert_default()
    }

    /// Take the glyph out of `(x, y)` and out of its other half; the
    /// colour stays.
    fn cut(&mut self, x: u16, y: u16) {
        let i = y as usize * self.cols as usize + x as usize;
        if self.cells.get(i).is_none_or(|cell| cell.is_none()) {
            // Nothing was laid here.
            return;
        }
        let cell = self.at(x, y);
        let other = if cell.spacer && x > 0 {
            Some(x - 1)
        } else if cell.glyph.is_some_and(|g| g.width == 2) && x + 1 < self.cols {
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

    #[allow(clippy::too_many_arguments)]
    fn paint(
        &mut self,
        x: u16,
        y: u16,
        glyph: &raster::Glyph,
        at: GlyphAt,
        tint: bool,
        alpha: f32,
        cover: bool,
    ) {
        let wide = glyph.cell.width == 2;
        let host = self.host;
        if !tint {
            self.cut(x, y);
            let cell = self.at(x, y);
            cell.glyph = Some(at);
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
                self.at(x, y).lay(bg, alpha, cover, &host);
            }
        }
    }

    /// Put the app on top of the rows `rows` marks: `out` is `base` with
    /// the back layers seen through it. `text`: the cell of a glyph.
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
    fn resolve<'a>(
        &self,
        base: &Grid,
        out: &mut Grid,
        rows: &[bool],
        text: impl Fn(GlyphAt) -> &'a Cell,
    ) {
        if self.cells.is_empty() {
            return;
        }
        let host = self.host;
        let no_ink = |x: u16, y: u16| !base.cell(x, y).has_ink();
        for y in 0..self.rows {
            if !rows[y as usize] || !self.laid[y as usize] {
                continue;
            }
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
                    if app.style.bg == Color::Default || (own.cover && own.bg.is_some()) {
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
                    if let (Some(alpha), Some(fg)) = (ink.fade, ink.fg) {
                        if style.attrs.contains(Attrs::INVERSE) {
                            // Inverse: the glyph is drawn in `bg`, on the
                            // app's foreground.
                            let ground = palette::rgb(style.fg, host.fg);
                            let [r, g, b] = palette::mix(ground, palette::rgb(fg, host.fg), alpha);
                            style.bg = Color::Rgb(r, g, b);
                        } else {
                            style.fg = faded(style.bg, fg, alpha, &host);
                        }
                    }
                    style
                };
                // A wide glyph needs two cells without ink, and one
                // style: both halves on the same backdrop.
                let glyph = back.glyph.filter(|g| {
                    no_ink(x, y)
                        && (g.width != 2
                            || (x + 1 < self.cols
                                && no_ink(x + 1, y)
                                && seen(x, back) == seen(x + 1, back)))
                });
                match glyph {
                    Some(g) => {
                        let g = text(g);
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
