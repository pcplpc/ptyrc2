// SPDX-License-Identifier: GPL-3.0-or-later
//! Widgets: small windows clients keep on screen, laid out by the driver.
//! See `plan/design.md`, "ptyrc-core: widgets".
//!
//! The driver calls `touch` when the screen or the frames changed, and
//! `tick` before the scene is ticked and composed: a widget moves in the
//! same repaint.

mod border;
mod rooms;
#[cfg(test)]
mod tests;

pub use border::{Align, Border, BorderPatch, BorderSpec, TitlePatch, TitleSpec, compose, presets};

use crate::frames::{HeatClass, HeatView};
use crate::scene::{Anchor, Content, Layer, Scene, Space, SpritePatch, SpriteSpec};
use crate::types::{FrameId, FrameSel, OwnerId, Prefer, RAW, SpriteId, SurfaceInfo, Surfaces};
use ptyrc_vt::{Grid, Rect};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::time::Duration;

pub type WidgetId = u64;

/// Tags a connection declares, at most.
pub const MAX_TAGS: usize = 64;
/// Variants of a tag, at most.
pub const MAX_VARIANTS: usize = 16;
/// Largest inner size of a variant.
pub const MAX_SIZE: [u16; 2] = [200, 60];

const FRAME_NAMES: [&str; 5] = ["raw", "active", "largest", "quietest", "idle"];

/// Tunables. Times in milliseconds.
#[derive(Clone, Debug, PartialEq)]
pub struct WidgetConfig {
    /// A cell is open after this long without a visible change.
    pub quiet_ms: u32,
    /// Columns and rows kept between a widget and what is around it.
    pub gap: (u16, u16),
    /// A widget pressed this long is laid out again.
    pub evict_ms: u32,
    /// A larger variant elsewhere must be found this long before the
    /// widget moves there.
    pub replace_ms: u32,
    /// Least time between two looks.
    pub look_ms: u32,
}

impl Default for WidgetConfig {
    fn default() -> WidgetConfig {
        WidgetConfig {
            quiet_ms: 1000,
            gap: (2, 1),
            evict_ms: 300,
            replace_ms: 2000,
            look_ms: 250,
        }
    }
}

/// One variant of a tag, as declared.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariantDecl {
    pub tag: String,
    /// `WxH` when absent; unique in the tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Inner size `[w, h]`.
    pub size: [u16; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
}

/// `widget.declare`: variants, grouped by tag; `border`: the default of
/// the connection.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Declare {
    #[serde(default, deserialize_with = "or_default")]
    pub widgets: Vec<VariantDecl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<Border>,
}

fn default_fill() -> f32 {
    0.5
}

/// A member given as null is absent.
fn or_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

fn or_fill<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
    Ok(Option::<f32>::deserialize(d)?.unwrap_or_else(default_fill))
}

/// `widget.show`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Show {
    pub tag: String,
    #[serde(default, deserialize_with = "or_default")]
    pub frame: FrameSel,
    #[serde(default, deserialize_with = "or_default")]
    pub prefer: Prefer,
    #[serde(default = "default_fill", deserialize_with = "or_fill")]
    pub fill: f32,
    /// The tag when absent; `""`: none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<Border>,
    /// `back` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<Layer>,
}

impl Show {
    pub fn new(tag: &str) -> Show {
        Show {
            tag: tag.into(),
            frame: FrameSel::default(),
            prefer: Prefer::Any,
            fill: default_fill(),
            title: None,
            border: None,
            layer: None,
        }
    }
}

/// `widget.set`: the content of a variant, the title, or both.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Set {
    pub id: WidgetId,
    /// Absent: the variant shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WidgetState {
    Placed,
    Hidden,
    Gone,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariantInfo {
    pub name: String,
    pub size: [u16; 2],
}

/// A widget, as on the wire. `size`: inner, of the variant shown;
/// `rect`: outer, screen cells; `border`: null for an object.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WidgetInfo {
    pub id: WidgetId,
    pub owner: OwnerId,
    pub tag: String,
    pub frame: Option<FrameId>,
    pub state: WidgetState,
    pub variant: Option<String>,
    pub size: Option<[u16; 2]>,
    pub rect: Option<[u16; 4]>,
    pub sprite: Option<SpriteId>,
    pub title: String,
    pub border: Option<String>,
    pub variants: Vec<VariantInfo>,
}

/// Widget failure; `code` is the wire error code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WidgetError {
    /// No widget of that id for this connection.
    NoWidget(WidgetId),
    NoFrame(String),
    NotUsable(String),
    BadParams(String),
}

impl WidgetError {
    pub fn code(&self) -> &'static str {
        match self {
            WidgetError::NoWidget(_) => "no-sprite",
            WidgetError::NoFrame(_) => "no-frame",
            WidgetError::NotUsable(_) => "not-usable",
            WidgetError::BadParams(_) => "bad-params",
        }
    }
}

impl std::fmt::Display for WidgetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WidgetError::NoWidget(id) => write!(f, "no such widget: {id}"),
            WidgetError::NoFrame(name) => write!(f, "no such frame: {name}"),
            WidgetError::NotUsable(why) | WidgetError::BadParams(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for WidgetError {}

fn bad<T>(why: impl Into<String>) -> Result<T, WidgetError> {
    Err(WidgetError::BadParams(why.into()))
}

#[derive(Clone, Debug)]
struct Variant {
    name: String,
    size: [u16; 2],
    content: Option<Content>,
}

/// A border resolved: its preset name (none for an object) and spec.
#[derive(Clone, Debug)]
struct Framed {
    name: Option<String>,
    spec: BorderSpec,
}

/// What a connection declared.
#[derive(Default)]
struct Decl {
    /// In order of first declaration.
    tags: Vec<(String, Vec<Variant>)>,
    border: Option<Framed>,
}

impl Decl {
    fn variants(&self, tag: &str) -> Option<&Vec<Variant>> {
        self.tags.iter().find(|(t, _)| t == tag).map(|(_, v)| v)
    }
}

/// Where a widget is: variant, top-left in view cells, outer size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Place {
    variant: usize,
    at: (u16, u16),
    outer: (u16, u16),
}

impl Place {
    fn rect(&self) -> Rect {
        Rect::new(self.at.0, self.at.1, self.outer.0, self.outer.1)
    }
}

/// What the sprite of a widget shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Drawn {
    frame: FrameId,
    at: (u16, u16),
    layer: i32,
    rev: u64,
}

struct Widget {
    id: WidgetId,
    owner: OwnerId,
    tag: String,
    sel: FrameSel,
    frame: Option<FrameId>,
    prefer: Prefer,
    fill: f32,
    title: Option<String>,
    border: Option<Framed>,
    layer: Layer,
    place: Option<Place>,
    /// Laid out again as a new one at the next look.
    relayout: bool,
    /// Pressed since.
    pressed: Option<Duration>,
    /// A larger place found, and since when.
    grow: Option<(Place, Duration)>,
    sprite: Option<SpriteId>,
    drawn: Option<Drawn>,
    /// Changes when what the sprite shows changes.
    rev: u64,
    /// Outer rect on screen at the last tick.
    screen: Option<[u16; 4]>,
    /// Its frame was usable at the last tick.
    live: bool,
    /// The info last returned by `tick`.
    told: Option<WidgetInfo>,
}

/// One frame as a look sees it.
struct Look {
    info: SurfaceInfo,
    /// The view, clipped to the screen.
    view: Rect,
    /// Open cells of the view, row-major, before the widgets.
    open: Vec<bool>,
}

/// Order of the rooms: largest variant, nearest to `prefer`, largest
/// room, topmost, leftmost.
type Rank = (Reverse<u32>, i64, Reverse<u32>, u16, u16);

/// Every widget of every connection, and the layout.
pub struct Widgets {
    cfg: WidgetConfig,
    borders: Vec<(String, BorderSpec)>,
    decls: std::collections::BTreeMap<OwnerId, Decl>,
    /// Oldest first.
    widgets: Vec<Widget>,
    next_id: WidgetId,
    /// Widgets gone since the last tick: their info, their sprite.
    gone: Vec<(WidgetInfo, Option<SpriteId>)>,
    /// The screen, an obstacle or a declaration changed.
    dirty: bool,
    /// A look at the next tick, whatever the pace.
    urgent: bool,
    /// A sprite to update at the next tick.
    pending: bool,
    last_look: Option<Duration>,
    last_tick: Duration,
    /// Next timer: an eviction, a move, cells turning quiet.
    due: Option<Duration>,
    /// When the next cell of a view turns quiet.
    quiet_due: Option<Duration>,
}

fn ms(n: u32) -> Duration {
    Duration::from_millis(n as u64)
}

impl Widgets {
    pub fn new(cfg: WidgetConfig) -> Widgets {
        let mut widgets = Widgets {
            cfg,
            borders: Vec::new(),
            decls: Default::default(),
            widgets: Vec::new(),
            next_id: 1,
            gone: Vec::new(),
            dirty: false,
            urgent: false,
            pending: false,
            last_look: None,
            last_tick: Duration::ZERO,
            due: None,
            quiet_due: None,
        };
        for (name, spec) in presets() {
            widgets.register_border(name, spec);
        }
        widgets
    }

    pub fn config(&self) -> &WidgetConfig {
        &self.cfg
    }

    /// Add a preset, or replace the one of that name.
    pub fn register_border(&mut self, name: &str, border: BorderSpec) {
        match self.borders.iter_mut().find(|(n, _)| n == name) {
            Some(slot) => slot.1 = border,
            None => self.borders.push((name.to_string(), border)),
        }
    }

    /// Preset names: the core's, then the extensions'.
    pub fn borders(&self) -> Vec<String> {
        self.borders.iter().map(|(n, _)| n.clone()).collect()
    }

    fn preset(&self, name: &str) -> Result<&BorderSpec, WidgetError> {
        match self.borders.iter().find(|(n, _)| n == name) {
            Some((_, spec)) => Ok(spec),
            None => bad(format!("no such border: {name}")),
        }
    }

    fn resolve_border(&self, border: &Border) -> Result<Framed, WidgetError> {
        match border {
            Border::Name(name) => Ok(Framed {
                name: Some(name.clone()),
                spec: self.preset(name)?.clone(),
            }),
            Border::Object(patch) => {
                let base = self.preset(patch.base.as_deref().unwrap_or("plain"))?;
                let spec = patch.apply(base);
                spec.check().map_err(WidgetError::BadParams)?;
                Ok(Framed { name: None, spec })
            }
        }
    }

    fn check_content(content: &Option<Content>) -> Result<(), WidgetError> {
        match content {
            Some(c) if c.rows.is_some() && c.cells.is_some() => {
                bad("content: rows and cells are exclusive")
            }
            Some(c) if c.rows.is_none() && c.cells.is_none() => bad("content: rows or cells"),
            _ => Ok(()),
        }
    }

    /// Declare variants for the connection. A tag declared again has its
    /// variants replaced, and its widget is laid out again. Returns the
    /// tags of the connection, in order of first declaration. Nothing is
    /// applied on error.
    pub fn declare(&mut self, owner: OwnerId, decl: Declare) -> Result<Vec<String>, WidgetError> {
        let border = decl
            .border
            .as_ref()
            .map(|b| self.resolve_border(b))
            .transpose()?;
        let mut tags: Vec<(String, Vec<Variant>)> = Vec::new();
        for v in decl.widgets {
            if v.tag.is_empty() || v.tag.chars().count() > 64 {
                return bad(format!("tag: 1 to 64 characters, not {:?}", v.tag));
            }
            let [w, h] = v.size;
            if w == 0 || h == 0 || w > MAX_SIZE[0] || h > MAX_SIZE[1] {
                return bad(format!(
                    "size of {}: from 1x1 to {}x{}, not {w}x{h}",
                    v.tag, MAX_SIZE[0], MAX_SIZE[1]
                ));
            }
            Widgets::check_content(&v.content)?;
            let name = v.name.unwrap_or_else(|| format!("{w}x{h}"));
            if name.is_empty() {
                return bad(format!("variant of {}: an empty name", v.tag));
            }
            let at = match tags.iter().position(|(t, _)| *t == v.tag) {
                Some(at) => at,
                None => {
                    tags.push((v.tag.clone(), Vec::new()));
                    tags.len() - 1
                }
            };
            let list = &mut tags[at].1;
            if list.iter().any(|x| x.name == name) {
                return bad(format!("variant {name} of {}: declared twice", v.tag));
            }
            if list.len() == MAX_VARIANTS {
                return bad(format!("variants of {}: {MAX_VARIANTS} at most", v.tag));
            }
            list.push(Variant {
                name,
                size: v.size,
                content: v.content,
            });
        }
        let own = self.decls.entry(owner).or_default();
        let new = tags
            .iter()
            .filter(|(t, _)| own.variants(t).is_none())
            .count();
        if own.tags.len() + new > MAX_TAGS {
            return bad(format!("tags: {MAX_TAGS} a connection at most"));
        }
        let border_changed = border.is_some();
        if let Some(border) = border {
            own.border = Some(border);
        }
        let mut touched: Vec<String> = Vec::new();
        for (tag, variants) in tags {
            match own.tags.iter_mut().find(|(t, _)| *t == tag) {
                Some(slot) => slot.1 = variants,
                None => own.tags.push((tag.clone(), variants)),
            }
            touched.push(tag);
        }
        let names = own.tags.iter().map(|(t, _)| t.clone()).collect();
        for w in self.widgets.iter_mut().filter(|w| w.owner == owner) {
            if touched.contains(&w.tag) || (border_changed && w.border.is_none()) {
                w.relayout = true;
                w.rev += 1;
            }
        }
        self.dirty = true;
        self.pending = true;
        Ok(names)
    }

    /// Show a declared tag: one widget per connection and tag. Shown
    /// again, the same widget with the new options, laid out again.
    pub fn show(&mut self, owner: OwnerId, show: Show) -> Result<WidgetId, WidgetError> {
        if self
            .decls
            .get(&owner)
            .and_then(|d| d.variants(&show.tag))
            .is_none()
        {
            return bad(format!("no such tag: {}", show.tag));
        }
        if !(0.1..=1.0).contains(&show.fill) {
            return bad(format!("fill: 0.1 to 1, not {}", show.fill));
        }
        let border = show
            .border
            .as_ref()
            .map(|b| self.resolve_border(b))
            .transpose()?;
        let frame = match &show.frame {
            FrameSel::Id(id) => Some(*id),
            FrameSel::Name(name) if name == "raw" => Some(RAW),
            FrameSel::Name(name) if FRAME_NAMES.contains(&name.as_str()) => None,
            FrameSel::Name(name) => return Err(WidgetError::NoFrame(name.clone())),
        };
        let layer = show.layer.unwrap_or(Layer::BACK);
        let id = match self
            .widgets
            .iter_mut()
            .find(|w| w.owner == owner && w.tag == show.tag)
        {
            Some(w) => {
                w.sel = show.frame;
                w.frame = frame;
                w.prefer = show.prefer;
                w.fill = show.fill;
                w.title = show.title;
                w.border = border;
                w.layer = layer;
                w.relayout = true;
                w.rev += 1;
                w.id
            }
            None => {
                let id = self.next_id;
                self.next_id += 1;
                self.widgets.push(Widget {
                    id,
                    owner,
                    tag: show.tag,
                    sel: show.frame,
                    frame,
                    prefer: show.prefer,
                    fill: show.fill,
                    title: show.title,
                    border,
                    layer,
                    place: None,
                    relayout: true,
                    pressed: None,
                    grow: None,
                    sprite: None,
                    drawn: None,
                    rev: 0,
                    screen: None,
                    live: false,
                    told: None,
                });
                id
            }
        };
        self.urgent = true;
        self.dirty = true;
        Ok(id)
    }

    /// Set the content of a variant (absent: the one shown) and, or, the
    /// title. Kept by the driver; a variant never set is blank.
    pub fn set(&mut self, owner: OwnerId, set: Set) -> Result<(), WidgetError> {
        let Some(w) = self
            .widgets
            .iter_mut()
            .find(|w| w.id == set.id && w.owner == owner)
        else {
            return Err(WidgetError::NoWidget(set.id));
        };
        if set.content.is_none() && set.title.is_none() {
            return bad("content or title: one at least");
        }
        Widgets::check_content(&set.content)?;
        let decl = self
            .decls
            .get_mut(&owner)
            .expect("a widget has a declaration");
        let variants = decl
            .tags
            .iter_mut()
            .find(|(t, _)| *t == w.tag)
            .map(|(_, v)| v)
            .expect("a widget has a tag");
        if let Some(content) = set.content {
            let index = match &set.variant {
                Some(name) => match variants.iter().position(|v| v.name == *name) {
                    Some(i) => i,
                    None => return bad(format!("no such variant: {name}")),
                },
                None => match w.place {
                    Some(p) => p.variant,
                    None => {
                        return Err(WidgetError::NotUsable(format!(
                            "widget {} shows no variant: name one with `variant`",
                            w.id
                        )));
                    }
                },
            };
            variants[index].content = Some(content);
            if w.place.is_some_and(|p| p.variant == index) {
                w.rev += 1;
            }
        } else if let Some(name) = &set.variant
            && !variants.iter().any(|v| v.name == *name)
        {
            return bad(format!("no such variant: {name}"));
        }
        if let Some(title) = set.title {
            w.title = Some(title);
            w.rev += 1;
        }
        self.pending = true;
        Ok(())
    }

    /// The widget goes; the declaration stays. False: no such widget
    /// for this owner.
    pub fn hide(&mut self, owner: OwnerId, id: WidgetId) -> bool {
        let Some(at) = self
            .widgets
            .iter()
            .position(|w| w.id == id && w.owner == owner)
        else {
            return false;
        };
        let w = self.widgets.remove(at);
        self.gone.push((self.gone_info(&w), w.sprite));
        self.dirty = true;
        true
    }

    /// The owner is gone: its widgets and its declarations.
    pub fn drop_owner(&mut self, owner: OwnerId) {
        let (gone, kept): (Vec<Widget>, Vec<Widget>) = std::mem::take(&mut self.widgets)
            .into_iter()
            .partition(|w| w.owner == owner);
        self.widgets = kept;
        for w in gone {
            self.gone.push((self.gone_info(&w), w.sprite));
            self.dirty = true;
        }
        self.decls.remove(&owner);
    }

    /// The screen or the frames changed: a look is due, `look_ms` after
    /// the last at the earliest.
    pub fn touch(&mut self) {
        self.dirty = true;
    }

    pub fn is_empty(&self) -> bool {
        self.widgets.is_empty() && self.gone.is_empty()
    }

    fn border_of(&self, w: &Widget) -> Framed {
        w.border
            .clone()
            .or_else(|| self.decls.get(&w.owner).and_then(|d| d.border.clone()))
            .unwrap_or_else(|| Framed {
                name: Some("plain".into()),
                spec: BorderSpec::default(),
            })
    }

    fn variants_of(&self, w: &Widget) -> &[Variant] {
        self.decls
            .get(&w.owner)
            .and_then(|d| d.variants(&w.tag))
            .map_or(&[], |v| v.as_slice())
    }

    fn info_of(&self, w: &Widget) -> WidgetInfo {
        let variants = self.variants_of(w);
        let shown = w.place.and_then(|p| variants.get(p.variant));
        WidgetInfo {
            id: w.id,
            owner: w.owner,
            tag: w.tag.clone(),
            frame: w.frame,
            state: if w.place.is_some() {
                WidgetState::Placed
            } else {
                WidgetState::Hidden
            },
            variant: shown.map(|v| v.name.clone()),
            size: shown.map(|v| v.size),
            rect: w.place.and(w.screen),
            sprite: w.place.and(w.sprite),
            title: w.title.clone().unwrap_or_else(|| w.tag.clone()),
            border: self.border_of(w).name,
            variants: variants
                .iter()
                .map(|v| VariantInfo {
                    name: v.name.clone(),
                    size: v.size,
                })
                .collect(),
        }
    }

    fn gone_info(&self, w: &Widget) -> WidgetInfo {
        WidgetInfo {
            state: WidgetState::Gone,
            variant: None,
            size: None,
            rect: None,
            sprite: None,
            ..self.info_of(w)
        }
    }

    /// Every widget, oldest first.
    pub fn list(&self) -> Vec<WidgetInfo> {
        self.widgets.iter().map(|w| self.info_of(w)).collect()
    }

    pub fn info(&self, id: WidgetId) -> Option<WidgetInfo> {
        self.widgets
            .iter()
            .find(|w| w.id == id)
            .map(|w| self.info_of(w))
    }

    /// Ids of the widgets of an owner.
    pub fn ids_of(&self, owner: OwnerId) -> Vec<WidgetId> {
        self.widgets
            .iter()
            .filter(|w| w.owner == owner)
            .map(|w| w.id)
            .collect()
    }

    /// Tags an owner declared.
    pub fn tags_of(&self, owner: OwnerId) -> Vec<String> {
        self.decls
            .get(&owner)
            .map(|d| d.tags.iter().map(|(t, _)| t.clone()).collect())
            .unwrap_or_default()
    }

    /// Variant names of a widget's tag.
    pub fn variants_named(&self, id: WidgetId) -> Vec<String> {
        self.widgets
            .iter()
            .find(|w| w.id == id)
            .map(|w| self.variants_of(w).iter().map(|v| v.name.clone()).collect())
            .unwrap_or_default()
    }

    /// When `tick` is next due; `None`: nothing changes by itself.
    pub fn next_wake(&self) -> Option<Duration> {
        if self.is_empty() {
            return None;
        }
        let mut wake = self.due;
        let mut at = |t: Duration| wake = Some(wake.map_or(t, |w| w.min(t)));
        if self.pending || self.urgent || !self.gone.is_empty() {
            at(self.last_tick);
        }
        if self.dirty {
            at(self
                .last_look
                .map_or(self.last_tick, |l| l + ms(self.cfg.look_ms)));
        }
        wake
    }

    /// Lay out, then add, move, update and remove the sprites. `cursor`:
    /// the screen cell of the cursor when it shows. Returns the widgets
    /// whose info changed, the gone ones included.
    pub fn tick(
        &mut self,
        now: Duration,
        base: &Grid,
        cursor: Option<(u16, u16)>,
        surfaces: &dyn Surfaces,
        scene: &mut Scene,
    ) -> Vec<WidgetInfo> {
        self.last_tick = now;
        let mut changed: Vec<WidgetInfo> = Vec::new();
        for (info, sprite) in std::mem::take(&mut self.gone) {
            if let Some(id) = sprite {
                scene.remove(id);
            }
            changed.push(info);
        }
        self.pending = false;
        if self.widgets.is_empty() {
            self.due = None;
            self.dirty = false;
            self.urgent = false;
            return changed;
        }

        // A sprite removed by other means (`draw.remove`, `draw.clear`, a
        // frame forgotten) takes its widget away.
        let lost: Vec<WidgetId> = self
            .widgets
            .iter()
            .filter(|w| w.sprite.is_some_and(|s| !scene.contains(s)))
            .map(|w| w.id)
            .collect();
        for id in lost {
            if let Some(at) = self.widgets.iter().position(|w| w.id == id) {
                let w = self.widgets.remove(at);
                changed.push(self.gone_info(&w));
                self.dirty = true;
            }
        }

        for w in &mut self.widgets {
            if w.frame.is_none() {
                w.frame = surfaces.resolve(&w.sel);
            }
            let info = w.frame.and_then(|id| surfaces.info(id));
            w.live = info.is_some_and(|i| i.usable);
            if info.is_none() && w.place.is_some() {
                // The frame is no more.
                w.place = None;
                w.pressed = None;
                w.grow = None;
            }
        }

        let timer = self.due.is_some_and(|d| now >= d);
        if self.dirty || self.urgent || timer {
            self.press(now, base, surfaces);
        }
        let paced = self
            .last_look
            .is_none_or(|l| now >= l + ms(self.cfg.look_ms));
        if self.urgent || timer || (self.dirty && paced) {
            self.look(now, base, cursor, surfaces);
            self.dirty = false;
            self.urgent = false;
            self.last_look = Some(now);
        }
        self.sync(now, surfaces, scene);
        self.schedule(now);

        for i in 0..self.widgets.len() {
            let info = self.info_of(&self.widgets[i]);
            if self.widgets[i].told.as_ref() != Some(&info) {
                self.widgets[i].told = Some(info.clone());
                changed.push(info);
            }
        }
        changed
    }

    /// Mark the placed widgets pressed or not: ink under them, or out of
    /// their view.
    fn press(&mut self, now: Duration, base: &Grid, surfaces: &dyn Surfaces) {
        for w in self.widgets.iter_mut().filter(|w| w.live) {
            let (Some(place), Some(info)) = (w.place, w.frame.and_then(|id| surfaces.info(id)))
            else {
                continue;
            };
            let view = info.rect.intersect(&base.rect());
            let r = place.rect();
            let out = r.right() > view.w
                || r.bottom() > view.h
                || view.x != info.rect.x
                || view.y != info.rect.y;
            let ink = !out
                && (r.y..r.bottom())
                    .any(|y| (r.x..r.right()).any(|x| base.cell(view.x + x, view.y + y).has_ink()));
            if out || ink {
                w.pressed.get_or_insert(now);
            } else {
                w.pressed = None;
            }
        }
    }

    /// The next timer: an eviction, a move, a cell turning quiet.
    fn schedule(&mut self, now: Duration) {
        let mut due: Option<Duration> = self.quiet_due.filter(|t| *t > now);
        let mut at = |t: Duration| {
            if t > now {
                due = Some(due.map_or(t, |d| d.min(t)));
            }
        };
        for w in self.widgets.iter().filter(|w| w.live) {
            if let Some(t) = w.pressed {
                at(t + ms(self.cfg.evict_ms));
            }
            if let Some((_, t)) = w.grow {
                at(t + ms(self.cfg.replace_ms));
            }
        }
        self.due = due;
    }

    /// The open cells of a frame's view before the widgets: no ink, not a
    /// panel, quiet, off the cursor's row. Also when the next cell that is
    /// otherwise open turns quiet.
    fn open_cells(
        &self,
        info: SurfaceInfo,
        base: &Grid,
        cursor: Option<(u16, u16)>,
        heat: Option<&HeatView>,
    ) -> (Look, Option<f32>) {
        let view = info.rect.intersect(&base.rect());
        let (w, h) = (view.w as usize, view.h as usize);
        let mut open = vec![false; w * h];
        let quiet = self.cfg.quiet_ms as f32 / 1000.0;
        let mut soonest: Option<f32> = None;
        if view.x == info.rect.x && view.y == info.rect.y {
            for y in 0..h {
                let sy = view.y + y as u16;
                if cursor.is_some_and(|(cx, cy)| cy == sy && cx >= view.x && cx < view.right()) {
                    continue;
                }
                for x in 0..w {
                    if base.cell(view.x + x as u16, sy).has_ink() {
                        continue;
                    }
                    let Some(heat) = heat.filter(|h| x < h.cols as usize && y < h.rows as usize)
                    else {
                        continue;
                    };
                    let i = y * heat.cols as usize + x;
                    if heat.class[i] == HeatClass::Panel {
                        continue;
                    }
                    let still = heat.still[i];
                    if still >= quiet {
                        open[y * w + x] = true;
                    } else {
                        let left = quiet - still;
                        soonest = Some(soonest.map_or(left, |s: f32| s.min(left)));
                    }
                }
            }
        }
        (Look { info, view, open }, soonest)
    }

    /// The best place for a widget as a new one, on `open` (view cells)
    /// less the rects `taken`.
    fn best(&self, w: &Widget, look: &Look, taken: &[Rect]) -> Option<Place> {
        let (vw, vh) = (look.view.w as usize, look.view.h as usize);
        let mut open = look.open.clone();
        for t in taken {
            let t = t.intersect(&Rect::new(0, 0, look.view.w, look.view.h));
            for y in t.y..t.bottom() {
                open[y as usize * vw + t.x as usize..y as usize * vw + t.right() as usize]
                    .fill(false);
            }
        }
        let variants = self.variants_of(w);
        let thick = self.border_of(w).spec.thickness();
        let outer: Vec<(u16, u16)> = variants
            .iter()
            .map(|v| (v.size[0] + 2 * thick, v.size[1] + 2 * thick))
            .collect();
        let (gx, gy) = self.cfg.gap;
        let (vw16, vh16) = (look.view.w, look.view.h);
        let mut best: Option<(Rank, Place)> = None;
        for r in rooms::rooms(&open, vw, vh) {
            // The gap is kept from all but the edges of the view.
            let gl = if r.x == 0 { 0 } else { gx };
            let gr = if r.right() == vw16 { 0 } else { gx };
            let gt = if r.y == 0 { 0 } else { gy };
            let gb = if r.bottom() == vh16 { 0 } else { gy };
            if r.w < gl + gr || r.h < gt + gb {
                continue;
            }
            let (aw, ah) = (r.w - gl - gr, r.h - gt - gb);
            let target = w.fill * (r.w as f32 * r.h as f32);
            let pick = outer
                .iter()
                .enumerate()
                .filter(|(_, (ow, oh))| *ow <= aw && *oh <= ah)
                .min_by(|(i, (aw_, ah_)), (j, (bw, bh))| {
                    let (a, b) = (*aw_ as u32 * *ah_ as u32, *bw as u32 * *bh as u32);
                    let (da, db) = ((a as f32 - target).abs(), (b as f32 - target).abs());
                    da.total_cmp(&db).then(b.cmp(&a)).then(i.cmp(j))
                });
            let Some((i, &(ow, oh))) = pick else {
                continue;
            };
            let at = (r.x + gl + (aw - ow) / 2, r.y + gt + (ah - oh) / 2);
            let dist = prefer_distance(w.prefer, at, (ow, oh), (vw16, vh16));
            let key = (
                Reverse(ow as u32 * oh as u32),
                dist,
                Reverse(r.w as u32 * r.h as u32),
                r.y,
                r.x,
            );
            if best.as_ref().is_none_or(|(k, _)| key < *k) {
                best = Some((
                    key,
                    Place {
                        variant: i,
                        at,
                        outer: (ow, oh),
                    },
                ));
            }
        }
        best.map(|(_, p)| p)
    }

    /// Lay the widgets out, the oldest first: those to place, those
    /// evicted, those that can grow.
    fn look(
        &mut self,
        now: Duration,
        base: &Grid,
        cursor: Option<(u16, u16)>,
        surfaces: &dyn Surfaces,
    ) {
        let mut looks: Vec<Look> = Vec::new();
        let mut soonest: Option<f32> = None;
        for i in 0..self.widgets.len() {
            let w = &self.widgets[i];
            if !w.live {
                continue;
            }
            let Some(info) = w.frame.and_then(|id| surfaces.info(id)) else {
                continue;
            };
            let evicted = w.pressed.is_some_and(|t| now >= t + ms(self.cfg.evict_ms));
            let fresh = w.place.is_none() || w.relayout || evicted;
            if !fresh && w.pressed.is_some() {
                // Pressed, not yet evicted: it stays.
                continue;
            }
            if !looks.iter().any(|l| l.info.id == info.id) {
                let heat = surfaces.heat(info.id, now);
                let (look, left) = self.open_cells(info, base, cursor, heat.as_ref());
                if let Some(left) = left {
                    soonest = Some(soonest.map_or(left, |s: f32| s.min(left)));
                }
                looks.push(look);
            }
            let look = looks.iter().find(|l| l.info.id == info.id).expect("looked");
            // Every other widget placed is an obstacle, in this view's
            // cells.
            let taken: Vec<Rect> = self
                .widgets
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .filter_map(|(_, o)| {
                    let rect = o.screen?;
                    o.place?;
                    obstacle(rect, look.view)
                })
                .collect();
            let found = self.best(&self.widgets[i], look, &taken);
            let view = look.view;
            let w = &mut self.widgets[i];
            if fresh {
                if found != w.place {
                    w.rev += 1;
                }
                w.place = found;
                w.pressed = None;
                w.grow = None;
                w.relayout = false;
            } else if let (Some(place), Some(new)) = (w.place, found) {
                let area = |p: &Place| p.outer.0 as u32 * p.outer.1 as u32;
                if area(&new) > area(&place) {
                    match w.grow {
                        Some((cand, since)) if cand == new => {
                            if now >= since + ms(self.cfg.replace_ms) {
                                w.place = Some(new);
                                w.grow = None;
                                w.rev += 1;
                            }
                        }
                        _ => w.grow = Some((new, now)),
                    }
                } else {
                    w.grow = None;
                }
            } else {
                w.grow = None;
            }
            w.screen = w
                .place
                .map(|p| [view.x + p.at.0, view.y + p.at.1, p.outer.0, p.outer.1]);
        }
        self.quiet_due = soonest.map(|s| now + Duration::from_secs_f32(s).max(ms(1)));
    }

    /// Bring the sprites in line with the places.
    fn sync(&mut self, now: Duration, surfaces: &dyn Surfaces, scene: &mut Scene) {
        for i in 0..self.widgets.len() {
            let w = &self.widgets[i];
            let info = w.frame.and_then(|id| surfaces.info(id));
            let screen = match (w.place, info) {
                (Some(p), Some(info)) => Some([
                    info.rect.x + p.at.0,
                    info.rect.y + p.at.1,
                    p.outer.0,
                    p.outer.1,
                ]),
                _ => None,
            };
            let want = match (w.place, w.frame) {
                (Some(p), Some(frame)) => Some(Drawn {
                    frame,
                    at: p.at,
                    layer: w.layer.0,
                    rev: w.rev,
                }),
                _ => None,
            };
            if want == w.drawn && w.sprite.is_some() == want.is_some() {
                self.widgets[i].screen = screen;
                continue;
            }
            let content = want.map(|_| self.content_of(w));
            let w = &mut self.widgets[i];
            w.screen = screen;
            match (want, w.sprite) {
                (None, Some(id)) => {
                    scene.remove(id);
                    w.sprite = None;
                }
                (None, None) => {}
                (Some(d), sprite) => {
                    let anchor = Anchor::At {
                        at: [d.at.0 as i64, d.at.1 as i64],
                        space: Space::View,
                    };
                    let updated = sprite.is_some_and(|id| {
                        let old = w.drawn;
                        let patch = SpritePatch {
                            frame: (old.map(|o| o.frame) != Some(d.frame))
                                .then_some(FrameSel::Id(d.frame)),
                            anchor: Some(anchor.clone()),
                            content: (old.map(|o| o.rev) != Some(d.rev))
                                .then(|| content.clone().expect("content")),
                            layer: Some(Layer(d.layer)),
                            ..SpritePatch::default()
                        };
                        scene.update(id, patch, now).is_ok()
                    });
                    if !updated {
                        let spec = SpriteSpec {
                            frame: FrameSel::Id(d.frame),
                            anchor,
                            content,
                            layer: Some(Layer(d.layer)),
                            ..SpriteSpec::default()
                        };
                        w.sprite = scene.add(w.owner, spec, now).ok();
                    }
                }
            }
            w.drawn = if w.sprite.is_some() { want } else { None };
        }
    }

    /// The sprite content of a placed widget.
    fn content_of(&self, w: &Widget) -> Content {
        let variants = self.variants_of(w);
        let place = w.place.expect("placed");
        let v = &variants[place.variant];
        let title = w.title.clone().unwrap_or_else(|| w.tag.clone());
        compose(
            &self.border_of(w).spec,
            &title,
            v.content.as_ref(),
            v.size[0],
            v.size[1],
        )
    }
}

/// The screen rect `rect` in the cells of `view`, if it meets it.
fn obstacle(rect: [u16; 4], view: Rect) -> Option<Rect> {
    let [x, y, w, h] = rect;
    let r = Rect::new(x, y, w, h).intersect(&view);
    (!r.is_empty()).then(|| Rect::new(r.x - view.x, r.y - view.y, r.w, r.h))
}

/// How far a rect is from the point `prefer` names in a `vw x vh` view,
/// each axis scaled by the view's size; 0 for `any`.
fn prefer_distance(prefer: Prefer, at: (u16, u16), outer: (u16, u16), view: (u16, u16)) -> i64 {
    let (fx, fy) = match prefer {
        Prefer::TopLeft => (0, 0),
        Prefer::Top => (1, 0),
        Prefer::TopRight => (2, 0),
        Prefer::Left => (0, 1),
        Prefer::Center => (1, 1),
        Prefer::Right => (2, 1),
        Prefer::BottomLeft => (0, 2),
        Prefer::Bottom => (1, 2),
        Prefer::BottomRight => (2, 2),
        Prefer::Any => return 0,
    };
    let (vw, vh) = (view.0 as i64, view.1 as i64);
    // Doubled: the centre of the rect, the point in the view.
    let cx = 2 * at.0 as i64 + outer.0 as i64;
    let cy = 2 * at.1 as i64 + outer.1 as i64;
    (cx - fx * vw).abs() * vh + (cy - fy * vh).abs() * vw
}
