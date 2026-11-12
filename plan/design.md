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
ptyrc-server -> ptyrc-core, ptyrc-vt, ptyrc-proto (mio, nix, signal-hook)
plugins      -> ptyrc-core
ptyrc-core   -> ptyrc-vt (serde, serde_json, regex)
ptyrc-proto  -> serde, serde_json
ptyrc-vt     -> vte, unicode-width, smol_str
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
  host shows.

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
not use DECSC/DECRC (one slot, the app may own it). It wraps its output in
synchronized update (`CSI ? 2026 h/l`) unless the app has one open. It
feeds its own output to `host`.

Coalescing: after a read the driver keeps reading while the pty is
readable, then polls up to `coalesce_ms` (default 1) for more, bounded by
8 ms and 1 MiB. A whole app redraw and the sprite motion it implies go out
in one write.

Resize: both `Vt` are resized without reflow, sprites are forgotten as
painted (`host.grid := app.grid`), every frame returns to the pool.

Exit: sprites are unpainted, the host termios restored, the exit status is
the app's.

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

Scope: what full-screen programs and shells emit. C0, CSI cursor moves,
ED/EL/ECH, IL/DL/ICH/DCH, SU/SD, DECSTBM, RI/IND/NEL, SGR (16, 256, RGB,
underline styles and colour), DECSC/DECRC, alt screen (47, 1047, 1049),
DECOM, DECAWM, IRM, DECTCEM, charsets (G0/G1, DEC special graphics),
tabs, OSC 0/2 title, OSC 8 hyperlinks, mode 2026, REP, wide and combining
glyphs. Unknown sequences are consumed and ignored. Modes that only
concern input (mouse, bracketed paste, cursor keys) are tracked, not acted
on.

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
2. **Horizontal scroll**: same on columns when rows changed with `dy = 0`.
3. **Splice**: the top aligns at one shift and the bottom at another. `k`
   lines were inserted or deleted at the seam; the buffer is spliced and
   the rows below move.
4. **Jump**: nothing aligns. The new rows are looked up in the buffer by
   line hash; the origin with the most votes wins. Otherwise a line-number
   gutter (increasing integers at the left of consecutive rows), or a
   plugin `view_hint`, gives the absolute row. Otherwise a new island is
   allocated far below the known extent; when a later scroll shows an
   island touching another, they are merged and their sprites translated.
5. **Edit**: none of the above. Visible lines are rewritten in place.

The origin follows at once (sprites move in the burst that scrolls).
Content enters the remembered buffer once it has been unchanged for
`commit_ms` (150), so a half-drawn screen is not remembered.

### Dynamic frame

Segments the screen into panes, each with an extended frame.

- **Separators**: vertical runs of border glyphs (`│ ┃ ║ | ╎ ┆ ┊` and
  junctions), horizontal runs (`─ ━ ═ -` and junctions), and bars (a row
  whose background or inverse attribute is uniform across the rect: status
  lines, tab bars).
- **Boxes**: closed rectangles of border glyphs with corners; the interior
  is a pane.
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

### Heatmap

Per frame, in view coordinates, per cell: `activity` (EWMA of change
events, time constants 2 s and 60 s) and `occupancy` (EWMA of non-blank).
`safe = (1 - activity) * (1 - occupancy)`. Exposed as digits 0 to 9.

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
  "blend": "over",
  "z": 0,
  "ttl_ms": null,
  "persist": false,
  "motion": {"vx": 0.0, "vy": 0.0, "ax": 0.0, "ay": 0.0, "bounce": false},
  "effect": {"name": "confetti", "params": {}}
}
```

- `frame`: an id, or `"raw"`, `"active"` (pane holding the cursor),
  `"largest"`, `"quietest"`. A name is resolved when the sprite is added.
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
- `blend`: `over`; `behind` (only where the app cell is blank: text stays
  on top); `tint` (keeps the app glyph, applies the style).
- `effect`: a native generator in place of `content`; it owns the sprite's
  cells every tick. Built in: `confetti`. Plugins add more.
- A sprite belongs to the connection that added it and dies with it unless
  `persist`. `ttl_ms` bounds its life in any case.

Sprites are clipped to the view rect of their frame. `compose` goes through
`Grid::put`.

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
coding-agent TUIs: prompt box, streaming transcript). First draw
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

Methods (`frame` is a frame id or name, default `"raw"`):

| method | params | result |
|---|---|---|
| `session.info` | | hello data, `alt_screen`, `title`, `exited` |
| `screen.text` | `frame`, `rect` | `{"lines":[...],"size":[c,r]}` |
| `screen.cells` | `frame`, `rect` | `{"rows":[[run,...],...]}` |
| `screen.find` | `pattern`, `regex`, `ignore_case`, `frame`, `known` | `{"matches":[{"frame","x","y","len","text","screen":[c,r]\|null}]}` |
| `screen.cursor` | `trail` | `{"x","y","visible","frame","trail":[[ms,x,y],...]}` |
| `screen.wait` | `pattern` or `stable_ms` or `gone`, `timeout_ms`, `frame` | `{"matched":bool,"match":...}` |
| `input.send` | `text` or `keys` or `bytes` (base64), `paste` | `{}` |
| `frame.list` | | `{"frames":[info,...],"layout":id}` |
| `frame.text` | `frame`, `y0`, `y1` | `{"lines":[...],"y0":n}` remembered buffer |
| `frame.heat` | `frame` | `{"rows":["0123...",...]}` |
| `frame.at` | `x`, `y` | `{"frame":id,"pos":[x,y]}` |
| `draw.put` | sprite | `{"id":n}` |
| `draw.update` | `id`, patch | `{}` |
| `draw.remove` | `id` | `{}` |
| `draw.clear` | `all` | `{"removed":n}` |
| `draw.list` | | `{"sprites":[...]}` |
| `draw.ext` | `name`, `function`, `params`, sprite fields | `{"id":n}` |
| `draw.exts` | | `{"exts":[{"name","functions":[...]}],"effects":[...]}` |
| `batch` | `calls`: `[{"method","params"}]` | `{"results":[...]}`, applied in one repaint |
| `subscribe` | `topics`: `screen`, `cursor`, `frames`, `input`, `size`, `exit` | `{}` |
| `debug.overlay` | `what`: `frames`, `heat`, `off` | `{}` |
| `debug.dump` | | app grid, host grid, sprites, frames |

Frame info: `{"id","kind":"raw"|"pane"|"chrome","state":"candidate"|
"stable"|"inactive","usable","rect":[x,y,w,h],"origin":[x,y],
"extent":[y_min,y_max],"key","support":"vim"|null}`.

Keys for `input.send`: `enter`, `tab`, `esc`, `space`, `backspace`,
`delete`, `up`, `down`, `left`, `right`, `home`, `end`, `pageup`,
`pagedown`, `f1`..`f12`, `ctrl+<x>`, `alt+<x>`, `shift+tab`; any other
string is sent as text. Arrow keys follow the app's cursor-key mode.

## CLI

`ptyrc <verb>`; `-s/--session` on every client verb; `--json` where a human
form exists.

```
ptyrc run [--name N] [--headless --size CxR] [--host-log F] [--] CMD...
ptyrc ls                          sessions
ptyrc text [-f FRAME] [--ansi]    screen as plain text
ptyrc find [-E] [-i] PATTERN      matches with coordinates
ptyrc cursor
ptyrc send TEXT                   type text
ptyrc keys KEY...                 press keys
ptyrc wait [PATTERN] [--stable MS] [--gone] [--timeout MS]
ptyrc frames [--watch]
ptyrc heat [-f FRAME]
ptyrc draw text|rm|ls|clear|ext ...
ptyrc show                        live mirror of the remote screen
ptyrc events [TOPIC...]           JSON events, one per line
ptyrc debug overlay frames|heat|off ; ptyrc debug dump
ptyrc call METHOD [JSON]          raw request
```

## Python

`python/ptyrc`: `Pilot` (blocking, stdlib sockets, one reader thread for
events), `Frame`, `Sprite` handles; mirrors the protocol one to one, plus
`wait_for`, context managers and `Pilot.spawn(argv, headless=True)` for
tests. `plugins/client-apps/python`: `ptyrc-apps`, one module per app,
`python -m ptyrc_apps.<name>` and `ptyrc-app <name>`: `confetti`,
`flower`, `clock`, `fishes`, `snake`, `sand`.

## Tests

- Rust unit tests per crate, `cargo test --workspace`.
- `ptyrc-vt`: the repair property above; recorded streams of real programs.
- `ptyrc-server`: headless end-to-end, a `Vt` fed with the host log stands
  for the host.
- Python: `uv run pytest`, against a headless driver.
- `scripts/e2e-tmux.sh`: the driver inside a detached tmux, `tmux
  capture-pane` is the host's verdict.
