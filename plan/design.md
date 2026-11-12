# Design

Contract shared by all packages. `plan/intent.md` is the owner's brief,
`plan/interview.md` the decisions taken with the owner, `plan/ideas.md` the
backlog. Names in this file are binding; signatures may grow.

## Layout

```
rust/ptyrc-vt/       lib  terminal model: bytes -> Grid; repair painter
rust/ptyrc-core/     lib  frames (raw, extended, dynamic), heatmap, pool,
                          placement, scene (sprites, animations, effects),
                          plugin traits
rust/ptyrc-proto/    lib  wire envelope, framing, blocking client
rust/ptyrc-server/   lib  the driver: pty, event loop, socket, RPC dispatch
rust/ptyrc-cli/      bin  `ptyrc <verb>`; links the plugins in
plugins/tui-support/<name>/   Rust crate `ptyrc-tui-<name>`: per-app presets
plugins/draw-ext/<name>/      Rust crate `ptyrc-draw-<name>`: draw functions
plugins/client-apps/python/   Python package `ptyrc-apps`: confetti, clock...
python/ptyrc/        Python pilot library, `import ptyrc`
scripts/             development and demo scripts
plan/                intent, design, interview, ideas
```

One cargo workspace and one uv workspace at the root. Dependency direction:

```
ptyrc-cli    -> ptyrc-server, ptyrc-proto, every plugin crate (clap)
ptyrc-server -> ptyrc-core, ptyrc-vt, ptyrc-proto (rustix, signal-hook, libc)
plugins      -> ptyrc-core
ptyrc-core   -> ptyrc-vt (serde, serde_json, regex)
ptyrc-proto  -> serde, serde_json
ptyrc-vt     -> unicode-width, smol_str
ptyrc-apps   -> ptyrc (Python, stdlib only)
```

## Vocabulary

- **Driver / server**: `ptyrc run`, the process that owns the pty of the
  wrapped app and sits between it and the real terminal.
- **Pilot / client**: anything speaking the protocol on the driver's socket.
- **Host**: the real terminal the driver writes to.
- **App**: the wrapped program.
- **Grid**: a `cols x rows` array of cells. Coordinates are 0-based,
  `x`/`col` to the right, `y`/`row` downwards, everywhere (API, CLI, wire).
- **Frame**: a drawable surface with its own coordinate space.
- **View**: the rectangle of the screen that shows part of a frame; `origin`
  is the frame position of its top-left cell.
- **Sprite**: a drawing owned by a client, attached to a frame by an anchor.
- **Burst**: the app output coalesced by the driver into one host write.

## Rendering: pass-through and repair

App bytes reach the host unmodified. With no sprite the driver writes
nothing of its own: it is transparent.

The driver holds two `Vt`:

- `app`: fed with app bytes only. What the app means to show.
- `host`: fed with every byte written to the host, ours included. What the
  host shows. It is kept only while sprites are up: copied from `app` when
  the first sprite appears.

Invariant at every injection point: `host` cursor, pen and modes equal
`app`'s; `host.grid()` equals the desired grid (app grid + sprites).

Per burst:

1. `app.feed(burst)`; take its events.
2. If the burst scrolled lines of the main screen into the host scrollback
   and sprites are painted, first emit `repair(host, base)`, `base` being
   the app grid before the burst: sprites never enter the scrollback.
3. Emit the burst; `host.feed(burst)`.
4. `frames.update(...)`, `scene.tick(...)`,
   `desired = scene.compose(app.grid(), frames)`.
5. If `app.in_ground()`: emit `repair(host, desired)`. Otherwise wait for
   the next burst: never inject inside an escape sequence or a UTF-8
   sequence.
6. One `write` to the host. `base = app.grid().clone()`.

A timer tick (animation, pool promotion) runs steps 4 to 6 alone.

`repair` paints only cells where `host.grid() != desired`, then restores
the state it found: cursor position, pending wrap (by reprinting the last
column cell), pen, hyperlink, charset, insert mode, origin mode. It does
not use DECSC/DECRC (one slot, the app may own it). It feeds its own
output to `host`.

A burst and its repair are one synchronized update (`CSI ? 2026 h/l`): the
engine opens it before the burst and closes it after the repair. When the
app closes its own update with the burst, the repair goes before the
app's closing sequence.

Coalescing: after a read the driver keeps reading while the pty is
readable, then polls up to `coalesce_ms` (default 1) for more, bounded by
8 ms and 1 MiB. A burst under 32 bytes is written at once (key echo).
`--transparent` does not coalesce. A whole app redraw and the sprite
motion it implies go out in one write.

Resize: both `Vt` are cropped or padded alike, without reflow; the host is
repaired from there; every frame returns to the pool. A main screen the
host reflows is out of reach of both models until the app redraws.

Exit: CAN, sprites unpainted, any open update closed, the host termios
restored; the exit status is the app's.

The driver is plumbing: a level-triggered `poll(2)` loop over the pty, the
host, the signals, the socket and one deadline. What is drawn and what the
methods answer belongs to an `Engine` (`ptyrc-server/src/engine.rs`):
`ScreenEngine` implements this section; `PassThrough` (`ptyrc run
--transparent`) does nothing.

Headless: `ptyrc run --headless --size 80x24` has no host terminal. The
host stream is discarded, or appended to `--host-log FILE`. Tests and
automation use it.

## ptyrc-vt

```rust
pub enum Color { Default, Indexed(u8), Rgb(u8, u8, u8) }
pub struct Attrs(pub u16);          // BOLD DIM ITALIC UNDERLINE BLINK INVERSE
                                    // HIDDEN STRIKE OVERLINE UL_DOUBLE
                                    // UL_CURLY UL_DOTTED UL_DASHED
pub struct Style { fg, bg, ul: Color, attrs: Attrs, link: u32 }
pub struct Cell { text: SmolStr, width: u8, style: Style }
pub struct Grid;  pub struct Rect { x, y, w, h: u16 }
```

`Cell.width`: 1, 2 (head of a wide glyph) or 0 (its spacer, empty text).
`Grid::put` is the only writer of glyphs: it blanks the orphaned half of a
wide glyph it cuts. The emulator and the compositor both go through it, so
both agree on what a partial overwrite leaves.

```rust
pub struct Vt;
impl Vt {
    pub fn new(cols: u16, rows: u16) -> Vt;
    pub fn feed(&mut self, bytes: &[u8]);
    pub fn resize(&mut self, cols: u16, rows: u16);
    pub fn grid(&self) -> &Grid;                 // the active screen
    pub fn set_grid(&mut self, grid: &Grid);     // host := app after resize
    pub fn cursor(&self) -> Cursor;              // col,row,visible,pending_wrap
    pub fn pen(&self) -> Style;
    pub fn modes(&self) -> Modes;                // alt_screen, origin, autowrap,
                                                 // insert, sync_update,
                                                 // bracketed_paste, app_cursor,
                                                 // mouse, charset state...
    pub fn in_ground(&self) -> bool;
    pub fn title(&self) -> &str;
    pub fn link(&self, id: u32) -> Option<&Link>;
    pub fn take_events(&mut self) -> Vec<VtEvent>;
    pub fn same_state(&self, other: &Vt) -> bool; // cursor, pen, modes
}
pub enum VtEvent {
    Scroll { region: Rect, dy: i16, scrollback: bool },  // dy<0: content up
    AltScreen(bool), Clear, Title, Bell,
}
pub fn repair(host: &mut Vt, desired: &Grid, out: &mut Vec<u8>);
```

The parser is the crate's own (a `Vt` is `Clone`, and `in_ground` is
exact). `repair` also reprints the glyph the cursor follows, so a REP or a
late combining mark from the app still refers to the app's glyph.

Scope: what full-screen programs and shells emit. C0, CSI cursor moves,
ED/EL/ECH, IL/DL/ICH/DCH, SU/SD, DECSTBM, RI/IND/NEL, SGR (16, 256, RGB,
underline styles and colour), DECSC/DECRC, alt screen (47, 1047, 1049),
DECOM, DECAWM, IRM, DECTCEM, charsets (G0/G1, DEC special graphics),
tabs, OSC 0/2 title, OSC 8 hyperlinks, mode 2026, REP, wide and combining
glyphs. Unknown sequences are consumed and ignored. Modes that only
concern input (mouse, bracketed paste, cursor keys) are tracked, not acted
on.

Background and foreground of a cell, the one notion shared by the
frames, the heatmap and the layers:

- `Cell::has_ink()`: foreground. A glyph, or a blank that is underlined,
  struck or overlined. Hidden text is not ink.
- `Cell::backdrop()`: background. The colour that fills the cell: `bg`,
  or `fg` when inverse.
- `Cell::looks_like(&other)`: same on screen. Two cells without ink look
  alike when their backdrops are equal. A clear of a clean area, and an
  identical redraw, change nothing.

Test: `repair` property. For random streams of app bytes and random
desired grids, after `repair` the host grid equals the desired grid and
`host.same_state(app)`.

## ptyrc-core: frames

`Frames` owns every frame. No clock inside: the caller passes `now`
(monotonic `Duration`), tests are deterministic.

```rust
pub struct Frames;
impl Frames {
    pub fn new(cfg: FrameConfig, supports: Vec<Box<dyn TuiSupport>>) -> Frames;
    pub fn update(&mut self, screen: &ScreenInput, now: Duration) -> Vec<FrameEvent>;
    pub fn tick(&mut self, now: Duration) -> Vec<FrameEvent>;
    pub fn next_wake(&self) -> Option<Duration>;
    pub fn list(&self) -> Vec<FrameInfo>;
    pub fn text(&self, id: FrameId, y0: i64, y1: i64) -> Vec<String>;
    pub fn heat(&self, id: FrameId) -> Option<HeatView>;
}
impl Surfaces for Frames { ... }     // what the scene needs, see types.rs
pub struct ScreenInput<'a> {
    grid: &'a Grid, cursor: Cursor, alt_screen: bool, title: &'a str,
    events: &'a [VtEvent],
}
```

### Raw frame

Id 0, the whole screen, always present, `origin = (0, 0)`. Plain text,
find (literal or regex), cell styles, cursor and cursor trail (last 256
`(time, col, row)` samples). Usable 250 ms after a resize or an alt-screen
switch, immediately otherwise.

### Extended frame

A sparse buffer of lines addressed by `i64` frame rows, each line a run of
cells starting at an `i64` frame column, plus the view: screen rect and
origin. Each update compares the previous view content with the new one:

1. **Scroll**: the vertical shift `dy` that aligns the most rows (blank rows
   do not vote, rows unique in the view weigh more). Accepted when it
   beats `dy = 0` clearly and aligns at least `max(2, 30 %)` of the
   non-blank rows. `VtEvent::Scroll` inside the rect seeds the candidates.
   Rows that stay put while the rest shifts are chrome: the scroll band is
   the largest run of rows that moves, learned over time.
   **Feed**: no row aligns, but the emulator counted the scroll. The
   distance is trusted when the rows went into the host scrollback (main
   screen), or when the screen was fed as a tape: one direction, fewer
   lines than the scrolled region has rows, no clear in the last 100 ms.
   Then every line that went through is on screen before or after. A
   pager's page down or up is such a feed (`less`, `man`, `more`: one
   line less than the screen). A pager paints a jump by a clear, by
   cursor addressing, or by feeding a whole screen or more: not a feed.
   Under a feed, the last rows that did not travel are bottom chrome.
2. **Horizontal scroll**: same on columns when rows changed with `dy = 0`.
3. **Splice**: the top aligns at one shift and the bottom at another. `k`
   lines were inserted or deleted at the seam; the buffer is spliced and
   the rows below move.
4. **Jump**: nothing aligns. The new rows are looked up in the buffer by
   line hash; the origin with the most votes wins. Otherwise a line-number
   gutter (increasing integers at the left of consecutive rows), a
   plugin `view_hint`, or a status row gives the absolute row. Otherwise
   a new island is allocated far below the known extent; when a later
   scroll shows an island touching another, they are merged and what is
   fixed on their rows, remembered or only on screen, is translated
   (`shift`).
5. **Edit**: none of the above. Visible lines are rewritten in place.

Status row: `line N` or `lines N-M` on the last row of the pane or the
row below it, under the cursor, starting in standout, on the alternate
screen (`man`, `less -M`). It gives absolute rows and is bottom chrome.
It is the weakest witness: after the gutter and the plugin. When it moves
less than the rows did it counts file lines, not screen rows (wrapped
lines): it is not read any more for that frame.

A hint that says further than a scroll moved, or the other way, makes the
scroll a jump to the hinted row: a jump fed in two bursts looks like two
scrolls.

A pane wiped (one non-blank row or none) starts an island. When the
rows painted next are remembered elsewhere (half of them, three or more),
the view is there and the empty island is given back: a clear and its
repaint may come in two bursts.

Islands: at most 8 per frame; the one shown least recently is forgotten
with its lines. `extent` in the frame info, and `frame.text` without a
range, are those of the island on screen. Rows placed by a hint within
50000 rows of an island belong to it. A splice moves the rows of its own
island only.

The origin follows at once (sprites move in the burst that scrolls).
Content enters the remembered buffer once it has been unchanged for
`commit_ms` (150), so a half-drawn screen is not remembered.

### Dynamic frame

Segments the screen into panes, each with an extended frame.

- **Separators**: vertical runs of border glyphs (`│ ┃ ║ | ╎ ┆ ┊` and
  junctions), horizontal runs (`─ ━ ═ -` and junctions), and bars (a row
  whose background or inverse attribute is uniform across the rect: status
  lines, tab bars).
  A bar is a row that one backdrop, not the default, covers (nine cells
  in ten) while the rows above and below show another. A bar that moves
  without taking its text along is the highlight of a list: bars are not
  believed in that list for `distrust_ms`.
- **Boxes**: closed rectangles of border glyphs with corners; the interior
  is a pane.
- **Panels**: rectangles of one backdrop that is not their surroundings',
  without border, at least a pane in size and narrower than the rect they
  float in (a modal, a prompt box, a sidebar). A row belongs to a panel
  when it shows the panel's colour at both edges and nothing of the
  surrounding backdrop between them. A panel is a pane, as a box.
- **Guillotine**: a rect is split by a separator that spans it; recursion.
  tmux and vim layouts are of this form.
- Leaves below `min_pane` (3 rows, 8 columns) are merged into chrome.

A layout is the set of pane rects. Every pane of a new layout is a
**candidate**; it becomes **stable** after `settle_ms` (default 1500,
range 1000 to 3000) without structural change, and only then is usable for
drawing. A separator that moves with a detected scroll was content: the
pane is demoted and merged back.

Identity. Known layouts and their frames are kept when they leave the
screen (**inactive**). A pane of a new layout is matched against inactive
frames of the same rect: plugin `frame_key` first, then the share of view
rows found in the remembered buffer. Above the threshold the frame is
attached again, with its sprites; below, a new one is allocated. A
whole-pane change with no alignment, no buffer hit and no matching inactive
frame is a jump when the chrome around it is unchanged, a new frame when
it changed too.

A segmentation that differs from the current one first only suspends the
panes it touches (not usable, same burst). It replaces the layout after
`flap_ms` (100); if the old one returns sooner the panes resume without
penalty. A resize commits at once.

Frame events: `layout`, `added`, `attached`, `stable`, `inactive`,
`dropped`, `scroll {frame, dx, dy}`, `shift {frame, from, to, dy}`, `jump`,
`rect`. After a `shift` (splice, island merge, renumbering) the scene
translates what is fixed on those frame rows. A `scroll` needs nothing:
the origin has moved.

Further rules, from real apps:

- The alternate screen has its own frames; those of the main screen come
  back when it is left.
- A key given by a preset for the whole screen (a tmux window) prefixes
  the keys of its panes. Presets are looked for when two rows or more
  changed, when the title or the layout changed, else once a second; a
  preset found is held 4 s and asked (chrome, hint, key) on every change.
  A preset that knows the whole screen may segment on any change.
- When nothing aligns, the emulator's scroll counts are trusted only if
  rows entered the host scrollback (a shell printing a lot at once).
- A pane replaced whole twice within 3 s is a monitor (htop): rewritten
  in place from then on, no island.
- A hinted view that contradicts what is remembered is another document:
  another frame; the first is found again by content.
- `find`: a literal is searched in the row as it is; for a regular
  expression trailing blanks are not text.

### Heatmap

Per frame, in view coordinates, per cell:

- `activity`: EWMA of change events, time constants 2 s and 60 s. An
  event is a visible change: the cell does not look as it did at the last
  update (`Cell::looks_like`). Being written is not an event: a clear of a
  clean area, an identical redraw, another pen on a blank count for
  nothing. A change undone within 150 ms never happened (a clear in one
  burst, the same text again in the next).
- `occupancy`: EWMA of foreground (`Cell::has_ink`), 5 s. A background
  colour does not occupy.
- `still`: seconds without a visible change, from 0 when the frame is
  first watched or resized.
- `safe = (1 - activity) * (1 - occupancy)`. Exposed as digits 0 to 9.

The backdrop of a frame is the `Cell::backdrop` most of its cells have.
Class of a cell, exposed as a letter:

| class | letter | meaning |
|---|---|---|
| `hot` | `h` | changed on screen within 10 s |
| `text` | `t` | foreground that does not move |
| `panel` | `p` | no foreground, a backdrop other than the frame's |
| `free` | `.` | no foreground, the frame's backdrop |

A panel is background of another colour: not occupied, as safe as a free
cell.

Idle: `score` is the share of the view's cells that are not hot and have
`still >= 10 s`; text that does not move is static. `idle` is `score >=
0.8`. `free` is the share static and without foreground. All three are
computed at the time of the request: time passes without a tick.

`debug overlay heat`: red for hot (brighter when busier), light blue for
static text, green for free, violet for a panel; the last three brighten
until the cell has been still for 10 s.

### Placement

`place(frame, w, h, prefer, taken)`: the `w x h` rect of the view with the
best mean `safe`, plus a bias for `prefer` (nine positions and `any`),
never overlapping `taken`. A placed sprite moves only when another spot is
better by a margin for `replace_ms` (2000): no jitter.

### Pool and gating

A sprite is drawn only on a usable frame. A structural change hides the
sprites of the frames it touches in the same burst; they return when the
frame, or the frame that inherits its identity, is stable again.

## ptyrc-core: scene

```rust
pub struct Scene;
impl Scene {
    pub fn add(&mut self, owner: OwnerId, spec: SpriteSpec, now: Duration)
        -> Result<SpriteId, SceneError>;
    pub fn update(&mut self, id: SpriteId, patch: SpritePatch, now: Duration)
        -> Result<(), SceneError>;
    pub fn remove(&mut self, id: SpriteId) -> bool;
    pub fn clear(&mut self, owner: Option<OwnerId>);
    pub fn drop_owner(&mut self, owner: OwnerId);   // keeps `persist` sprites
    pub fn list(&self) -> Vec<SpriteInfo>;
    pub fn tick(&mut self, now: Duration, surfaces: &dyn Surfaces);
    pub fn next_wake(&self) -> Option<Duration>;
    pub fn compose(&self, base: &Grid, surfaces: &dyn Surfaces) -> Grid;
    pub fn register_effect(&mut self, name: &str, make: EffectFactory);
}
```

Sprite, as JSON (the wire form; serde on the core types):

```json
{
  "frame": 0,
  "anchor": {"at": [x, y], "space": "frame"},
  "content": {"rows": ["text", "..."], "style": {"fg": "red"}},
  "frames": [{"rows": ["..."], "ms": 100}],
  "loop": true,
  "layer": "back",
  "blend": "normal",
  "z": 0,
  "ttl_ms": null,
  "persist": false,
  "motion": {"vx": 0.0, "vy": 0.0, "ax": 0.0, "ay": 0.0, "bounce": false},
  "effect": {"name": "confetti", "params": {}}
}
```

- Defaults: `frame` raw; `anchor` `{"at": [0, 0], "space": "view"}` (`at`
  given without `space` means `frame`); `loop` false: an animation plays
  once and the sprite is removed; frame `ms` 100.
- `size: [w, h]`: canvas of an effect; default the frame's view.
- `frame`: an id, or `"raw"`, `"active"` (pane holding the cursor),
  `"largest"`, `"quietest"` (least mean activity), `"idle"` (highest idle
  score, then most free, then largest). A name is resolved when the sprite is added.
- `anchor`, one of:
  - `{"at": [x, y], "space": "frame" | "view"}`: fixed in the frame (moves
    with scrolling) or in the view (HUD). Negative `view` coordinates count
    from the right and bottom edges.
  - `{"text": "flower", "regex": false, "nth": 0, "side": "right" | "left"
    | "above" | "below" | "over", "gap": 1}`: next to a match, re-resolved
    when the frame content changes; hidden while there is no match.
  - `{"auto": {"prefer": "top-right"}}`: placement by heatmap.
  - `{"cursor": {"dx": 1, "dy": 0}}`: follows the cursor.
- `content`: `rows` (strings; a space is transparent unless `"opaque":
  true`) with one `style`, or `cells`: rows of runs `[{"t": "ab", "fg":
  ..., ...}]` for per-run styles. `frames` instead of `content` for an
  animation; `loop`: `true`, `false` or a count; the sprite is removed
  after the last frame of a finite animation.
- `style`: `fg`, `bg` (colour name, `0..255`, `"#rrggbb"`, `"default"`;
  absent: inherit the cell below), `bold`, `dim`, `italic`, `underline`,
  `inverse`, `strike`, `blink`.
- `layer`: an integer other than 0, or `"back"` (-1), `"front"` (1). See
  Layers. Absent: `back`; `front` for a tint.
- `blend`: `normal`; `tint` (keeps the glyph below, applies the style).
  `over` and `behind`, names of the first draft, are read as `normal`
  with the layer `front` or `back` when none is given.
- `z`: order inside a layer.
- `effect`: a native generator in place of `content`; it owns the sprite's
  cells every tick. Built in: `confetti`. Plugins add more.
- A sprite belongs to the connection that added it and dies with it unless
  `persist`. `ttl_ms` bounds its life in any case.

Sprites are clipped to the view rect of their frame. `compose` goes through
`Grid::put`.

A finite animation and an effect start their clock when the sprite is
first drawn: added on a frame that is not usable yet, or on a text that
is not there yet, they wait. `ttl_ms` counts from the add in any case.

### Layers

A sandwich. The app is layer 0. Sprites of negative layers are behind
it, sprites of positive layers in front. Paint order is `(layer, z,
id)`. The default is behind: the app's text stays in front of what is
drawn.

Background and foreground are those of `ptyrc-vt`: `Cell::has_ink()`,
`Cell::backdrop()`.

Back layers are flattened among themselves first (a nearer glyph
replaces a farther one; a glyph without `bg` keeps the colour of the
layers behind it), then seen through the app, cell by cell:

- A back glyph shows where the app cell has no ink. A wide glyph needs
  two such cells with the same resulting style; otherwise it is not
  drawn, by halves neither.
- A back colour (`bg`) shows where the app's backdrop is the default
  one and the cell is not inverse. An app glyph there keeps its text,
  its foreground and its attributes, and stands on the sprite's
  background: a true sandwich. A wide app glyph takes the colour behind
  its head.
- Another backdrop is the app's and wins: a panel of another colour, a
  status bar, inverse video. A back glyph is still seen through its
  cells without ink, on the app's colour; a back `bg` is not seen there.
- A `tint` behind the app is a colour of background only.

With back layers alone, every app cell with ink keeps its glyph, its
foreground and its attributes, and every backdrop the app chose is kept
(tested as a property).

Front layers go over whatever is below, app text included: a space is
transparent unless `opaque`; a glyph without `bg` keeps the colour
below it. A front `tint` recolours the app's glyphs.

The reference apps follow: confetti and sparkles fall behind the text
(`ptyrc-app confetti --front` brings the confetti over it), the clock is
in front.

A scene of four layers:

```
ptyrc draw text --layer -2 --bg 17 --opaque ...    static background
ptyrc draw text --at 4,2 '<*//><'                  a drawing in front of it
                                                   the app's text
ptyrc draw text --layer front --auto top-right ... a display in front of all
```

## Plugins

Rust crates linked into `ptyrc-cli` and registered at startup
(`rust/ptyrc-cli/src/plugins.rs`). The core knows no app.

```rust
pub trait TuiSupport: Send {
    fn name(&self) -> &'static str;
    fn detect(&self, pane: &PaneView) -> f32;             // confidence 0..1
    fn chrome(&self, pane: &PaneView) -> Option<Chrome> { None }
    fn view_hint(&self, pane: &PaneView) -> Option<ViewHint> { None }
    fn frame_key(&self, pane: &PaneView) -> Option<String> { None }
    fn segment(&self, screen: &PaneView) -> Option<Vec<Rect>> { None }
    fn tuning(&self) -> Tuning { Tuning::default() }
}
pub trait DrawExt: Send {
    fn name(&self) -> &'static str;
    fn functions(&self) -> Vec<FnDoc>;
    fn draw(&self, function: &str, params: &serde_json::Value)
        -> Result<SpriteSpec, String>;
    fn effects(&self) -> Vec<(&'static str, EffectFactory)> { vec![] }
}
```

A `TuiSupport` is consulted for a pane (and for the whole screen) when its
`detect` is at least 0.5. First: `vim`, `tmux`, `agent-tui` (the family of
coding-agent TUIs: a prompt box, rounded, between rules or a panel edged
by `┃`, under a streaming transcript). First draw
extensions: `shapes` (line, rect, circle, ellipse), `bigtext` (block
letters).

## Protocol

Unix stream socket `$XDG_RUNTIME_DIR/ptyrc/<session>.sock` (directory 0700,
socket 0600), `/tmp/ptyrc-<uid>/` without `XDG_RUNTIME_DIR`. `<session>` is
`--name` or the driver pid. The driver exports `PTYRC_SESSION` and
`PTYRC_SOCKET` to the app. A client picks, in order: `--session` /
`PTYRC_SESSION`, else the only live session, else fails with the list.

UTF-8 JSON, one object per line. Version 1.

```
server -> {"event":"hello","data":{"protocol":1,"server":"ptyrc/2.0.0",
           "session":"...","pid":1,"argv":["vim"],"size":[80,24]}}
client -> {"id":1,"method":"screen.text","params":{}}
server -> {"id":1,"result":{...}}
server -> {"id":1,"error":{"code":"no-frame","message":"..."}}
server -> {"event":"frames","data":{...}}          after subscribe
```

Error codes: `bad-request`, `unknown-method`, `bad-params`, `no-frame`,
`no-sprite`, `not-usable`, `timeout`, `gone`, `internal`.

An error may carry `"hint"`: what to try, in one line. The valid values
(frames, sprites, key names, colours, anchors, topics, extensions,
effects) and the nearest one (`did you mean active?`). The driver writes
it (`ptyrc-server/src/hints.rs`), so every client reads the same words.

Methods (`frame` is a frame id or name, default `"raw"`):

| method | params | result |
|---|---|---|
| `session.info` | | hello data, `alt_screen`, `title`, `driver_pid`, `headless` |
| `session.resize` | `size`: `[c, r]` | `{}`; headless only |
| `screen.text` | `frame`, `rect`, `ansi` | `{"lines":[...],"size":[c,r],"rect":[x,y,w,h]}` |
| `screen.cells` | `frame`, `rect` | `{"rows":[[run,...],...]}` |
| `screen.find` | `pattern`, `regex`, `ignore_case`, `frame`, `known` | `{"matches":[{"frame","x","y","len","text","screen":[c,r]\|null}]}` |
| `screen.cursor` | `trail` | `{"x","y","visible","frame","trail":[[ms,x,y],...]}` |
| `screen.wait` | `pattern` or `stable_ms` or `gone`, `timeout_ms`, `frame` | `{"matched":bool,"match":...}` |
| `input.send` | `text` or `keys` or `bytes` (base64), `paste` | `{}` |
| `frame.list` | | `{"frames":[info,...],"layout":id}` |
| `frame.text` | `frame`, `y0`, `y1` | `{"lines":[...],"y0":n}` remembered buffer; default: the island on screen |
| `frame.heat` | `frame` | `{"rows":["0123...",...],"classes":["ht.p...",...]}` |
| `frame.idle` | `frame` | `{"score":0..1,"idle":bool,"free":0..1}` |
| `frame.get` | `frame` | `{"frame":info}` |
| `frame.at` | `x`, `y`, or `name` | `{"frame":id,"pos":[x,y]}` |
| `draw.put` | sprite | `{"id":n}` |
| `draw.update` | `id`, patch | `{}` |
| `draw.remove` | `id` | `{}` |
| `draw.clear` | `all` | `{"removed":n}` |
| `draw.list` | | `{"sprites":[...]}` |
| `draw.ext` | `name`, `function`, `params`, sprite fields; with `id`: redraw that sprite in place (content replaced, the rest kept unless given) | `{"id":n}` |
| `draw.exts` | | `{"exts":[{"name","functions":[...]}],"effects":[...]}` |
| `batch` | `calls`: `[{"method","params"}]` | `{"results":[{"result"} or {"error"},...]}`, applied in one repaint |
| `subscribe` | `topics`: `screen`, `cursor`, `frames`, `input`, `size`, `exit` | `{}` |
| `debug.overlay` | `what`: `frames`, `heat`, `off` | `{}` |
| `debug.dump` | | app grid, host grid, sprites, frames |

Frame info: `{"id","kind":"raw"|"pane"|"chrome","state":"candidate"|
"stable"|"inactive","usable","rect":[x,y,w,h],"origin":[x,y],
"extent":[y_min,y_max],"key","support":"vim"|null,"idle":0..1,
"is_idle":bool}`.

Keys for `input.send`: `enter`, `tab`, `esc`, `space`, `backspace`,
`delete`, `up`, `down`, `left`, `right`, `home`, `end`, `pageup`,
`pagedown`, `insert`, `f1`..`f12`, `ctrl+<x>`, `alt+<x>`, `shift+tab`; any
other string is sent as text. Arrow keys follow the app's cursor-key mode.
A string plainly meant as a key and that is not one is refused
(`bad-params`, nothing of the list is sent): a modifier prefix (`ctrl+xx`,
`shift+up`), `f13`, a key name in another case (`Enter`), a common alias
(`return`, `escape`, `pgdn`, `ctrl-c`). The hint names the key meant.
`subscribe` refuses an unknown topic.

## CLI

`ptyrc <verb>`; `-s/--session` on every client verb; `--json` where a human
form exists.

```
ptyrc run [--name N] [--headless --size CxR] [--host-log F]
          [--transparent] [--no-presets] [--coalesce-ms N] [--] CMD...
ptyrc ls                          sessions
ptyrc text [-f FRAME] [--ansi]    screen as plain text
ptyrc find [-E] [-i] PATTERN      matches with coordinates
ptyrc cursor
ptyrc send TEXT                   type text
ptyrc keys KEY...                 press keys
ptyrc wait [PATTERN] [--stable MS] [--gone] [--timeout MS]
ptyrc frames [--watch]
ptyrc heat [-f FRAME] [--classes]
ptyrc idle [-f FRAME]             score; status 0 when idle, 1 when not
ptyrc draw text|ext|effect|exts|rm|ls|clear ...   (sprites persist)
      [--layer back|front|N] [--blend normal|tint] [-z N]
      draw ext --id SPRITE NAME FUNCTION [JSON]   redraw in place
ptyrc show                        live mirror of the remote screen
ptyrc events [TOPIC...]           JSON events, one per line
ptyrc debug overlay frames|heat|off ; ptyrc debug dump
ptyrc call METHOD [JSON]          raw request
```

### Errors

On stderr: `ptyrc: <what failed>`, then one indented line saying what to
try. No panic, no errno number.

```
$ ptyrc run -- hto
ptyrc: hto: command not found in PATH
  did you mean htop?
$ ptyrc text -f activ
ptyrc: no-frame: no such frame: activ
  did you mean active? frames: 0, 1; names: raw, active, largest, quietest, idle
```

Exit status:

| status | meaning |
|---|---|
| 0 | done |
| 1 | a negative answer: `find` without a match (silent), `wait` timed out (says what was waited for) |
| 2 | usage, or a request that is wrong: a bad key, colour, anchor, position, JSON, method, topic, session name for `run` |
| 3 | no session to talk to: none, not found, several and none chosen, dead (stale socket, removed), bad name; `run`: the name is taken, no socket directory |
| 4 | the request names what is not there or not usable: frame, sprite |
| 5 | the connection or the system: closed by the driver, I/O |
| 126 | `run`: the command is there and cannot run: a directory, not executable, no interpreter |
| 127 | `run`: the command is not there |

Otherwise the status of `ptyrc run` is the app's (128 + signal when
killed). The Python pilot raises `PtyrcError` with the same `code`,
`message` and `hint`.

## Python

`python/ptyrc`: `Pilot` (blocking, stdlib sockets, one reader thread for
events), `Frame`, `Sprite` handles; mirrors the protocol one to one, plus
`wait_for`, context managers and `Pilot.spawn(argv, headless=True)` for
tests. `Frame.check_idle()` is the idle score, `Frame.is_idle` the
boolean; both ask the driver when read. `plugins/client-apps/python`: `ptyrc-apps`, one module per app,
`python -m ptyrc_apps.<name>` and `ptyrc-app <name>`. Every app takes
`-s SESSION` and `-f FRAME`; its sprites die with its connection.

| app | default frame | what runs where |
|---|---|---|
| `confetti` | `raw` | native effect. `--spawn [--rate N]`: one request per burst, N a minute (20), until interrupted; back layer unless `--front` |
| `clock` | `raw` | one `draw.update` a second; front layer |
| `flower` | `raw` | text anchor and sway animation native; three requests in all |
| `fishes` | `largest` | positions in frame coordinates, computed by the client, one batch every 100 ms; flee the cursor |
| `snake` | `largest` | client: breadth-first search over the free cells of the view, one batch per move (12 a second); frame coordinates |
| `sand` | `idle` | native effect `sand` (draw extension `sand`), walls sent once; the client polls the pane's text every 0.5 s, removes the sand when it changes and pours again once `is_idle` |

Draw extension `sand`: effect `sand`, params `walls` (rows of text; a
cell that is not a space holds the sand), `rate` (grains per second,
30), `colors`, `duration_ms`, `seed`. 20 steps a second.

`scripts/demo-apps.sh [snake] [--check]`: the apps on a scrolling vim
buffer. `scripts/demo-frames.sh [--check]`: a tmux session of two
windows under the driver, one drawing per pane, sand in the idle one.
`--check` runs detached and asserts what is drawn.

## Tests

- `scripts/e2e-frames.sh`: frames on real vim, tmux, less, man, more,
  htop, a shell and a stand-in agent TUI, in a detached tmux.
- `scripts/e2e-tuis.sh`: htop, less, man, tmux, ranger, glances and
  opencode in a detached tmux: the screen read against the host, the
  layout found and its steadiness, a sprite drawn and removed, the exit.
  `KNOWN` marks a defect recorded in `plan/ideas.md`.
- `scripts/e2e-render.sh`: the driver in a detached tmux; what tmux shows
  is compared, text and styles, with the driver's model of the host.
- `scripts/e2e-heat.sh`: heat classes and idle on a shell, vim and htop.
- `scripts/bench.sh`: throughput and key-echo latency, bare, transparent,
  engine, engine without presets, engine with sprites.

- Rust unit tests per crate, `cargo test --workspace`.
- `ptyrc-vt`: the repair property above; recorded streams of real programs.
- `ptyrc-server`: headless end-to-end, a `Vt` fed with the host log stands
  for the host.
- `ptyrc-cli/tests/errors.rs`: every error case of the CLI, message, hint
  and exit status.
- Python: `uv run pytest`, against a headless driver.
- `scripts/e2e-tmux.sh`: the driver inside a detached tmux, `tmux
  capture-pane` is the host's verdict.
