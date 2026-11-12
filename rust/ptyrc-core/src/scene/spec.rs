// SPDX-License-Identifier: GPL-3.0-or-later
//! Wire form of sprites. See `plan/design.md`, "ptyrc-core: scene".

use crate::types::{FrameId, FrameSel, OwnerId, Prefer, SpriteId};
use ptyrc_vt::{Attrs, Color, Rgb};
use serde::{Deserialize, Serialize};

/// A colour on the wire: a name (`red`, `bright-red`, `default`), an index
/// `0..255`, or `"#rrggbb"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ColorRepr", into = "ColorRepr")]
pub struct ColorSpec(pub Color);

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum ColorRepr {
    Index(u8),
    Name(String),
}

const NAMES: [&str; 8] = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
];

impl std::str::FromStr for ColorSpec {
    type Err = String;

    fn from_str(s: &str) -> Result<ColorSpec, String> {
        let name = s.trim().to_ascii_lowercase();
        if name == "default" {
            return Ok(ColorSpec(Color::Default));
        }
        if let Some(hex) = name.strip_prefix('#') {
            if hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                let v = u32::from_str_radix(hex, 16).map_err(|e| e.to_string())?;
                return Ok(ColorSpec(Color::Rgb(
                    (v >> 16) as u8,
                    (v >> 8) as u8,
                    v as u8,
                )));
            }
            return Err(format!("bad colour: {s}"));
        }
        if let Ok(n) = name.parse::<u8>() {
            return Ok(ColorSpec(Color::Indexed(n)));
        }
        if name == "gray" || name == "grey" {
            return Ok(ColorSpec(Color::Indexed(8)));
        }
        let (bright, base) = match name.strip_prefix("bright") {
            Some(rest) => (8, rest.trim_start_matches(['-', '_'])),
            None => (0, name.as_str()),
        };
        match NAMES.iter().position(|n| *n == base) {
            Some(i) => Ok(ColorSpec(Color::Indexed(bright + i as u8))),
            None => Err(format!("bad colour: {s}")),
        }
    }
}

impl TryFrom<ColorRepr> for ColorSpec {
    type Error = String;

    fn try_from(repr: ColorRepr) -> Result<ColorSpec, String> {
        match repr {
            ColorRepr::Index(n) => Ok(ColorSpec(Color::Indexed(n))),
            ColorRepr::Name(s) => s.parse(),
        }
    }
}

impl From<ColorSpec> for ColorRepr {
    fn from(c: ColorSpec) -> ColorRepr {
        match c.0 {
            Color::Default => ColorRepr::Name("default".into()),
            Color::Indexed(n) if n < 8 => ColorRepr::Name(NAMES[n as usize].into()),
            Color::Indexed(n) if n < 16 => {
                ColorRepr::Name(format!("bright-{}", NAMES[n as usize - 8]))
            }
            Color::Indexed(n) => ColorRepr::Index(n),
            Color::Rgb(r, g, b) => ColorRepr::Name(format!("#{r:02x}{g:02x}{b:02x}")),
        }
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_true(b: &bool) -> bool {
    *b
}

fn yes() -> bool {
    true
}

fn one() -> f32 {
    1.0
}

/// The default colours of the host terminal: what `Color::Default`
/// shows. Used to blend a sprite with `alpha`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostColors {
    pub fg: Rgb,
    pub bg: Rgb,
}

impl Default for HostColors {
    /// Light grey on black.
    fn default() -> HostColors {
        HostColors {
            fg: [0xe5, 0xe5, 0xe5],
            bg: [0, 0, 0],
        }
    }
}

/// Style of sprite cells. An absent colour inherits the cell below.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fg: Option<ColorSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<ColorSpec>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub dim: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub underline: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub inverse: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub strike: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub blink: bool,
}

impl StyleSpec {
    /// Foreground only.
    pub fn fg(color: Color) -> StyleSpec {
        StyleSpec {
            fg: Some(ColorSpec(color)),
            ..StyleSpec::default()
        }
    }

    pub fn attrs(&self) -> Attrs {
        let mut a = Attrs::default();
        for (on, bit) in [
            (self.bold, Attrs::BOLD),
            (self.dim, Attrs::DIM),
            (self.italic, Attrs::ITALIC),
            (self.underline, Attrs::UNDERLINE),
            (self.inverse, Attrs::INVERSE),
            (self.strike, Attrs::STRIKE),
            (self.blink, Attrs::BLINK),
        ] {
            if on {
                a.insert(bit);
            }
        }
        a
    }
}

/// A run of text with its own style, for `content.cells`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub t: String,
    #[serde(flatten)]
    pub style: StyleSpec,
}

/// What a sprite shows: `rows` with one `style`, or `cells`.
///
/// In `rows` a space is transparent unless `opaque`. In `cells` a space is
/// transparent unless `opaque` or its run sets `bg`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Content {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cells: Option<Vec<Vec<Run>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<StyleSpec>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub opaque: bool,
}

impl Content {
    pub fn rows<S: AsRef<str>>(rows: &[S]) -> Content {
        Content {
            rows: Some(rows.iter().map(|r| r.as_ref().to_string()).collect()),
            ..Content::default()
        }
    }

    pub fn styled(mut self, style: StyleSpec) -> Content {
        self.style = Some(style);
        self
    }
}

fn default_ms() -> u64 {
    100
}

/// One frame of an animation, shown for `ms` (default 100).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimFrame {
    #[serde(flatten)]
    pub content: Content,
    #[serde(default = "default_ms")]
    pub ms: u64,
}

/// `true`: forever. `false`: once. A count: that many times.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LoopSpec {
    Flag(bool),
    Count(u32),
}

impl Default for LoopSpec {
    fn default() -> LoopSpec {
        LoopSpec::Flag(false)
    }
}

impl LoopSpec {
    /// Number of plays; `None`: forever.
    pub fn plays(self) -> Option<u32> {
        match self {
            LoopSpec::Flag(true) => None,
            LoopSpec::Flag(false) => Some(1),
            LoopSpec::Count(n) => Some(n.max(1)),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Space {
    #[default]
    Frame,
    View,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    #[default]
    Right,
    Left,
    Above,
    Below,
    Over,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoAnchor {
    #[serde(default)]
    pub prefer: Prefer,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CursorAnchor {
    #[serde(default)]
    pub dx: i32,
    #[serde(default)]
    pub dy: i32,
}

/// Where a sprite sits. Default: `{"at": [0, 0], "space": "view"}`; `space`
/// alone defaults to `frame`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Anchor {
    /// Fixed in the frame (scrolls with it) or in the view. Negative `view`
    /// coordinates count from the right and bottom edges: `-1` puts the
    /// sprite against the edge.
    At {
        at: [i64; 2],
        #[serde(default)]
        space: Space,
    },
    /// Next to the `nth` match. `gap` defaults to 1 for `left` and `right`,
    /// 0 otherwise. `left` and `right` centre the sprite vertically on the
    /// match, `above` and `below` horizontally. `slide` (the default):
    /// while the row of the match is in the view, the sprite moves along
    /// that centred axis to stay whole inside the view.
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "is_false")]
        regex: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        ignore_case: bool,
        #[serde(default)]
        nth: usize,
        #[serde(default)]
        side: Side,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        gap: Option<u16>,
        #[serde(default = "yes", skip_serializing_if = "is_true")]
        slide: bool,
    },
    Auto {
        auto: AutoAnchor,
    },
    Cursor {
        cursor: CursorAnchor,
    },
}

impl Default for Anchor {
    fn default() -> Anchor {
        Anchor::At {
            at: [0, 0],
            space: Space::View,
        }
    }
}

/// How a sprite cell meets what is below it. `normal`: it replaces it.
/// `tint`: the glyph below stays, the style is applied.
///
/// `over` and `behind` are the names of protocol drafts: `normal`, with
/// the layer `front` or `back` when the sprite gives none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Blend {
    #[default]
    Normal,
    Tint,
    Over,
    Behind,
}

impl Blend {
    /// Without the draft names.
    pub fn canonical(self) -> Blend {
        match self {
            Blend::Tint => Blend::Tint,
            _ => Blend::Normal,
        }
    }

    /// Layer of a sprite that gives none: a tint recolours the app's
    /// text, anything else is drawn behind it.
    pub fn default_layer(self) -> i32 {
        match self {
            Blend::Tint | Blend::Over => Layer::FRONT.0,
            Blend::Normal | Blend::Behind => Layer::BACK.0,
        }
    }
}

/// Place in the stack. The app is layer 0: negative layers are behind its
/// text, positive ones in front. On the wire an integer other than 0, or
/// a name: `"wallpaper"` (-100), `"back"` (-1), `"front"` (1), `"hud"`
/// (100).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "LayerRepr", into = "i32")]
pub struct Layer(pub i32);

impl Layer {
    pub const BACK: Layer = Layer(-1);
    pub const FRONT: Layer = Layer(1);
    /// Behind every drawing of `back`: a static background.
    pub const WALLPAPER: Layer = Layer(-100);
    /// In front of every drawing of `front`: displays.
    pub const HUD: Layer = Layer(100);

    /// The named layers, back to front. Integers between two names are
    /// layers between them.
    pub const NAMES: [(&'static str, Layer); 4] = [
        ("wallpaper", Layer::WALLPAPER),
        ("back", Layer::BACK),
        ("front", Layer::FRONT),
        ("hud", Layer::HUD),
    ];
}

#[derive(Deserialize)]
#[serde(untagged)]
enum LayerRepr {
    Index(i32),
    Name(String),
}

impl std::str::FromStr for Layer {
    type Err = String;

    fn from_str(s: &str) -> Result<Layer, String> {
        let s = s.trim();
        if let Some((_, layer)) = Layer::NAMES.iter().find(|(name, _)| *name == s) {
            return Ok(*layer);
        }
        match s {
            "app" => Err("the layer app is the app's: use a layer behind it or in front".into()),
            other => match other.parse::<i32>() {
                Ok(0) => Err("layer 0 is the app: use a negative or a positive layer".into()),
                Ok(n) => Ok(Layer(n)),
                Err(_) => Err(format!(
                    "bad layer: {s} (wallpaper, back, front, hud, or an integer other than 0)"
                )),
            },
        }
    }
}

impl TryFrom<LayerRepr> for Layer {
    type Error = String;

    fn try_from(repr: LayerRepr) -> Result<Layer, String> {
        match repr {
            LayerRepr::Index(n) => n.to_string().parse(),
            LayerRepr::Name(s) => s.parse(),
        }
    }
}

impl From<Layer> for i32 {
    fn from(layer: Layer) -> i32 {
        layer.0
    }
}

/// Velocity (cells per second) and acceleration added to the anchor.
/// `bounce`: reflect on the edges of the view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Motion {
    #[serde(default)]
    pub vx: f64,
    #[serde(default)]
    pub vy: f64,
    #[serde(default)]
    pub ax: f64,
    #[serde(default)]
    pub ay: f64,
    #[serde(default)]
    pub bounce: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectSpec {
    pub name: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// A sprite as a client sends it. Exactly one of `content`, `frames`,
/// `effect`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SpriteSpec {
    #[serde(default)]
    pub frame: FrameSel,
    #[serde(default)]
    pub anchor: Anchor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frames: Option<Vec<AnimFrame>>,
    #[serde(default, rename = "loop")]
    pub looping: LoopSpec,
    /// Absent: `back`; `front` for a tint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<Layer>,
    #[serde(default)]
    pub blend: Blend,
    /// The sprite is on the app's panel of that backdrop: drawn in its
    /// cells only; behind the app, its opaque `bg` replaces the colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<ColorSpec>,
    /// Opacity, `0..=1`. Absent: opaque, colours are written as given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpha: Option<f32>,
    #[serde(default)]
    pub z: i32,
    #[serde(default)]
    pub ttl_ms: Option<u64>,
    #[serde(default)]
    pub persist: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<Motion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<EffectSpec>,
    /// Canvas of an `effect`, `[w, h]`. Absent: the view of the frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<[u16; 2]>,
}

/// Changes to a sprite; absent fields are kept. `content` or `frames`
/// replace what is shown and restart the animation; `ttl_ms` counts from
/// the update; `motion` restarts from the current position.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SpritePatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<FrameSel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<Anchor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frames: Option<Vec<AnimFrame>>,
    #[serde(default, rename = "loop", skip_serializing_if = "Option::is_none")]
    pub looping: Option<LoopSpec>,
    /// The running effect of that name is given the `params` and keeps
    /// its state; another effect starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<EffectSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<Layer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blend: Option<Blend>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<ColorSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpha: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persist: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<Motion>,
}

/// A sprite as `draw.list` reports it. `frame` is null until a frame name
/// is resolved; `screen` is the top-left cell at the last tick, null while
/// hidden.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpriteInfo {
    pub id: SpriteId,
    pub owner: OwnerId,
    pub frame: Option<FrameId>,
    pub anchor: Anchor,
    pub kind: String,
    pub size: [u16; 2],
    /// Style of the content, of the first frame of an animation, of the
    /// first run of `cells`; absent for an effect or when none is given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<StyleSpec>,
    pub layer: i32,
    pub blend: Blend,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<ColorSpec>,
    #[serde(default = "one")]
    pub alpha: f32,
    pub z: i32,
    pub persist: bool,
    pub screen: Option<[i64; 2]>,
    /// A glyph of the sprite is inside the view of a usable frame.
    /// Always false from `Scene::list`.
    #[serde(default)]
    pub visible: bool,
    /// Back layers: of its glyphs inside the view, the share under app
    /// cells with ink, where they do not show. 0 for a front layer.
    #[serde(default)]
    pub covered: f32,
}

/// Scene failure; `code` is the wire error code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SceneError {
    NoFrame(String),
    NoSprite(SpriteId),
    BadParams(String),
}

impl SceneError {
    pub fn code(&self) -> &'static str {
        match self {
            SceneError::NoFrame(_) => "no-frame",
            SceneError::NoSprite(_) => "no-sprite",
            SceneError::BadParams(_) => "bad-params",
        }
    }
}

impl std::fmt::Display for SceneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SceneError::NoFrame(name) => write!(f, "no such frame: {name}"),
            SceneError::NoSprite(id) => write!(f, "no such sprite: {id}"),
            SceneError::BadParams(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for SceneError {}
