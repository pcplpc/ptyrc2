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

Host colours: at start the driver asks the host terminal for its default
foreground and background (OSC 10, OSC 11), unless given (`--host-fg`,
`--host-bg`), headless or transparent. The first reply to each within
1 s is taken out of the keyboard stream; later replies are the app's
and are read on the way. No answer: `#e5e5e5` on `#000000`.

At run time: `session.colors {fg, bg, ask}`, all optional, answers the
colours and where each comes from (`default`, `option`, `host`,
`client`), and `asked`. `fg`, `bg`: a colour (`#rrggbb`, a name,
`0..255`) to set. `ask`: the host is asked again for those not given;
its reply comes later. Any change is an event of topic `colors` (the
same object) and one repaint: every `alpha` mix is computed again.
`ptyrc call session.colors '{"bg": "#fdf6e3"}'`.

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
   and sprites are painted, first unpaint the rows that leave (`base`,
   the app grid before the burst, in those rows): sprites never enter
   the scrollback. The other rows scroll with their sprites and are
   repaired after. A burst that also scrolls a region, inserts, deletes
   or clears: `repair(host, base)`, everything is unpainted.
3. Emit the burst; `host.feed(burst)`.
4. `frames.update(...)`, `widgets.tick(...)`, `scene.tick(...)`,
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

`palette`: `xterm(n)`, `rgb(color, default)`, `mix(under, over, alpha)`
(linear in sRGB), `parse_color_spec` (`rgb:R/G/B` with 1 to 4 digits,
`rgba:`, `#RRGGBB`).

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
    pub fn paged(&mut self, dy: i8, at: Option<(u16, u16)>, now: Duration);
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

`Grid::row_stamp(row)` changes at every write to the row; a clone is
another grid. `Screen::scan` reads only rows whose stamp changed, then
compares cell hashes: a write that changes nothing is still no change.

### Extended frame

A sparse buffer of lines addressed by `i64` frame rows, each line a run of
cells starting at an `i64` frame column, plus the view: screen rect and
origin. Each update compares the previous view content with the new one:

1. **Scroll**: the vertical shift `dy` that aligns the most rows (blank rows
   do not vote, rows unique in the view weigh more). Accepted when it
   beats `dy = 0` clearly and aligns at least `max(2, 30 %)` of the
   non-blank rows. `VtEvent::Scroll` inside the rect seeds the candidates.
   Fewer are enough when every row that could align does (two or more,
   each unique in the view) and none stayed: a page with an overlap
   (vim `ctrl+f` keeps two rows).
   Rows that stay put while the rest shifts are chrome: the scroll band is
   the largest run of rows that moves, learned over time. Rows inside a
   region the emulator scrolled are not top chrome.
   **Feed**: no row aligns, but the emulator counted the scroll. The
   distance is trusted when the rows went into the host scrollback (main
   screen), or when the screen was fed as a tape: one direction, fewer
   lines than the scrolled region has rows, no clear in the last 100 ms.
   Then every line that went through is on screen before or after. A
   pager's page down or up is such a feed (`less`, `man`, `more`: one
   line less than the screen). A pager paints a jump by a clear, by
   cursor addressing, or by feeding a whole screen or more: not a feed.
   Two to four pages fed in one burst (a key held) are a feed: a page
   is the region less its bottom chrome, so their sum is not a jump's;
   the rows that went through unseen stay unknown. Feeds that are
   pieces of one write and sum to the region or more, not to whole
   pages, are one jump: the rows are looked up, else an island. A
   piece is told by the last row once bottom chrome is known: a feed
   that is whole ends on the prompt as it was before, a piece does not
   and the next is waited for 100 ms (a page then part of the next, a
   wheel flick: feeds, however close). Without bottom chrome, pieces
   are feeds less than 10 ms apart.
   Under a feed, the last rows that did not travel are bottom chrome:
   those of the screen before when fed forward, of the screen after
   when fed backward. A blank last row does not vote (a page cut before
   its prompt); a last row erased alone in the observation before (a
   prompt taken away when the key comes) votes as it was. On the
   alternate screen the first such vote is believed: the prompt row is
   chrome from the first page. Elsewhere two are needed.
   **Page asked**: `pageup`, `pagedown` (`CSI 5 ~`, `CSI 6 ~`) or a
   wheel notch up or down, with no modifier, among the bytes on their
   way to the app (host keyboard, `input.send`, `input.click`, the
   input of a tape) is a hint for 300 ms: `Frames::paged`. A key is for
   every pane, a notch for the pane at its cell. When no rule above
   placed a change that left fewer than 40 % of the rows where they
   were, the two views are stitched: the least shift that way under
   which every row that could align does, blank rows included. One
   aligned row is enough when it tells: unique in the view before and
   after, 4 glyphs or more. Where no chrome is known, up to 2 rows at
   an end may differ (chrome not learned yet), fewest first; they are
   learned as chrome by the scroll. No stitch: the hint is given up,
   the rules below apply as without it.
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

Share: `(END)` at the end of the last row of the pane, or a percentage
as its last word (`45%`, `--More--(20%)`), the cursor on that row. An
estimate, never a hint. An island first shown with a share is put among
the others in the order of their shares, when a slot (`ISLAND_GAP`) is
free between them, else below as before; within its slot at the
estimated row, when the length of the document is known: rows and a
share seen together on the rows that start at 0, the largest share
kept. Islands stay a slot apart whatever the estimate, and are merged
exactly when walked into. Nothing is estimated under a hint.

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
`commit_ms` (150), so a half-drawn screen is not remembered. A row that
leaves the view is remembered on the way out, unless it came less than
10 ms before: it was on its way through (the marker of a pager's jump).

### Dynamic frame

Segments the screen into panes, each with an extended frame.

- **Separators**: vertical runs of border glyphs (`│ ┃ ║ | ╎ ┆ ┊` and
  junctions), horizontal runs (`─ ━ ═ -` and junctions), and bars (a row
  whose background or inverse attribute is uniform across the rect: status
  lines, tab bars).
  A bar is a row that one backdrop, not the default, covers (nine cells
  in ten) while the rows above and below show another. Such a row is an
  entry of a table, not a bar, when a row next to it that is no bar has
  its shape: 6 word edges or more (starts and ends) in the same cells
  with the same class (digit, letter, same sign), six in ten of those of
  the row that has fewer. A header of letters over figures stays a bar.
  A bar that moves
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
  changed, when one row changed that is not the row that had changed
  alone the last time (a paint in pieces: the rows, then a ruler), when
  the title or the layout changed, else once a second; a
  preset found is held 4 s and asked (chrome, hint, key) on every change.
  A preset that knows the whole screen may segment on any change.
- A pane that leaves at a layout change and a new one that overlaps it,
  of the same key, with fewer than two rows of text to compare (an
  empty shell resized): the same frame, when it is the only such pane.
- Advice asked of the presets for a pending layout is kept until the
  layout is committed or replaced; ticks do not clear it.
- When nothing aligns, the emulator's scroll counts are trusted only if
  rows entered the host scrollback (a shell printing a lot at once).
- A pane replaced whole is a monitor when the cells were rewritten where
  they are (no clear, no scroll in the burst), no cursor shows in the
  pane, and the rows kept their shape: on average 30 % of the cells keep
  their glyph and 70 % their class (digit, letter, same sign). Failing
  that, when it was replaced twice within 3 s; or, a cursor showing,
  three times at even intervals (one part in ten) with the cursor on
  the same cell and no scroll nor jump between: two islands are spent,
  then it is a monitor (`watch -n 5`). A monitor is rewritten in
  place: every change is an edit of the rows in view, no frame is spent,
  no island. It ends on a clear, or on a change of half its rows that is
  of another shape. A scroll the emulator counted and the rows confirm
  (nine in ten) is still a scroll. A preset declares one with
  `Tuning.monitor`.
- Rows of a pane under a smaller pane of the last segmentation (a
  palette, a modal) are not read while they are: they keep what they
  showed and do not vote. Never seen before (a pane first seen under
  an overlay), they are blank until uncovered.
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

`debug overlay heat`: dark backgrounds only. Red for hot (brighter when
busier), blue for static text, green for free, violet for a panel; the
last three brighten until the cell has been still for 10 s.

The heat is lazy. A row is brought up to date when the screen changed on
it; otherwise it is read as it would stand at the asked time (decay
composes). An update or a tick without a visible change costs nothing
per cell. The backdrop counts are kept per row. A pane that missed a
scan, or whose rect moved, is read whole again.

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
    pub fn tick_on(&mut self, now: Duration, base: &Grid, surfaces: &dyn Surfaces);
    pub fn next_wake(&self) -> Option<Duration>;
    pub fn compose(&self, base: &Grid, surfaces: &dyn Surfaces) -> Grid;
    pub fn register_effect(&mut self, name: &str, make: EffectFactory);
    pub fn set_host_colors(&mut self, host: HostColors);
}
```

`set_host_colors`: what `Color::Default` shows; `#e5e5e5` on `#000000`
until told. `alpha`: opacity, `0..=1`; absent: opaque, colours written
as given. Under 1 the colours are mixed with those below and written as
RGB (palette colours through the xterm table, `default` through the
host colours).

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
  "alpha": 1.0,
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
    `"slide": true` (default): while the row of the match is in the view,
    the sprite moves along its centred axis (`left`, `right`: rows;
    `above`, `below`: columns) to stay whole inside the view, never onto
    the match; `false`: strict centring.
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
- `panel`: a colour. The sprite is on the app's panel of that backdrop:
  drawn only in the cells whose backdrop is that colour, in any layer.
  See Layers.
- `blend`: `normal`; `tint` (keeps the glyph below, applies the style).
  `over` and `behind`, names of the first draft, are read as `normal`
  with the layer `front` or `back` when none is given.
- `z`: order inside a layer.
- `effect`: a native generator in place of `content`; it owns the sprite's
  cells every tick. Built in: `confetti`, `sparkle`. Plugins add more.
  See Effects.
- A sprite belongs to the connection that added it and dies with it unless
  `persist`. `ttl_ms` bounds its life in any case.

Sprites are clipped to the view rect of their frame. `compose` goes through
`Grid::put`.

`Scene::list_on(base, surfaces)`: `list` plus `visible` (a glyph inside
the view of a usable frame) and `covered` (back layers: share of the
glyphs in view under app cells with ink; 0 in front).
`Scene::forget_rows` (`FrameEvent::Forgotten`) and `Scene::drop_frame` remove the
sprites fixed on forgotten rows or on a forgotten frame and return their
ids; a text anchor on those rows only loses its held match.

A finite animation and an effect start their clock when the sprite is
first drawn: added on a frame that is not usable yet, or on a text that
is not there yet, they wait. `ttl_ms` counts from the add in any case.

### Effects

```rust
pub trait Effect: Send {
    fn render(&mut self, t: Duration, canvas: &mut Canvas) -> bool;
    fn looks(&self) -> Looks { Looks::No }        // No, Cells, Heat
    fn look(&mut self, under: &Under) {}
    fn tune(&mut self, params: &Value) -> Result<(), String>;
    fn shift(&mut self, from: i64, to: i64, dy: i64) {}
    fn period(&self) -> Duration;                 // 50 ms
}
impl Under<'_> {
    pub size: (u16, u16); pub cursor: Option<(u16, u16)>;
    pub origin: (i64, i64);
    pub fn cell(&self, x, y) -> Option<&Cell>;
    pub fn ink(&self, x, y) -> bool;  pub fn inked(&self) -> Vec<bool>;
    pub fn heat(&self, x, y) -> Option<Warmth>;   // class, still, activity
    pub fn row(&self, y: i64) -> Option<Vec<bool>>;
}
```

- `Scene::tick_on` is `tick` with the app's grid: an effect that looks
  (`Looks::Cells`) is shown, before each render on a usable frame, the
  app's cells under its canvas. `Under` borrows the grid: nothing is
  copied, nothing is read for an effect that does not look. `tick`
  alone shows nothing.
- Cells are those of the screen, a canvas cell each: a tab or a wide
  glyph shifts nothing. `ink` is `Cell::has_ink`, true for both halves
  of a wide glyph. A canvas cell outside the view of the frame (a
  `size`, an anchor, a scroll) shows nothing.
- `origin`: frame position of the canvas cell `(0, 0)`. When it changes
  (the app scrolled under a canvas fixed in the view) the effect is
  shown and rendered in the same tick, off its period: what it keeps
  on frame rows moves with the text in the burst that scrolls.
- `Looks::Heat`: also the heat of those cells (`Surfaces::heat`), read
  again every 200 ms at most.
- `row(y)`: ink of the canvas columns on a row above (negative) or
  below the canvas and out of the view, as the frame remembers it
  (`Surfaces::ink_row`: a remembered character other than a space);
  `None` in the view or when not remembered.
- `shift`: rows `from..=to` of the frame moved by `dy` (a splice, an
  island merge; `Scene::shift`). The effect moves what it keeps on
  them, and is shown and rendered at the next tick, off its period.
- `tune`: new parameters for a running effect; its state is kept. An
  effect that does not implement it refuses. `confetti` takes
  `duration_ms` and `origin` (the pieces keep their flight); `sparkle`
  takes `count`, `colors`, `life_ms`, `duration_ms`; any other key is
  refused.
- `confetti` params: `count` (64), `colors`, `duration_ms` (3000),
  `origin` (`[x, y]` on the canvas; a third down the middle), `spread`
  (speed scale, 1), `seed`, `shape`: `fan` (upwards, wider than tall;
  the default), `ball` (every direction, every speed), `ring` (every
  direction, one speed), `star` (five points), `fountain` (a narrow jet
  upwards).
- `Canvas::put_faded(x, y, text, style, opacity)`: a glyph with its
  own opacity, multiplied by the sprite's `alpha`.

`draw.update {id, effect: {name, params}}`: the running effect of that
name is tuned (`bad-params` with the effect's words when it refuses,
nothing changed); another name, or a sprite with a content, starts that
effect in place: frame, anchor, layer and `size` are kept. `draw.ext`
with `id` does the same when the extension draws an effect. `ptyrc
draw effect --id SPRITE NAME [JSON]`.

### Layers

A sandwich. The app is layer 0. Sprites of negative layers are behind
it, sprites of positive layers in front. Paint order is `(layer, z,
id)`. The default is behind: the app's text stays in front of what is
drawn. Names: `wallpaper` (-100), `back` (-1), `front` (1), `hud` (100);
an integer between two names is a layer between them; `app` and 0 are
refused.

`layer.define {name, z}` declares a name for the session, every client
included, until the driver exits; `layer.list` and both answers:
`{"layers": [{name, z}]}`, back to front. Refused: a fixed name, `app`,
an integer, `z` 0, more than 64 names. Declared again a name moves; the
sprites already there keep their place. A name is replaced by its `z`
when a sprite is put or updated; an unknown name is `bad layer`, with
the names as a hint.

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
- On a panel (`panel`: the backdrop named): the sprite is clipped to
  the cells of that backdrop, inverse ones included. Its opaque `bg`
  replaces the panel's colour in the cells that are not inverse; a
  glaze is mixed with it. Without `panel` an opaque back colour never
  shows under a panel.
- A `tint` behind the app is a colour of background only.
- With `alpha`, a back `bg` is a glaze: mixed with the opaque back
  colour or the host's default where the app left the background, with
  the app's backdrop where it chose one (a panel); inverse cells stay
  the app's. A back glyph's `fg` is mixed with the background it stands
  on; on an inverse cell that is the app's foreground, and the cell
  stays inverse.

With back layers alone, every app cell with ink keeps its glyph, its
foreground and its attributes, and every backdrop the app chose is
kept, but the one a sprite names as its `panel`; inverse cells are
kept in any case (tested as a property).

Front layers go over whatever is below, app text included: a space is
transparent unless `opaque`; a glyph without `bg` keeps the colour
below it. A front `tint` recolours the app's glyphs. With `alpha`, a
front glyph's `bg` is mixed with the background below and its `fg` with
the result; a blank with a `bg` is glass: the glyph below stays, both
its colours mixed. A `tint` mixes each colour with the one it replaces.
A wide front glyph without `bg` over two backdrops is not drawn.

The reference apps follow: confetti and sparkles are behind the text
(`ptyrc-app confetti --front` brings the confetti over it), the clock is
in front.

A scene of four layers:

```
ptyrc draw text --layer -2 --bg 17 --opaque ...    static background
ptyrc draw text --at 4,2 '<*//><'                  a drawing in front of it
                                                   the app's text
ptyrc draw text --layer front --auto top-right ... a display in front of all
ptyrc draw text --layer wallpaper --bg '#804000' --opaque --alpha 0.4 ...
```

## ptyrc-core: widgets

A widget is a small window a client keeps on screen: a content in a
border. The client says what; the driver says where, in which size, and
when it moves. Clients share the screen: their widgets pave it.

- **Variant**: one way to show a piece of information: a `name`, an
  inner `size` `[w, h]`, a content of that size.
- **Tag**: names the information. The variants of a tag show the same
  thing, laid out for different sizes.
- **Widget**: a tag shown. At most one of its variants at a time, in a
  border, as one sprite of the scene owned by the client's connection.

```rust
pub struct Widgets;
impl Widgets {
    pub fn new(cfg: WidgetConfig) -> Widgets;
    pub fn register_border(&mut self, name: &str, border: BorderSpec);
    pub fn borders(&self) -> Vec<String>;
    pub fn declare(&mut self, owner: OwnerId, decl: Declare)
        -> Result<Vec<String>, WidgetError>;
    pub fn show(&mut self, owner: OwnerId, show: Show)
        -> Result<WidgetId, WidgetError>;
    pub fn set(&mut self, owner: OwnerId, set: Set) -> Result<(), WidgetError>;
    pub fn hide(&mut self, owner: OwnerId, id: WidgetId) -> bool;
    pub fn drop_owner(&mut self, owner: OwnerId);
    pub fn hide_owner(&mut self, owner: OwnerId, hidden: bool);
    pub fn touch(&mut self);                      // the screen changed
    pub fn tick(&mut self, now: Duration, base: &Grid,
        cursor: Option<(u16, u16)>,               // when it shows
        surfaces: &dyn Surfaces, scene: &mut Scene) -> Vec<WidgetInfo>;
    pub fn next_wake(&self) -> Option<Duration>;
    pub fn list(&self) -> Vec<WidgetInfo>;
    pub fn info(&self, id: WidgetId) -> Option<WidgetInfo>;
}
```

No clock inside. `tick` lays out, adds, updates and removes the sprites,
and returns the widgets whose info changed (one `widgets` event each).
The driver touches on every burst, resize and frame change, and ticks
before the scene is ticked and composed: a widget moves in the same
repaint.

### Declared, shown, set

- `widget.declare {widgets: [{tag, name, size, content}], border}`:
  the collection of the connection. A tag declared again has its
  variants replaced; a widget of it is laid out again; a variant of
  the same name and size, given no `content`, keeps its own. `name`: `WxH`
  when absent, unique in the tag. `content`: optional, the member of a
  sprite (`{"rows": [...], "style": {...}}` or `{"cells": [[run,
  ...], ...]}`). Limits: 64 tags a connection, 16 variants a tag,
  inner sizes from 1x1 to 200x60. Answer: `{"tags": [...]}`, every tag
  of the connection, in order of first declaration. Nothing is applied
  on error.
- `widget.show {tag, frame, prefer, fill, title, border, layer,
  modal}`: one
  widget per connection and tag; shown again, the same widget with the
  new options, those absent at their defaults. `frame`: an id or a
  name, resolved once, at the call (`raw`). `prefer`: as for `auto`
  (`any`). `fill`: `0.1..=1` (0.5). `title`: the tag when absent;
  `""`: none. `layer`: `back` when absent (`hud` for a modal), a
  declared name allowed. `modal` (false): see Modal. It
  is laid out at once: the answer `{"widget": info}` is placed when a
  room is there.
- `widget.set {id, variant, content, title}`, `content` and `title`
  optional, one at least: the content of one variant (`variant` absent:
  the one shown; `not-usable` when none is), kept by the driver; and,
  or, the title. A variant never set is blank.
- `widget.hide {id}`: the widget goes; the declaration stays.
- Params: a member given as null is absent; an unknown member is
  refused (`bad-params`).
- A widget dies with its connection. A widget id that is not one of
  the connection's: `no-sprite`, `no such widget: N`, the hint naming
  the connection's widgets.

Content is clipped to the inner size and padded with blanks; a wide
glyph cut by the right edge is a blank. The inside is opaque: a cell
without `bg` gets the default background, nothing of the layers behind
shows through. The app's text does: the default layer is `back`, a
widget never hides what the app writes.

### Borders

One cell thick, dim. The title is on the bottom edge, between the
corners: `before`, the title, `after`, up to the cell before the
corner (`side` bottom, `align` right): `└─── clock ┘`. Room: the
cells between the corners less `before` and `after`. Cut with `…` when it does not fit, absent
(with `before` and `after`) under 3 cells of room. The edge keeps its
pattern on both sides of the title. The title is not dim.

`border`: a preset name, or an object: `{"base": NAME, "corners":
"┌┐└┘", "top": "─", "bottom": "─", "left": "│", "right": "│", "style":
{...}, "title": {"side": "top" | "bottom", "align": "left" | "center"
| "right", "before": " ", "after": " ", "style": {...}}, "chase":
{"len": 3, "ms": 80, "runs": 1, "style": {...}}}`. `base`: the preset the other members
replace (`plain`), a member of `title` each; `style` replaces the
whole style. An edge is a pattern, one glyph or several, repeated
along it from the top or the left; glyphs of one cell only; four
corners and four edges, or none at all. An unknown member: `bad
border`. Given at `widget.declare` it is the connection's default
(absent: kept); at `widget.show`, the widget's.

Presets of the core: `plain` (thin, square, dim; the default), `round`,
`heavy`, `double`, `ascii`, `alert` (heavy, bright yellow, bold, not
dim, the title too; a bright white chase: `len` 6, `ms` 50, `runs` 2),
`none` (no border, no title: the outer size is the inner one). A draw extension adds presets
(`DrawExt::borders`); `widget.borders` lists the names, the core's
first. Extension `borders`: `painting` (gilt double frame, ticks along
the edges), `baroque` (bronze, crossed corners, studded edges), `mat`
(a pale mount of half blocks), `sketch` (dashed, rounded, dim).

`chase`: `runs` runs of `len` border cells in that style (it replaces
the border's), evenly spaced, go round the border clockwise, a cell
every `ms` (20 to 10000; `len` 64, `runs` 16 at most; `len` 0: none),
under the title. Played by the driver: the cell reached is the time
over `ms` (`ChaseSpec::step`), no state; `next_wake` names the next
step while a chased widget is drawn on a usable frame, and only then.
A step is one content update of the sprite; no look.

### Layout

In screen cells. Widgets are laid out one at a time, the oldest first;
each one placed is an obstacle to the next.

- **Open cell**, for a widget: inside the view of its frame; no ink in
  the app's grid; not of class `panel`; no visible change for
  `quiet_ms` (1000); not on the row of a visible cursor; not under
  another widget placed.
- **Room**: a maximal rectangle of open cells (histogram and stack,
  O(cells)).
- A variant fits a room that holds its outer size and `gap` (2
  columns, 1 row) on every side but those on an edge of the view: a
  widget keeps the gap from text, from other widgets, from the cursor
  row.
- In a room: of the variants that fit, the one whose outer area is the
  nearest to `fill` of the room's; the larger on a tie. In the room
  less its gaps it is on the side `prefer` names (`top-right`: the
  top right corner; `top`: the top, centred), centred for `any`.
- Of the rooms: the one that gives the largest variant (outer area);
  then the nearest to `prefer`, when not `any` (the widget's centre to
  the point named, each axis over the view's size); then the largest;
  then the topmost, the leftmost.
- No room for any variant: a widget placed in the same frame, not
  pressed, the largest first, takes a smaller variant, laid out as a
  new one: the largest that lets this one in. Both move in the same
  look. None does: the widget is hidden until a room opens.

A placed widget stays where it is, as it is, unless shrunk for
another:

- **Pressed**: an app cell under its outer rectangle has ink, or the
  rectangle is no longer whole inside the view. Seen at the first tick
  after a change. It stays drawn (the text shows over it). Pressed for
  `evict_ms` (300), it is laid out again as a new one: elsewhere,
  smaller, or hidden.
- **Grown**: at each look, when a layout as a new widget, its own cells
  counted open, gives a larger variant (outer area), and the same one
  in the same place for `replace_ms` (2000) of looks, it moves there.
  Never for a variant of the same size or a smaller one: no jitter.
- A look: when the screen, an obstacle or a declaration changed
  (`touch`), `look_ms` (250) apart at least; at once after
  `widget.show`; and when a timer is due: an eviction, a move, the
  first cell, otherwise open, that turns quiet. A quiet screen costs
  nothing.

### Modal

`widget.show {modal: true}`: a widget that must be seen (the manager).

- Layer `hud` when none is given: border and inside are opaque, in
  front of the app's text.
- Laid out as any widget, in a room. No room for any variant: it is
  put **over** the text, with no look at the other widgets: the
  largest variant whose outer size and the gap on both sides fit the
  view; none: the largest that fits the view, no gap; none: the
  smallest, clipped by the view. In the view less the gaps, on the
  side `prefer` names, centred for `any`. The cells of the gap around
  it (as far as the view goes) are blanked, opaque, by the same
  sprite: the blackout moves and goes with the widget. In a room
  there is no blackout.
- It stays: never pressed, evicted, grown, nor shrunk for another,
  and it shrinks none; text written under it is hidden. Laid out
  again only when the view of its frame changes size, its declaration
  changes, or it is shown again.
- It is an obstacle to the others; a widget under a modal or its
  blackout is pressed, and evicted after `evict_ms`.
- Info: `"modal": true`, `"over"`: put over the text.

### Hidden owners

`Widgets::hide_owner(owner, hidden)` (`app.pause`): the widgets of
that connection are not drawn (their sprites are removed; info
`hidden`, as one without a room), keep their place and variant, are
neither laid out nor pressed, and are no obstacle. Shown again: where
they were; laid out again when another widget took the place, pressed
as ever when text did. Contents set meanwhile are kept.

The widget's sprite: frame and layer as shown, anchored in view cells,
border and content in one `cells` content, owned by the connection.
Hidden with its frame (not usable), back with it. It is listed by
`draw.list`; removed by
`draw.remove`, `draw.clear` or with a forgotten frame, the widget goes
as by `widget.hide`.

### Events

Topic `widgets`: the info of a widget, when it changes: placed, moved,
another variant, hidden, gone.

Widget info: `{"id","owner","tag","frame","state":"placed"|"hidden"|
"gone","variant":name|null,"size":[w,h]|null,"rect":[x,y,w,h]|null,
"sprite":id|null,"title","border":name|null,"variants":[{"name",
"size"}],"modal":bool,"over":bool}`. `owner`: the connection. `frame`: null until a name is
resolved. `size`: inner, of the variant shown. `rect`: outer, screen
cells. `title`: as drawn, `""` for none. `border`: null for one given
as an object. Hidden or gone: `variant`, `size`, `rect`, `sprite`
null. The event's data is the info itself; `subscribe {snapshot}`
sends one per widget.

## Apps and the manager

Decided with the owner, 2026-11-13 (`plan/interview.md`). An **app** here
is a client program of the session, not the wrapped program.

### The table

The driver keeps a table of apps (`ptyrc-server/src/apps.rs`):

- **child**: a program the driver started: `app.start`, `Config::apps`
  (`run --app`, `tmux attach --app`), the hotkey. Its own process group,
  stdin and stdout null, stderr to `<socket dir>/<session>.app-<id>.log`
  (truncated at each run, removed with the entry and at the end);
  environment: `PTYRC_SESSION`, `PTYRC_SOCKET`, `PTYRC_APP=<id>`.
  SIGTERM to its group at the end of the session.
- **client**: a process connected to the socket that is no child's: one
  entry a pid (`SO_PEERCRED`), made at its first request that is not
  `app.*`, gone with its last connection. A process that only asks of
  the table (a verb of `ptyrc manage`) is no app.

A connection belongs to the child whose process group holds its peer
(`getpgid` of the peer pid, at accept), else to the client of its pid.

App info: `{"id","kind":"child"|"client","name","argv":[...],"pid",
"state":"running"|"paused"|"exited","status":code|null,"runs":n,
"since":seconds,"conns":[id,...],"sprites":n,"widgets":n,"frames":[id,
...],"moves":bool,"log":path|null,"me":bool}`. `id`: from 1; that of a
child is never used twice in a session, that of a client is free again
when it leaves and no entry came after it (a verb that comes and goes
costs none). `name`: given at `app.start`; else the file name of
`argv[0]`. `argv`: as started; of
a client, `/proc/<pid>/cmdline` (empty when unreadable). `status`: of an
exited child, 128 + signal when killed. `runs`: starts so far. `since`:
seconds in the present state. `sprites`, `widgets`: those its
connections own. `frames`: the ids of the frames those are on, in
order, as they are when the info is made (a change of them alone is
no event). `moves`: a connection of it is subscribed to `move`. `me`:
the entry of the connection that asks; false for one that is no app.

An exited child stays in the table (`exited`) until restarted or
forgotten; 16 at most, the oldest forgotten first. A client that left is
not kept, nor the child of the hotkey once it has ended.

### Methods

| method | params | result |
|---|---|---|
| `app.list` | | `{"apps":[info,...]}`, by id |
| `app.start` | `argv`: `[program, arg...]`, `name` | `{"app":info}`; `bad-params` with the reason when it does not start |
| `app.stop` | `app`: id | `{"app":info}`; SIGTERM, SIGCONT when paused, SIGKILL after 2 s. A child: its group; a client: its pid. An exited child: nothing |
| `app.pause` | `app` | `{"app":info}`; see Pause |
| `app.resume` | `app` | `{"app":info}` |
| `app.restart` | `app`, `argv`, `name` (both optional: as before) | `{"app":info}`, at once; stopped as by `app.stop`, started again when it has ended, under the same id; an exited one at once (`bad-params` when it does not start: it stays exited, status 127, the reason in its log). A client: `not-usable` |
| `app.forget` | `app` | `{}`; an exited child leaves the table; another state: `not-usable` |
| `app.move` | `app`, `frame`: an id or a name, resolved at the call | `{"app":info,"frame":id}`; see Move |

An unknown id: `no-sprite`, `no such app: N`, the hint naming the apps.
An app may not stop, pause or restart itself (`me`): `not-usable`.
An unknown member: `bad-params`. Not on a replay (`bad-request`), nor
on `--transparent` (`Config::transparent`): no table.

Topic `apps`: the info of an app, when its state, pid, connections,
argv or `moves` change; `"state":"gone"` once for an entry that leaves
the table. `subscribe {snapshot}`: one per app. `me` is absent from
events. Sent once per turn of the driver's loop.

### Pause

`app.pause`: SIGSTOP (group of a child, pid of a client), and everything
its connections own is hidden: sprites, widgets (their place is kept,
not an obstacle to other widgets while hidden), effects. Their clocks
stand still: animations, effects, `ttl_ms` and motions go on at
`app.resume` from where they were. A sprite or a widget added while
paused is hidden too. A key grab it held is released.

Events are not queued for a paused connection (it does not read);
answers are. `app.resume`: SIGCONT, all is shown again, and each of its
connections is sent the snapshot of its topics (as `subscribe
{snapshot}`); one subscribed to `text` gets a `text` event per frame on
screen naming every row in view: an app that keeps the text reads it
again (`Engine::resume_events`). `Scene::hide_owner(owner, hidden,
now)`, `Widgets::hide_owner(owner, hidden)` (Widgets, Hidden owners),
through `Engine::hide_owner(conn, hidden)`.

### Move

An app that can change frame while it runs subscribes to the topic
`move`: that is its word for it (`moves`). `app.move` sends each such
connection the event `move`: `{"frame": id}`; the app moves what it
drew: the driver moves nothing. Not `moves`, paused or exited:
`not-usable`, the hint naming `ptyrc manage move --restart`. The topic
is sent to the app moved only, never broadcast; it has no snapshot,
and `ptyrc events` does not ask for it unless named. The frame is
resolved as by `frame.get` (`no-frame`, the same hint).

Python: `_cli.follow` takes it (the sprite is updated to the frame, then
followed there): `sand`, `rain`, `snow`; `_widgets.Deck` shows its tags
again on the frame: `clock`, `monitor`. The others are restarted.

### Hotkey

Replaces the demo. `Config::hotkey` (`hotkey.rs`): ctrl+t typed twice in
a row on the host keyboard, while no key is grabbed, runs that command
as a child (name `manage`); typed again while it runs, the pair ends
it (`app.stop`). Whoever is connected. That child leaves the table
when it ends. Held 350 ms, CSI u forms: as before. A
command that does not start rings the bell. `ptyrc run` and `ptyrc tmux
wrap` give `ptyrc manage --overlay` (the driver's own binary), or the
words of `$PTYRC_HOTKEY`. `--no-hotkey` (`--no-demo` is read as it),
`--transparent`: none. `ptyrc-app demo`, `$PTYRC_DEMO`: gone.

### Modal widgets

`widget.show {modal: true}`, the border member `chase` and the preset
`alert`: see "ptyrc-core: widgets", Modal and Borders.

### `ptyrc manage`

```
ptyrc manage                       on a terminal: the menu; else `ls`
ptyrc manage ls                    the apps: ID KIND STATE SINCE PID NAME ARGS
ptyrc manage start APP [OPTION...]     `ptyrc-app APP OPTION...`
ptyrc manage start --cmd PROGRAM [ARG...]   any program
ptyrc manage stop    APP... | --all
ptyrc manage pause   APP... | --all
ptyrc manage resume  APP... | --all      (`unpause`)
ptyrc manage restart APP [OPTION...]   options given: in place of the old
ptyrc manage forget  APP... | --all     exited ones
ptyrc manage move [--restart] APP FRAME   to that frame: live when it
                                   `moves`, else restarted with `-f`
ptyrc manage frames                the frames and the apps on each
ptyrc manage log APP               what it wrote on stderr
ptyrc manage --overlay             the menu in a modal widget (the hotkey)
```

- `APP`: an id, or a name: every app of that name not exited
  (`forget`: exited; `restart` with options, `move`, `log`: exactly
  one, a live one before an exited one; several: status 2, the ids
  named). None: status 4, the apps named. The name of an app run by
  `ptyrc-app` is its first word (`bricks`), told from `argv` for any
  kind (`manage::label`, `options`); else the `name` of its info.
  `--all`: all but `me` and the managers (`manage::is_manager`).
- `start` finds `ptyrc-app` as `ptyrc app` does (127 when absent) and
  sends its path in `argv`. It waits 400 ms: an app that ended by then
  with an error is status 1, the last line of its log shown.
- `start --cmd`: the name is the file name of the program.
- `restart` with options: the `argv` up to the app's name, then the
  options (`manage::with_options`); of a `--cmd` app, all that follows
  the program. A new start is watched 400 ms, as `start`.
- `stop`, `restart` wait for the end or the new start, 5 s at most
  (status 1 past it). A client stopped is `gone`.
- `move`: `app.move` when the app `moves`, unless `--restart`; else
  `frame.get` (the frame is there), then `app.restart` with `-f FRAME`
  in place of the `-f`, `-fX`, `--frame` or `--frame=` among its
  options, at their end when none (`manage::with_frame`).
- `frames`: `ID KIND SIZE KEY IDLE APPS`, the frames on screen
  (`frame.list`, not inactive); `KEY`: the key, else the preset;
  `APPS`: the names of the apps whose `frames` hold it.
- A verb prints a line an app: `ID NAME: STATE` (`exited N`, `gone`;
  `frame N` for a live move). `ls` leaves `me` out.
- `log`: the file of the info; a client has none (status 4).
- `--json`: the infos.
- No session chosen and several live, on a terminal: the menu asks.

The menu (`ptyrc-cli/src/manage/`): one model (`menu.rs`: keys in, rows
of styled runs out, for a given size; no clock, no I/O), a runner
(`ui.rs`: the actions of the model done on a `Backend`, the session or
a fake) and two fronts: `tty.rs` (this terminal, alternate screen, at
most 100x30) and `overlay.rs` (a modal widget of the session, tag and
title `manage`, border `alert`, variants 34x9, 48x13, 64x18, 78x22,
prefer centre, fill 1; `input.grab` of every key, escape ctrl+t). Keys
are named as `input.grab` names them, on both.

- Every screen: a title, the rows (the selected one an inverse bar; a
  list too long says `3/12` and shows `↑` `↓`), a line for a note or
  the help of the row, a foot line of keys. The foot drops the least
  useful keys first and always holds the way back (`esc back`, `esc
  leave`). `esc`, `q`, `←` go back; `↑↓`, `jk`, `tab`, `home`, `end`,
  page keys move. esc and a key in one read (`alt+x`) are both meant.
- Main list: the apps the driver started, then the clients from
  elsewhere (with their pid), by id: state mark (`●` running, `‖`
  paused, `○` ended), name and arguments, state, since; then `start an
  app…`, `random app`, `frames…`. A manager is not listed. enter: the
  actions of the row. Keys: `s` start, `p` pause or resume, `r`
  restart, `e` restart with other options, `m` move, `x` or `delete`
  stop (an exited one: forget), `l` log, `f` frames. The selection
  follows its app, and goes to an app just started.
- Start: the apps of the catalog with their line of help; enter: the
  form.
- Form: first the row that starts (`▶ start bricks`: enter twice from
  the list starts with the defaults); then a row per option, the frame
  last: a flag is a checkbox (`space`, `enter`), a count and a choice
  change with `←→`, a value is typed (`enter` edits, `enter` or `esc`
  ends, `backspace` clears; unset, its default shows dim); then the
  positionals and `more arguments` as typed lines (quotes as a shell).
  Wide (64 and more), the help of each option is a column. Restart
  with other options opens it filled from the `argv` running; words it
  does not know go to `more arguments`; a program that is no app of
  the catalog has that line alone.
- Frames (`f` on the main list, `frames…` among its rows): the
  frames on screen but chrome, one a row: id, kind, size, idle or
  busy, the apps on it (`app.list`, `frames`); its key or preset is the
  hint. Beside the list, from 48 columns (not at 34x9), a map of the
  screen to scale: a tile per pane with its id, the selected one
  filled; the whole screen selected: the border too. In the overlay
  the selected frame is also shown on the screen itself: a front tint
  over its view (`Action::Highlight`). enter on a frame: the apps on
  it, then their actions.
- Move (`m` on an app, `move to a frame…` among its actions): the same
  list as a picker, open on the frame the app is on; enter moves it:
  `app.move` when it `moves` and runs, its state kept; else it is
  restarted with `-f FRAME` in place of the one it had (`app.restart`,
  a child only; its state is not kept). The hint says which before
  enter: `moves live: state kept`, `restarts there: state lost`.
  Besides the frames, the names: `raw`, `active`, `largest`,
  `quietest`, `idle`. In the form the option `-f` is chosen with the
  same picker.
- The list follows the topic `apps` and is read again every second
  (the times, the frames of an app); the frames follow the topic
  `frames`. A refusal of the driver is shown on the note line. An app
  that ends with an error within 400 ms of a start shows the last line
  of its log there.

Catalog: `ptyrc-app --catalog` prints `{"apps":[{"name","help",
"options":[{"flags":["-r","--rules"],"kind":"flag"|"value"|"choice"|
"count","metavar","choices":[...],"default","help"}],"positionals":
[{"name","nargs","help"}],"random":[[arg,...],...]}]}`, from the
argparse parser of each app; `-h`, `-s` are left out. `random`: the
argument lists `random app` draws from (was `demo.VARIANTS`); empty:
never drawn. No `ptyrc-app`: the menu has no `start an app…`.

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
    fn segment(&self, area: &PaneView) -> Option<Vec<Rect>> { None }
    fn tuning(&self, pane: &PaneView) -> Tuning { Tuning::default() }
}
pub trait DrawExt: Send {
    fn name(&self) -> &'static str;
    fn functions(&self) -> Vec<FnDoc>;
    fn draw(&self, function: &str, params: &serde_json::Value)
        -> Result<SpriteSpec, String>;
    fn effects(&self) -> Vec<(&'static str, EffectFactory)> { vec![] }
    fn borders(&self) -> Vec<(&'static str, BorderSpec)> { vec![] }
}
```

A `TuiSupport` is consulted for a pane (and for the whole screen) when its
`detect` is at least 0.5. `PaneView`: `grid`, `rect`, `area` (the area
a preset segmented to make the pane; `rect` otherwise), `cursor`,
`title`, `alt_screen`. Chrome, hint, key and `Tuning` are asked per
pane: one preset may declare a pane a monitor and let another scroll.

`segment` is asked of the whole screen, then of every rectangle the
generic segmentation reaches on its way down (a tmux pane, the inside
of a box, a panel), before it is cut further. The whole screen is
asked of the preset surest of it only (the first, when equal): an app
at the top left of a tmux looks like the screen to its preset. A rule
from top to bottom inside a rectangle cuts it before a preset is
asked. Rules cut before bars:
what lies between two rules is one app's, bars included. A pane a
preset returns is asked once more of the other presets (an app in a
pane of an app); the panes of the second are final: two presets deep.
`PaneView::area` is then the pane of the first. Keys follow the key of
the screen (`tmux:in:0/htop:processes`), not that of the pane between. Presets are looked for on a rectangle as
on a pane (two rows changed, the title, else once a second); those
found are asked at every segmentation.

Presets: `tmux`, `vim`, `ranger`, `htop`, `glances`, `agent-tui` (the
family of coding-agent TUIs: a prompt box, rounded, between rules or a
panel edged by `┃`, under a streaming transcript). `ranger`: columns
from the selected entries (inverse runs), a column without one takes
what is left; title and status rows chrome; main column keyed
`ranger:<dir>`, hinted by the selected index; the preview, right of
it, a monitor. `htop`: main screen only
(`F1Help F2Setup`); meters `htop:meters` and table `htop:processes`
around the header row, function bar chrome, both monitors; no learned
chrome in the table. `glances`: a full-width rule over the `TASKS n (n
thr)` row; title and rules chrome; `glances:summary` between the last
two rules, under them `glances:sidebar` left of the column of `TASKS`
(8 cells or more) and `glances:processes`; all monitors. First draw
extensions: `shapes` (line, rect, circle, ellipse), `bigtext` (block
letters), `sand`, `rain`, `snow` (effects, all three tuned in place;
unknown parameters refused), `borders` (border presets of widgets:
`painting`, `baroque`, `mat`, `sketch`; no function). A draw extension
refuses a parameter of the wrong type.

## Tools

Debug tools for terminals: a session on tape, hands on a session, a
picture of it.

### Tapes

`ptyrc run --record FILE` writes the session as it is fed to the engine
(`ptyrc-server/src/record.rs`). UTF-8 JSON, one object per line; the
header, then records, `t` in microseconds since the driver started,
never decreasing:

```
{"ptyrc":"tape","version":2,"size":[80,24],"argv":["less","f"],
 "host_colors":{"fg":"#e5e5e5","bg":"#000000"},"presets":true}
{"t":1520,"o":"text"}       a burst of app output, valid UTF-8
{"t":1520,"ob":"base64"}    a burst that is not
{"t":9000,"i":"j"}          bytes typed into the app (`ib`); --record-input
{"t":9100,"size":[100,30]}  a resize
{"t":9200,"host_colors":{"fg":"#c0c0c0","bg":"#101010"}}  told by the host
{"t":9900,"exit":0}         the app exited
```

A burst is recorded with the time the engine was given. A reader skips
unknown members and records; versions 1 and 2 are read, another is
refused. A write that fails ends the recording, not the session.

The header holds what makes the replay the same session (version 2):
the size, the default colours of the host at the start (`--host-fg`,
`--host-bg`, else the defaults; what the host answers later, or a
client sets with `session.colors`, is a record), and whether the per-app presets were on. A replay takes them
from the tape; `--no-presets` and `--presets` say otherwise. A version 1
tape has neither: the defaults, presets on.

`ptyrc replay FILE` is a headless session with no app: the tape is fed
to the same engine, behind the same socket, and the clock of the driver
is the tape's (`player.rs`). Every method answers as on a live session;
`input.send` and `input.click` are refused (`bad-request`); sprites are
drawn; `session.info` has `"tape"`: the status.

- Played: tape time runs at `--speed` tape seconds per second (1).
- Not waiting (`--speed 0`, a seek): the clock jumps to the next record
  or to the engine's next deadline, whichever is first. Ticks happen at
  their deadlines, bursts at their recorded times: the same tape gives
  the same frames, on any machine.
- Paused (`--step`, `--until T` reached, `--hold` at the end): the clock
  stands still, and with it every timer of the driver. A `screen.wait`
  is then held to the real clock: it is answered when what it waits for
  is there (at once, or when another connection moves the tape), and
  times out `timeout_ms` of real time after it was asked. `stable_ms`
  and `idle` are measured in tape time: paused, they time out. Not
  paused, a wait times out in tape time. A seek past the end of the
  tape lets time pass with nothing fed.
- At the end of the tape, not held: the session ends with the status of
  the tape's `exit` record.

`ptyrc tape` moves a replay: `status`, `step [N]`, `seek T`, `play
[SPEED]`, `pause`, `quit`. A tape time is seconds, `end` or `end+N`.
A seek back replays from the start, without waiting, to the asked time:
the driver makes a new engine, so the frames are those of a first run
(the same snapshot, byte for byte). Sprites do not survive it; a
request the old engine held (a `screen.wait`) is refused, `not-usable`.

Snapshot (`ptyrc snap`, `Pilot.snapshot()`): what a session shows and
how it is segmented, without ids nor times: `{"size","alt_screen",
"title","cursor":[x,y,visible],"text":[...],"frames":[{"kind","state",
"usable","rect","origin","extent","key","support"}]}`.

Regression: `ptyrc replay FILE --snap [--at T,...]` prints the snapshots
at those tape times (default `end+3`), each with its `t`; `--check SNAPS`
compares with a file written so, at the times it holds, and names each
difference (`[1].frames[2].origin: expected [0,18], got [0,30]`), status
1. `rust/ptyrc-cli/tests/tapes/`: `less` (pages, lines, a page back,
`G`, `g`), `tmux` (a split, a second window, back), `vim` (half pages,
`:split`, `G`, `gg`, the split closed), `htop` (meters, the setup
screen, back), `ranger` (a directory entered and left), recorded and
snapped by `scripts/record-tapes.sh [--snap] [NAME...]`, held by
`tests/tapes.rs`. A tape holds no user name, no host name (replaced by
words of the same length), no path under /home. A change of the frames
that moves a snapshot is read in the diff of the `.snap`.

### Hands

- `ptyrc attach`: the remote screen here (as `show`, with the cursor)
  and the keys of this terminal there, as bytes. `--detach KEY`
  (`ctrl+t`) leaves; it is not sent, nor what was typed after it in the
  same read. The remote size is not changed: a larger remote screen is
  cut at this terminal's size; `--resize` gives a headless session this
  terminal's size, at the start and each time this terminal is resized
  (looked at every 50 ms). This terminal is given the modes of the app
  (`session.info`, `modes`: DECCKM, the mouse modes, 1006, bracketed
  paste), with each redraw and four times a second when nothing is
  drawn: arrows, clicks and pastes are typed as the app reads them.
  They are reset on leaving. `--sprites`: what the host shows, looked
  at every 50 ms.
- `input.grab`: one client takes keys of the host keyboard before the
  app: those named, or all. Each key taken is an `input` event for that
  client alone, subscribed or not: `{"key","bytes","grabbed":true}`,
  `key` its name as `input.send` takes it, a printable character
  itself, null for what has no name (a mouse report). Keys not taken go
  to the app. The escape key (`ctrl+t`, or `escape`: a named key that
  is not text) always releases: it reaches neither, the client gets
  `{"released":"escape"}`. A key the host terminal sends as CSI u
  (kitty keyboard protocol) or `CSI 27;m;c~` (modifyOtherKeys) is
  named, taken and matched as its legacy bytes; a release event is not
  the key. A grab ends with `input.release` or with its
  connection. Keys typed by clients (`input.send`) are never grabbed.
  One grab at a time (`not-usable` for another client).
  `scripts/grab-sprite.py`: the arrows move a sprite in vim's pane; vim
  does not see them.
- `Config::apps`: commands started as child apps once the program has
  started (Apps and the manager, The table).
- The hotkey (`hotkey.rs`): ctrl+t typed twice in a row on the host
  keyboard, while no key is grabbed, runs `Config::hotkey` as a child
  app; the pair reaches nobody (Apps and the manager, Hotkey). A first
  ctrl+t is held 350 ms, or until the next key: alone it goes to the
  app, late. CSI u and modifyOtherKeys forms count; a release event
  between the two does not break the pair.
- `input.click`: a click at a screen cell, written as the app asked:
  SGR (1006) or X10 bytes, press and release, press alone under mode 9
  or for the wheel. `mods`: `shift`, `alt`, `ctrl` held. `count`: 2 is
  a double click (the pair, twice). `to`: a drag, released on that
  cell, with one move between when the app follows drags (1002, 1003).
  `not-usable` when the app asked for no mouse report, or for a cell
  X10 cannot tell (past 222).
- Host mouse (`mouse.rs`): while a client is subscribed to `mouse`, the
  driver asks the host terminal for reports (1002, 1006; asked again
  after a burst that sets a private mode) and takes them out of the
  keyboard stream, SGR or X10, before a grab. A report on a sprite (the
  topmost whose box holds the cell; not an effect) is a `mouse` event
  and does not reach the app; after a press on a sprite, the moves and
  the release are that sprite's, wherever they are. Any other report
  is written again as the app asked (X10 for an app without 1006), or
  dropped when it did not ask (no mouse mode, a drag without 1002).
  The last listener gone, the host gets the app's modes back. A report
  cut by a read is not seen. `scripts/click-sprite.py`: a sprite button
  that counts its clicks.
- `ptyrc shot`: a picture of the screen or of a frame's view: `--svg`
  (default; one `<text>` per run with its length fixed, wide glyphs
  alone), `--html` (one `<pre>`), `--ansi`. The app alone, or with
  `--sprites` what the host shows; default colours are the host's. A
  visible cursor is its cell in inverse (`--svg`, `--html`;
  `--no-cursor`). No PNG: it needs a font rasterizer.
- `screen.text` and `screen.cells` take `"sprites": true`: the composed
  grid in place of the app's.

## Protocol

Unix stream socket `$XDG_RUNTIME_DIR/ptyrc/<session>.sock` (directory 0700,
socket 0600), `/tmp/ptyrc-<uid>/` without `XDG_RUNTIME_DIR`. `<session>` is
`--name` or the driver pid. The driver exports `PTYRC_SESSION` and
`PTYRC_SOCKET` to the app. A client picks, in order: `--session` /
`PTYRC_SESSION`, else the live session its tmux session names (`@ptyrc`,
see `ptyrc tmux`), else the only live session, else fails with the list.

Durations on the wire (`timeout_ms`, `ttl_ms`, `ms`, ...) are session
time: real time times `$PTYRC_CLOCK_RATE` (default 1), read by the driver
at its start and by `ptyrc.clock` at import. Programs in the pty keep real
time. Tests run at 5.

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
| `session.info` | | hello data, `alt_screen`, `title`, `driver_pid`, `headless`, `grabbed`, `handoff` (a successor waits), `tape` (a replay), `"modes":{"app_cursor","bracketed_paste","mouse":[1000,...],"mouse_sgr"}`, `"host_colors":{"fg","bg","from":{"fg","bg"}}` (`from`: `host`, `option`, `client`, `default`) |
| `session.resize` | `size`: `[c, r]` | `{}`; headless only |
| `session.colors` | `fg`, `bg`, `ask` | `host_colors`, and `asked`; see Rendering |
| `session.handoff` | `hangup` (true) | `{}`; the end of the app is a handoff: the driver ends as ever, then its caller puts a successor on the host terminal. `hangup`: SIGHUP to the app, SIGKILL after 1 s; false: the client makes the app leave. `bad-request` without a successor (`ptyrc run`) |
| `screen.text` | `frame`, `rect`, `ansi`, `sprites` | `{"lines":[...],"size":[c,r],"rect":[x,y,w,h]}` |
| `screen.cells` | `frame`, `rect`, `sprites` | `{"rows":[[run,...],...],"size":[c,r],"rect":[x,y,w,h]}` |
| `screen.find` | `pattern`, `regex`, `ignore_case`, `frame`, `known` | `{"matches":[{"frame","x","y","len","text","screen":[c,r]\|null}]}` |
| `screen.cursor` | `trail` | `{"x","y","visible","frame","pos":[x,y],"trail":[[ms,x,y],...]}`; `frame`: the innermost frame showing it, `pos`: its cell there |
| `screen.wait` | `pattern` or `stable_ms` or `idle` (or `gone`), `timeout_ms`, `frame` | `{"matched":bool,"match":...}`; with `idle`: `{"matched":true,"match":null,"frame":id,"idle":{"score","idle","free"}}`, the frame name resolved at each look |
| `input.send` | `text` or `keys` or `bytes` (base64), `paste` | `{}` |
| `input.click` | `x`, `y` (screen cell), `button`: `left`, `middle`, `right`, `wheel-up`, `wheel-down`, `wheel-left`, `wheel-right`; `mods`: `shift`, `alt`, `ctrl`; `count` (1 to 3); `to`: `[x, y]` | `{}` |
| `input.grab` | `keys`: names (absent: all), `escape` (`ctrl+t`) | `{"escape":name}` |
| `input.release` | | `{}` |
| `tape.status` | | `{"t","end","records","next","paused","speed","done"}`, seconds; a replay only |
| `tape.step` | `records` (1) | status, once paused there |
| `tape.seek` | `t`: tape seconds; before now: replayed from the start | status, once paused there |
| `tape.play`, `tape.pause` | `speed` | status |
| `tape.quit` | | `{}`; the replay ends |
| `frame.list` | | `{"frames":[info,...],"layout":id}` |
| `frame.text` | `frame`, `y0`, `y1` | `{"lines":[...],"y0":n}` remembered buffer; default: the island on screen |
| `frame.heat` | `frame` | `{"rows":["0123...",...],"classes":["ht.p...",...]}` |
| `frame.idle` | `frame` | `{"score":0..1,"idle":bool,"free":0..1}` |
| `frame.get` | `frame` | `{"frame":info}` |
| `frame.at` | `x`, `y`, or `name` | `{"frame":id,"pos":[x,y]}` |
| `draw.put` | sprite | `{"id":n}` |
| `draw.update` | `id`, patch; `effect`: tune the running effect, or start another | `{}` |
| `draw.remove` | `id` | `{}` |
| `draw.clear` | `all` | `{"removed":n}` |
| `draw.list` | | `{"sprites":[{..., "style":{...}, "screen":[x,y]\|null, "visible":bool, "covered":0..1}]}`; `style`: that of the content, of the first frame, of the first run; absent when none or an effect |
| `draw.ext` | `name`, `function`, `params`, sprite fields; with `id`: redraw that sprite in place (content or effect replaced, the rest kept unless given) | `{"id":n}` |
| `draw.exts` | | `{"exts":[{"name","functions":[...]}],"effects":[...]}` |
| `layer.define` | `name`, `z` | `{"layers":[{"name","z"}]}`; see Layers |
| `layer.list` | | the same |
| `widget.declare` | `widgets`: `[{tag, name, size, content}]`, `border` | `{"tags":[...]}`, the connection's; see Widgets |
| `widget.show` | `tag`, `frame`, `prefer`, `fill`, `title`, `border`, `layer` | `{"widget":info}` |
| `widget.set` | `id`, `variant`, `content`, `title` | `{}` |
| `widget.hide` | `id` | `{}` |
| `widget.list` | | `{"widgets":[info,...]}`, every connection's |
| `widget.borders` | | `{"borders":[name,...]}` |
| `app.list`, `app.start`, `app.stop`, `app.pause`, `app.resume`, `app.restart`, `app.forget`, `app.move` | | see Apps and the manager, Methods |
| `batch` | `calls`: `[{"method","params"}]` | `{"results":[{"result"} or {"error"},...]}`, applied in one repaint |
| `subscribe` | `topics`: `screen`, `cursor`, `frames`, `text`, `idle`, `draw`, `input`, `mouse`, `size`, `colors`, `widgets`, `apps`, `move`, `exit`; `snapshot` | `{}`; with `snapshot`, then the state now as events, to this client: `idle` per frame on screen, `cursor`, `frames`, `size`, `widgets` per widget, `apps` per app |
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

Event `idle`: `{"frame","idle","score","free"}`, per frame on screen,
when `idle` changes. The watch runs while subscribed or while a
`screen.wait {idle}` is pending. It looks when the heat says the first
busy frame can be idle (`HeatView::idle_in`: the time for the share of
still cells to reach 0.8 if nothing changes), and 250 ms at the
earliest after a look when the screen or the layout changed; an idle,
quiet screen costs no wake-up. A frame is not idle until seen so; a
late subscriber asks for a snapshot. Event `cursor`: the result of
`screen.cursor` without the trail, when the cursor moved on the screen
or in its frame (a scroll under it). Event `text`:
`{"frame","rows":[y,...],"origin":[x,y],"rect":[x,y,w,h]}`, per frame
on screen and per burst: the frame rows in view whose text is not what
the frame last showed there, and the view; sent when a row changed or
the view moved. A row that comes into view is not a change; styles are
not text. The base is the screen when the first client subscribes.
Event `draw`:
`{"removed":[ids],"frame","rows":[from,to]|null}`, sprites removed with
a collected island (`rows`) or a dropped frame (`null`). Event `mouse`:
`{"sprite","button","action","mods":[...],"at":[x,y]|null,"x","y",
"frame","pos":[x,y]|null}`, a report of the host mouse on a sprite:
`button` as in `input.click`, or `none`; `action`: `press`, `release`,
`drag`, `move`, `wheel`; `at`: the cell in the sprite, `x`, `y`: the
screen cell, `pos`: the cell in `frame`; `at`, `frame` and `pos` are
null for a drag or a release outside the sprite.

## CLI

`ptyrc <verb>`; `-s/--session` on every client verb; `--json` where a human
form exists.

```
ptyrc run [--name N] [--headless --size CxR] [--host-log F]
          [--transparent] [--no-presets] [--coalesce-ms N]
          [--host-fg COLOR] [--host-bg COLOR]
          [--record FILE [--record-input]] [--no-hotkey] [--app APP]...
          [--] CMD...
ptyrc replay FILE [--name N] [--speed X] [--until T] [--step] [--hold]
          [--no-presets | --presets] [--snap | --check SNAPS] [--at T,...]
ptyrc tape [status | step [N] | seek T | play [SPEED] | pause | quit]
ptyrc snap                        what is shown and the frames, JSON
ptyrc ls                          sessions
ptyrc text [-f FRAME] [--ansi]    screen as plain text
ptyrc find [-E] [-i] PATTERN      matches with coordinates
ptyrc cursor
ptyrc send TEXT                   type text
ptyrc keys KEY...                 press keys
ptyrc wait [PATTERN] [--stable MS] [--idle] [--gone] [--timeout MS]
ptyrc frames [--watch]
ptyrc heat [-f FRAME] [--classes]
ptyrc widgets [--watch]           widgets, their variant and place;
      --watch: then one line per change
ptyrc idle [-f FRAME]             score; status 0 when idle, 1 when not
ptyrc draw text|ext|effect|exts|rm|ls|clear ...   (sprites persist)
      [--layer back|front|N] [--blend normal|tint] [-z N]
      draw ext --id SPRITE NAME FUNCTION [JSON]   redraw in place
      draw effect --id SPRITE NAME [JSON]         tune in place
ptyrc show                        live mirror of the remote screen
ptyrc attach [--detach KEY] [--resize] [--sprites]
ptyrc grab [KEY...] [--escape KEY]   grabbed keys, one JSON line each
ptyrc click X,Y | --text [-E] [--nth N] PATTERN  [-b BUTTON]
      [-m MOD]... [--count N] [--to X,Y]
ptyrc shot [--svg|--html|--ansi] [-f FRAME] [--sprites] [--no-cursor]
      [-o FILE]
ptyrc events [TOPIC...] [--snapshot]   JSON events, one per line;
      all topics but `mouse` when none is named
ptyrc debug overlay frames|heat|off ; ptyrc debug dump
ptyrc call METHOD [JSON]          raw request
ptyrc app [NAME [OPTION...]]      a client app: `ptyrc-app NAME...`
ptyrc python [FILE [ARG...]]      Python with `pilot` connected:
      a console, or FILE run as `__main__`
ptyrc manage [ls | start | stop | pause | resume | restart | forget |
      move | frames | log]        the apps of the session; alone: a
      menu (Apps and the manager)
ptyrc tmux attach [-t SESSION] [-L NAME | -S PATH] [--app APP]...
ptyrc tmux detach [-t SESSION] [-L NAME | -S PATH]
```

`ptyrc app` executes `ptyrc-app` with its arguments as given, `ptyrc
python` `ptyrc-python`; `-s` becomes `$PTYRC_SESSION` and unsets
`$PTYRC_SOCKET`. The program is taken from `PATH`; else from `.venv/bin`
or `venv/bin` in the current directory or one above it; else from the same
above the `ptyrc` binary. None: status 127.

`ptyrc run --app APP`: `ptyrc-app` (found as above) with the words of APP
is a child app of the session once the program has started (Apps and
the manager). Repeatable. Not with `--transparent` (status 2); no `ptyrc-app`:
status 127, nothing runs. The options of `run` come before the program: a
first word that starts with `-` is one (unknown: status 2); `--` before a
program that does.

### tmux

`ptyrc tmux attach` puts a driver under a running tmux session; `ptyrc
tmux detach` takes it away. The session stays in the tmux server: only
its client is replaced, on the same terminal. Windows, panes and size
are those of before; the screen is redrawn once.

- The session: `-t`, else the one of the pane the verb runs in
  (`$TMUX_PANE`), else the one tmux takes. The server: `-S`, `-L`, else
  that of `$TMUX`, else tmux's default.
- `attach`: the client of the session last active is replaced
  (`detach-client -E`) by `ptyrc tmux wrap` (hidden): a driver named
  `tmux-<session>` (`-2`, `-3`... when taken; what a name cannot hold is
  `_`) whose app is `tmux attach-session -t <id>`, as `ptyrc run` with
  its defaults. The verb returns when the driver answers, 5 s at most.
  The tmux session names the driver in its option `@ptyrc`, unset when
  the driver ends: client verbs typed in a pane need no `-s`.
- No client on the session and a terminal that is not a pane of its
  server: the driver starts here, in place of the verb. In a pane, or
  without a terminal: status 3.
- Attached already: nothing is replaced, status 0. One driver per tmux
  session.
- `--app APP`: `ptyrc-app` with the words of APP, a child app of the
  driver (`app.start`), once the driver answers, and on a session
  attached already. One that ends within 400 ms with an error: status
  1, the last line of its log shown, the session stays attached. No `ptyrc-app`: status 127,
  nothing is replaced.
- `detach`: `session.handoff`, then the client leaves through tmux,
  without a word on the terminal (hung up when tmux does not know it);
  the driver ends and executes a plain `tmux attach-session` in its
  place. Not attached: status 0, nothing is done. `-s` names the driver.
- Detached by tmux (`prefix d`), or the session killed: the driver ends
  with its client, as tmux would; nothing takes its place.
- The driver that does not start, or that breaks: a plain client is
  executed in its place, and `attach` fails with its message (status 5,
  carried by the option `@ptyrc-error`). The terminal is never left
  without a client by a failure of ptyrc.
- The hooks `client-detached` and `client-attached` of tmux run at each
  replacement. Other clients of the session are not touched.
- Code: `ptyrc-cli/src/tmux.rs` (the verbs), `ptyrc-tui-tmux::server`
  (the tmux server, asked through the `tmux` program),
  `ptyrc-proto::session` and `ptyrc._wire` (`@ptyrc`). The one place
  where the binary and the clients name a program: decided with the
  owner.

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
| 1 | a negative answer: `find` without a match (silent), `wait` timed out (says what was waited for), `click --text` without a match, `replay --check` with a difference (names each) |
| 2 | usage, or a request that is wrong: a bad key, colour, anchor, position, JSON, method, topic, session name for `run` |
| 3 | no session to talk to: none, not found, several and none chosen, dead (stale socket, removed), bad name; `run`: the name is taken, no socket directory |
| 4 | the request names what is not there or not usable: frame, sprite, widget, app |
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
boolean; both ask the driver when read. `Pilot.wait_idle(frame, timeout)`
and `Frame.wait_idle(timeout)`: one `screen.wait {idle}`, no polling.
`Sprite.visible`. `Sprite.tune(**params)`: `draw.update {id, effect}`
under the name of the sprite's running effect, known from the call that
started it, else asked once (`draw.list`). `Pilot.host_colors`; `spawn(host_fg=, host_bg=)`;
`alpha=` in `sprite_spec` and `draw`. `subscribe(*topics, snapshot=)`;
`events(*topics, timeout=)` ends on silence, `event(timeout)` returns
None on silence; both raise `PtyrcError` (`gone`) on a closed
connection. Inside `batch()`, `text`, `find`, `sprites` and
`Sprite.info` return a container filled when the batch is sent.
`Cursor.pos`. Widgets: `Pilot.widget(tag, sizes, *, border=, title=,
frame=, prefer=, fill=, layer=, modal=)` declares the tag and shows it: `sizes`
is a list of `(w, h)`, or a dict of names to `(w, h)`; it returns a
`Widget`: `id`, `tag`, `info`, `variant` (the name shown, or None),
`size`, `rect`, `set(rows, *, variant=, title=, **style)`, `hide()`,
`placed`, `over`, `refresh()`, `update(info)` (the data of a `widgets` event;
false for another widget's). `content=`: a function of `(w, h)`, the
first rows of each variant. Rows are strings or rows of runs
(`widget_content`). `Pilot.widgets()`: the infos; `Pilot.borders()`:
the names.
Tools (`ptyrc/tools.py`):
`Pilot.locator(pattern)` and `Frame.locator(pattern)` give a `Locator`,
a place named by the text there and looked up at each use: `.nth(n)`,
`.right(n)` (from the last cell of the match), `.left(n)` (from the
first), `.above(n)`, `.below(n)`, then `.wait()`, `.read(width)`,
`.click(button, mods=, count=, to=)` (`to`: a locator or a cell),
`.draw(rows)` (in frame cells), `.pos()`, `.screen()`.
`Pilot.click`, `Pilot.clicks(*sprites, timeout=, presses=)` (the
`mouse` events on those sprites, presses only by default),
`Pilot.on_click(sprite, handler)` (a thread that reads them),
`Pilot.grab`, `Pilot.release`, `Pilot.apps`, `Pilot.app_start(argv,
name=)`, `app_stop`, `app_pause`, `app_resume`, `app_restart(app,
argv=, name=)`, `app_forget`, `app_move(app, frame)` (`app.*`, one to
one; the info, the frame id for a move), `Pilot.shown` (text with
the sprites), `Pilot.snapshot`; `Pilot.spawn(record=FILE)`,
`Pilot.replay(FILE)` (paused at the start) and `pilot.tape` (`step`,
`seek`, `play`, `pause`, `status`, `end`).
`plugins/client-apps/python`: `ptyrc-apps`, one module per app,
`python -m ptyrc_apps.<name>` and `ptyrc-app <name>`. Every app takes
`-s SESSION` and `-f FRAME` and runs until interrupted or the session
ends (no time limit option; tests pass `main(argv, seconds=...)`); its
sprites die with its connection, but for those of `confetti --one-shot`.

Shared by the apps (`_cli.py`, `_arcade.py`):

- `Sight(pilot, frame, cursor=False)`: a frame kept by events. It
  subscribes to `frames`, `text` (and `cursor`) with a snapshot;
  `look()`, called at each tick, takes the events waiting without
  blocking: `frames` replaces the frame's info, `text` sets its origin
  and rect and marks the text stale, `cursor` gives the cursor in frame
  cells (`pos` when the event names the frame, else `to_frame`). The
  text of the view is read (`screen.text`, into `rows`; `changed` true
  for that look) only when stale. `look()` is false while the frame is
  not usable; it then asks `frame.get` at each look (a raw frame alone
  settles with no event). `drawing()`: a context that drops a step
  refused with `not-usable` and marks the frame so. A quiet screen
  costs an app no read: `snake`, `bricks`, `fishes`, `flower`.
- `follow(pilot, frame, sprite, seconds, beats)`: stays connected on the
  topics `frames` (with a snapshot) and `exit`; when its frame is gone
  or inactive, moves the sprite to its `heir`: a usable pane of the
  same key over half of the smaller of the two, the old rectangle
  first scaled by how much the screen grew since the frame was last
  stable (two stacked tmux panes, 30 rows to 20: new frames, and the
  lower one's place overlaps its old rectangle by less than half).
  `beats`: an iterator of waits, the app's own work between two events
  (`snow`: a gust after each). `sand`, `rain`, `snow`.
- `Deck(pilot, args)` (`_widgets.py`): the widgets of an app, on the
  topics `widgets` and `exit`. `add(tag, sizes, render)` declares and
  shows a tag, every variant rendered once (`render(w, h)`: the size
  is the key, one variant a size); `run(period, seconds, tick, align)`
  renders the variants shown once a period, in one batch, after the
  app's `tick`; a variant just put is rendered at once, a widget only
  moved is not; `resize(widget, sizes)` declares its tag again.
  `options(parser)`: `--border`, `--prefer`, `--front`. `clock`,
  `monitor`.
- `Board(pilot, frame)`: a score, one sprite top right in view cells,
  `z` 5, blanks opaque (a racket under it does not show through); sent
  when it changes. `snake -e`, `bricks`.

| app | default frame | what runs where |
|---|---|---|
| `confetti` | `raw` | A round: `--bursts N` bursts (1, at most 10), each at its own offset in [0, 3 s); one queue of timed actions in one thread. Plain, a burst is four shots, one per colour (bright green, magenta, blue, yellow), each 0 to 1 s later by tenths; a shot is a pop, the one of the first ptyrc: a finite animation of 3 by 3 cells, 4 frames of 100 ms, centred on its cell, in view cells. `--fancy`: a burst is one shot, the native effect `confetti`, `--size` pieces for `--duration` seconds. `--random`: each pop is one of 5 kinds (1 by 1 to 7 by 5 cells), 70 to 140 ms a frame, of a random bright colour; with `--fancy`, each explosion has 0.4 to 1.8 times the size, 0.7 to 1.2 times the duration, a `spread` of 0.5 to 1.3, a random `shape`, and half of them one or two colours. `--fireworks`: a shot starts with a rocket, one sprite: a column from the last row of the view up to the cell, a `motion` braking at a constant rate to a stop after 0.45 s plus 0.045 s a row (at most 1.6 s; 0.85 to 1.15 times that), a finite animation of its head and of a trail of at most 3 rows, as long as it is fast, one of 5 looks drawn at random; the pop or the explosion (`shape` `ball` unless `--random`) is sent when it stops. `--sparkle`: the effect `sparkle` on layer -2. `-a`: those four. Each shot reads `frame.heat` once and aims at a random cell weighted by the share of non-hot cells within 3 of it, hot cells excluded unless all are, never within 3 of the last origins (as many as a round has shots, at most 10; the oldest forgotten first when the view is too small); with `--fireworks`, in the upper two thirds of the view. A round every 3 s until interrupted, sprites tied to the connection; `--one-shot`: one round of `persist` sprites, the app exits after the last is sent. Back layer unless `--front` |
| `clock` | `raw` | one widget, tag `clock`: 9 faces from 5x1 to 41x8 (8 with `--no-seconds`, 5 with `--big`: block digits only), each a pure function of the time and its size: `HH:MM`, with seconds, over the date, block digits of 3 and 5 rows, a braille dial, the largest with the seconds, the date and the week; `--tz`, `--utc`; `_widgets.Deck` draws the face shown on the second (on the minute with `--no-seconds`), a face just put at once; round border; back layer unless `--front`; `--print`: no driver, every face once |
| `flower` | `largest` | client, on a `Sight`: every 0.5 s it tends the garden; the word is looked for (`find`, case ignored) in the view when the text changed or the words did, not at each look; it wilts the flowers whose match left it or whose cells show text; while fewer than `--count` (1; 0: one per match), plant one on a random match without a flower, at most one a scan. Random size (3 shapes, 3 to 7 rows; a smaller one where it does not fit) and colours (256-colour petals, two-tone at times, heart, stem). Root 3 to 10 columns from an end of the word, 1 to 3 rows above or below; every cell of the shape cold (`heat_classes` `.`), in the view, off the other flowers and a cell away from every match; nearest first, rows counting double, ties at random. The match is redrawn bold with a `tint` sprite. Growth: two or so steps 0.35 s apart, sent by the client; then a sway animation played by the driver, random phase and pace; wilting: a finite animation, removed by the driver. With matches to spare, a flower wilts after 20 to 40 s. No cell of a flower between two cells that are not cold: never in the gap between two words. `-a` (`--anywhere`): in place of `--word`, a pool of 3 to 5 words of 4 letters or more drawn from the view, matched whole (`\b`, one regex `find`); a word leaves the pool when it leaves the view, or when none of its matches has a flower or room for one, and another is drawn; matches of the words with the fewest flowers first; `--count` defaults to the pool size; no bold. Frame coordinates; back layer unless `--front` |
| `fishes` | `largest` | 8 fishes, back layer; positions in frame coordinates, computed by the client, one batch every 100 ms; flee the cursor. Half of them, rounded down and drawn at random at birth, are shy (`--shy N`: N); a shy fish heads for the nearest spot of the view where the text hides at least 5 of its 6 cells, out of reach of the cursor (`hideouts`), slows down there, stays 4 to 10 s, then heads for a random spot at least 12 cells away (`lure`); fear first. On a `Sight` with the cursor: the text and the cursor are told, not asked; the reef is looked at when the text is read and once more 1 s after a scroll. A fish out of the view stays still; after a random 2 to 5 s it goes, at most one a tick (`tide`), and comes back in the view at the spot where the text hides most of it, ties at random, a side of the view when there is no text (`entry`). Back in view before its time, it carries on. Options, `-a` for all; everything behind the text, layers from the back: bubbles (-4), crossers (-3), decorations (-2), fishes and the other decorations (-1). `-d`: decorations (`_reef.DECOR`: kelp, shells, corals, a castle; some animated by the driver) standing on text: a box free with a free column each side, ink under two cells of its base out of three (`stand`). Each row of text of the view is surveyed once, the last row never, a row within 5 of the top once the view has been still for a read; 30 % of the lines get one, drawn by weight times height squared among those that fit. `Reef` remembers, in frame cells, the rows surveyed and what stands there; sprites exist within a view's height of the view. One whose text is gone is forgotten and its row surveyed again. `-b`: an area of 12 rows has a vent every 20 columns, drawn once; the vents of the area of the view's last row blow from that row, bursts of 2 to 5, 2 to 7 s apart; a bubble is one sprite: a finite animation (a `.` wobbling in 3 cells, dim) with a `motion` of 1.2 to 2.2 rows a second, gone at the top. `-r`: a fish that comes back is of a random species (`SPECIES`, 3 to 9 cells, one row); one event at a time, 8 to 20 s after the last: a crosser (`CROSSERS`, one sprite with a `motion` and a `ttl`; the jellyfish rises), an object (`SUNK`) falling from the top of the view down a free column, 3 rows or more, onto text, then kept by the reef, a hook down a free column, 5 to 9 s there, then up, or a bold fish leaving by the nearest side. Fishes flee the hook's tip and the grouper's mouth as a still cursor |
| `snake` | `largest` | client, on a `Sight`: the walls are made again when the text is read; breadth-first search over the free cells of the view, one batch per move (12 a second); frame coordinates. An apple goes in the region of the head (a new snake: in the largest), off the dead ends (`room`): a cell no cycle of free cells passes through or leads to, and a side of a bridge holding fewer than twice the snake grown plus 10 cells when the other side holds more; with no such cell, off the bridge rule; then anywhere free. When the view or its text changed, an apple the head can no longer reach, the text a wall, goes elsewhere. A head out of the view or written on makes a ghost: the body is shifted, unseen, so that the head is at most a cell out of the view; it moves a cell a step, along the longer axis first, through text, its body, beams and foes, towards the nearest cell (then the lowest `(x, y)`) of `room` of the largest region off its body (with no such cell it waits), and is solid again there; drawn dim, never bitten or burnt; the apple is then checked for reach from the head. The snake's sprite has `z` 4, over foes and particles. Options, `-a` for all; rules count in steps, the plain game of a seed is unchanged; all behind the text, `z` from the back: beams, snake and apple, foes, particles. `-e`: an apple eaten is a burst of 10 particles (`_arcade.spark`: one sprite, a finite animation with a `motion` and a `ttl`) and a lump that goes down the body two segments a step; one apple in 6 is golden: 5 segments in place of 2, 26 particles; a lost tail crumbles, 40 particles at most; the body is shaded from head to tail; the apple pulses; a `Board`: the length of the snake and the longest it was. `-x`: 2 foes at most, one 60 to 140 steps after the last, born 8 cells or more from the head. A foe moves every other step towards the middle segment of a body of 4 or more (breadth-first), at random else; reaching the body, it cuts it there and is gone. Foes are walls to the snake. `-l`: 2 beams at most, one 80 to 200 steps after the last: through a segment of the snake (half of them) or a free cell, along its row or column from wall to wall, 5 cells or more. Announced 14 steps, then firing 5: the snake is cut at the first segment in it (the head: it starts again), foes in it are gone. The snake takes an announced beam for a wall unless there is no other way; a firing one always. Foes and beams scrolled out of the view or written on are dropped |
| `bricks` | `largest` | client, on a `Sight` (walls made again when the text is read): a tick 32 times a second, a step and a batch every other tick (`--speed`: steps a second, 16), a batch in between only for the balls shown half way; frame coordinates; no one plays; the same seed gives the same game, the rules have no clock: they count in steps, `second` being the steps in a second. Map: a glyph of the view is a wall (`snake.walls_of`) for a ball; the sides of the view are walls; the bottom is open; the top is a wall with one team (the racket keeps the lower half: it comes back three quarters down the view, waits under the ball, a ball is served up), open with two. Ball, half a cell high, `▀` or `▄` by the half of the row it is in: a position in fractions of a cell; a move is 0.5 row and at most 1 column, along x, then along y, and an axis that would enter a solid cell is reversed instead (`fly`); 12 eighths of a move a step, an eighth more for each brick broken in a row since a racket (4 at most), 6 more when fast. Brick: 3 cells, one row, `⟦=⟧`. Racket `(━━━)`, 5 cells, `(═══)` for 2 steps when hit; never on a brick or another racket; text does not stop it: sprites are behind the text, a racket under it is hidden and its cells there are not solid, the ball meets the text; 2 cells a step along a shortest path, rows first (breadth-first over the places where it fits); hit from above or below, it sets the ball's `vx` by where it is hit, from an end only reversing it. The racket's choices, each step. It foresees each ball 100 steps with no racket, on a copy of the terrain: bricks hit lose lives, bombs blast, a group is gone when due. A ball that leaves: it goes to a row the ball enters, over or under it: the latest it reaches in time, on the ball's last leg and with 6 moves to spare if any, else the row where it is the least late; of the places on the three best rows, the one whose shot is worth the most: the shot is flown 50 steps; a brick broken counts 1, less with time (0.25 at least), a hit that does not break 0.3; less 0.25 when the ball then leaves by a side it keeps and it can be there, 20 when it cannot; a cell of the racket under text costs 0.02. Once 60 shots were weighed in a step, the best row only, and no ball is hit on purpose. With time in hand (twice the way and 6 moves) it takes a good capsule on the way. Else, in order: out of the 6 rows under a falling brick; a good capsule; every 4 steps, a ball that is safe, within 40 steps: hit on purpose when the shot is worth 0.5 brick more than the ball left alone; else it waits under the column of the nearest ball, 4 rows past the rows the ball is about to cross, and with no ball where it is: a cell under text counts for 4 columns, and it moves only for 3 columns or more. A shot chosen is kept while the ball comes as foreseen. Score: 10 a brick, 2 more for each brick before it in a row (5 at most), shown top right in view cells; the last ball out sets it to 0, a spare one costs nothing; a ball is served 6 steps after the last is gone, from a side of the racket, towards the far side of the view (up with one team), from the nearest free cell when the racket is under text. A ball 160 steps without a brick gets another angle; at 480, when not alone, it is gone. Terrain: bricks in frame cells; sprites within a view's height of the view. Groups, one at a time, while the view has fewer bricks than one a 48 cells (8 to 240); that many are laid in 12 s, the wait after a group counting its bricks, five times slower once the view has 55 % of them. A group is large when one fits: a medium group, the first that fits of: a wall of 2 rows by 3 to 5 bricks, a block of 2 by 3, a row of 4 to 6, a pile of 3 or 4 rows by 2; once in five a big wall is tried first: 3 or 4 rows by 6 to 8 bricks, no more bricks than the view takes, full (one in two) or, but for its row on one side, a checker, a frame or a slope; a colour a row; where none fits, a small group of 2 or 3 (a row, a pile). For each, 60 places are drawn, kept when free of text, bricks, rackets (and two rows about them) and of the next 60 moves of the balls, with free cells next to them (2; for a large group of two rows or more, as many as its longest row has cells) leading to 40 more; a large group touches no other brick; of the first 10 kept, those of the nearest band of rows to the top of the view (one team) or to its middle (two), the view cut in 3 bands; of these the farthest from the other groups, a row counting for two columns. A group goes 50 to 100 s after it is laid, drawn, two seconds later for each brick over three; dim for its last second, then `⟦·⟧`, `·`. 30 s with no brick broken: bricks, balls, capsules, falling bricks and twins go, the rackets come back about a place drawn in the view, off the text if they can, a new map is laid, a ball is served; the scores stay; three dim dots in the middle of the view. A brick written on is gone; scrolled out of the view, balls, capsules and falling bricks are dropped and the racket starts again in the view. Options, `-a` for all; all behind the text, `z` from the back: shards, bricks, the trail, capsules and falling bricks, rackets, balls, marks and particles, the score. `-r`: kinds of bricks, drawn by share: plain 66 %; hard 14 %, 3 hits, `⟦≡⟧` `⟦=⟧` `⟦-⟧`; loose 8 %, `⟨=⟩`: touched, it falls a row every other step, breaks on text or bricks, and kills the racket under it, unless under text, for 60 steps; bumper 5 %, `⟦o⟧`, 2 hits: the ball leaves at a random angle, faster for 24 steps; bomb 7 %, `⟦*⟧`: breaks the bricks within 3 columns and a row, bombs among them. `-b`: two broken bricks in five drop a capsule, a letter that sinks a row every third step and rests 2 s on text; drawn by share, the racket that touches it: `M` 60 %, each ball becomes two (5 at most), not dropped with 4 balls or more in play; `W` 10 %, 7 cells for 320 steps; `T` 10 %, a twin racket for 200, one a team, not made to last by another `T`; `F` 10 %, 5 s of fire: the ball goes through bricks; `N` 5 %, 3 cells for 240; `S` 5 %, balls faster for 160. No racket goes for `N` or `S`. `-m`: two rackets, two scores; one keeps the top, the other the bottom: the last ball out by a side sets its keeper's score to 0, and its keeper serves; a brick scores for the last racket hit; shots are worth their bricks, a ball out by the other's side 0.05: a tie-break; for a ball hit on purpose, the bricks a ball of the other team breaks alone count half. Face to face: 4 bounces of one ball, a team each in turn, within 2 s, the last two rackets 6 rows apart at most: one of the two, drawn, keeps away for 2 to 3 s: it waits 2 rows off its side, in the half of the columns the other is not in, hits no ball on purpose, takes no capsule, and still stops a ball about to leave by its side, as late as it can. `-e`: particles (`_arcade.spark`) for a brick, a bounce, a serve, a catch, a ball out; shards; a trail; `xN` for a combo; the best score. A blast, a kill and the flames of a ball on fire show without it. `--grab`, not in `-a`: a player. The four arrows are taken from the host keyboard (`input.grab`, escape ctrl+t), read among the events of the `Sight`; the racket of the player (alone: the only one; with `-m`: the bottom one, team 1) is not steered: `Game.press` of an arrow moves it for 2 steps, 3 cells a step along its row or a row a step, both at once if both are held; a key repeated by the terminal keeps it going, the opposite arrow stops it; it moves a cell at a time as far as it fits: off bricks, rackets and the two cells each side of a ball, inside the view, text no obstacle. Twins of its team and the other team play alone; of a duel, the racket that keeps away is never the player's; serve, kill and reset as for any racket. With a player a ball makes half the moves: its pace, the rally and the kick count in sixteenths of a move, in `_roll` and in the foresight alike; rackets, capsules, falling bricks and timers keep their pace. With a player and `-m`, the rackets that play alone (the other team, twins) move, and plan, at the pace of the player in place of 2 cells a step: the mean of the cells a step its racket moved over the last 4 s, idle steps counted, 0.5 at least, 3 at most; 2 until 4 s are played; a share of a cell is carried to the next step. The keys and the escape key are shown dim before the score. The escape key ends the app; the keys are released on leaving |
| `sand` | `idle` | native effect `sand` (draw extension `sand`), one request; one `screen.wait {idle}` on the frame name, answered by the driver when a pane is idle (`--now`: at once), then `_cli.follow`: when its frame is gone or inactive (panes with little text resized together), the sprite is moved to its heir, on the event's data; nothing asked. `--rate`; `--cursor`: the stream falls on the cursor's column |
| `cat` | `largest` | one sprite in frame cells, back layer (`--front`); driven by the topics `idle`, `cursor` (frame cells), `screen`, `frames`, subscribed with a snapshot, no tick: it waits on the socket until its next step. It stands on a ledge (3x11 free cells over 5 cells of text at least, or the floor of the view), walks, sits, grooms; sleeps on `idle` (a looped animation, nothing sent); jumps when the cursor is within 7 columns and 2 rows, when `covered` > 0.2 or the line under it goes; `visible` false after a scroll: back into the view after 2 to 4 s |
| `marks` | every pane, else `raw` | `PATTERN...`, `-e` regex, `-i`; one front `tint` per match in frame cells, a colour per pattern, on remembered rows too (`screen.find {known}`), 400 at most, nearest to the view first; looks again on `screen` and `frames` after 30 ms of silence (200 ms at most), finds in one batch, changes in another; topic `draw` drops what the driver removed; a display of the counts (in view, `/known`), `auto` top-right, front (`--no-hud`) |
| `monitor` | `raw` | widgets, one tag a monitor (`--monitor glance,connected,tcp-listen,disk-space`, all by default; titles `glance`, `connected`, `listening`, `disks`), 3 or 4 variants each, 18x1 to 56x13; `render(data, w, h)` gives `cells` of at most `h` rows of `w` cells, any size. Read every `--every` seconds (2), stdlib, `/proc` and `/var/run/utmp` (else `/run/systemd/sessions`): `glance` CPU and cores from two `/proc/stat` samples, memory, swap, load, uptime, network of non-virtual interfaces, the top 5 processes by CPU; `connected` the live USER_PROCESS records, grouped by user and origin (`tmux`, `local`, the host), tty idle from its atime, its foreground process; `tcp-listen` LISTEN sockets of `/proc/net/tcp{,6}` not on loopback (mapped included), the user, the process when its inode is in a readable `/proc/<pid>/fd`, both wildcards of one port and owner as `*`; `disk-space` block devices of `/proc/mounts` (no loop, no image), one a device, `statvfs`. Heights cut to the data (`trimmed`); when they change the tag is declared again (`Deck.resize`). `--merged`: one tag `monitor`, the n-th size the largest of the n-th sizes, one monitor a turn every `--cycle` seconds (8), the title following. `--print [WxH]`: no driver, each variant once in a plain box, ANSI on a tty |
| `rain` | `raw` | native effect `rain` (draw extension `rain`), one request, back layer; `--rate` drops per second over 80 columns; a drop lands on a free cell over text with probability 0.45, on the cursor and the floor always; a column whose least still cell (heat, within 3 columns) just changed gets 10 % of its rain, all of it after 10 s; a replaced frame is followed as `sand` does |
| `snow` | `largest` | native effect `snow` (draw extension `snow`), one request, back layer; `--rate` flakes per second over 80 columns (14), `--cover`: the snow lies, else it only falls, `--wind` columns per second (-1): every 6 to 15 s the app sends another wind, 0.2 to 2 times as strong, the other way one time in five (`Sprite.tune(wind=)`: tuned in place), as the `beats` of `_cli.follow` |

Draw extension `sand`: effect `sand`. Grains are braille dots, 2 x 4 a
cell, one step every 12 ms, a render every 48 ms.

- Walls: the cells under the canvas with ink, seen through
  `Effect::look`. A cell that gets text loses its grains; one that loses
  its text lets them fall. A resize keeps the grains, top-left fixed;
  those outside are lost.
- Scroll: grains are fixed on frame rows. When `Under.origin` moves the
  pile moves with the text; rows of cells that leave the canvas are put
  away, 1024 at most, the farthest forgotten first, and come back with
  their text. A floor that left lets its pile fall.
- Heat: a column costs 6 more as a spout for each of its hot cells.
- Spout: one stream, from the topmost free cell of a column: the nearest
  to the middle that lands on text, away from the ends of the run it
  lands on. Kept while it stays so; moved when the pile reaches it.
  `cursor`: the cursor's column, unless the cursor is outside the canvas
  or on column 0.
- A grain falls; held from below, awake, it slides down a diagonal or
  rolls two dots aside, and may stop on a one-dot drop; asleep, it
  topples only off a drop of three dots. A grain that moves wakes the
  three above it: avalanches.
- A heavy stream (more than 1.2 grains a step, 100 a second) has a
  reach `r`, the grains a step over 1.2, 8 at most: a held grain slides
  up to `r` dots a step, stops `r` times less often, gives way sideways
  through `r - 1` grains and wakes those it lands on. The slopes carry
  what the spout pours: at 400 a second, 4000 grains in 10 s, no column
  under the spout.
- Colour: the stream goes around `colors`, `color_ms` (20000) from one
  to the next, in 16 shades: strata. A cell has one colour: the mean
  of its grains.
- Params: `rate` (grains per second, 120; above 166 the stream widens),
  `cursor`, `colors`, `color_ms`, `walls` (rows standing for the app's
  until it is seen), `duration_ms`, `seed`.

Draw extension `snow`: effect `snow`, a render every 80 ms.

- Flakes: near (2.6 to 4.2 rows a second, `*` `+` `·` as they turn) or
  far (40 %, 1.2 to 2 rows a second, `·` at half opacity); a sway of 1.1
  columns; `wind`; out by a side, in by the other. Not drawn in the
  blank between two words.
- With `cover` only, snow lies. A ledge: a cell without ink, and none
  beside it, over ink, over the blank between two inked cells, over a
  full cell of snow, or the last of its column. A flake takes a ledge
  over text one time in two, any other always, and adds three eighths
  there, or to a neighbour ledge lower by more than three. A stack
  holds 3 cells (`▁` to `█` each) over text or the floor.
- The snow lies on frame rows: it scrolls with the text; rows within
  2000 of the view are kept. Snow written over is gone; snow that lost
  what held it falls again as flakes.
- Thaw: the top of a stack loses an eighth every 60 s on average.
  Melt: a full cell in 1.5 s where its cell or the one that holds it
  is hot, and within 3 columns of the cursor on its row and the row
  above.
- Params: `rate` (14), `wind` (-1), `cover` (false), `duration_ms`,
  `seed`; `tune`: `rate`, `wind`, `cover`, `duration_ms`.

`scripts/demo-apps.sh [snake] [--check]`: the apps on a scrolling vim
buffer. `scripts/demo-frames.sh [--check]`: a tmux session of two
windows under the driver, one drawing per pane, sand in the idle one.
`--check` runs detached and asserts what is drawn.

## Tests

- `scripts/e2e-frames.sh`: frames on real vim, tmux, less, man, more,
  htop, a shell and a stand-in agent TUI, in a detached tmux.
- `scripts/e2e-tuis.sh`: htop, less, man, tmux, ranger, glances,
  opencode, and `nested` (htop and ranger in the panes of a tmux), in a
  detached tmux: the screen read against the host, the
  layout found and its steadiness, a sprite drawn and removed, the exit.
  `KNOWN` marks a defect recorded in `plan/ideas.md`.
- `scripts/e2e-render.sh`: the driver in a detached tmux; what tmux shows
  is compared, text and styles, with the driver's model of the host.
- `scripts/e2e-heat.sh`: heat classes and idle on a shell, vim and htop.
- `scripts/e2e-tmux-attach.sh`: `ptyrc tmux attach` and `detach` on two
  private tmux servers, one for the terminal: the swap and what the
  terminal shows, the session found from a pane, apps, `prefix d`, the
  verbs typed in a pane, the driver that does not start, the terminal
  outside tmux.
- `scripts/bench.sh`: throughput and key-echo latency, bare, transparent,
  engine, engine without presets, engine with sprites. Then one
  `Frames::update` at 200x60 (scan, heat, update, tick);
  `BENCH_ONLY=frames` for it alone.

- Rust unit tests per crate, `cargo test --workspace`. Widgets
  (`ptyrc-core/src/widgets/tests.rs`): rooms against brute force,
  fill and centring, ranking, `prefer`, gap, cursor row, panel and
  changed cells, pressed and evicted, grown, two owners, declarations
  replaced, borders and titles, modals (over at each `prefer`, the
  blackout, kept under text, a resize, an obstacle), the chase,
  hidden owners.
- `ptyrc-vt`: the repair property above; recorded streams of real programs.
- `ptyrc-server`: headless end-to-end, a `Vt` fed with the host log stands
  for the host.
- `ptyrc-cli/tests/errors.rs`: every error case of the CLI, message, hint
  and exit status.
- `ptyrc-cli/src/manage/`: the menu by its keys and its rows, at the
  four sizes (`menu/tests.rs`; `screens`, `screens_of_frames`, ignored,
  print them); the runner on a fake table (`ui/tests.rs`).
  `tests/overlay.rs`: ctrl+t twice on a host pty, a start by the form,
  pause, resume, stop, the ways out. `scripts/e2e-manage.sh [--show]`:
  the same under `ptyrc tmux attach` on two private tmux servers, with
  the frames, a live move and the real `ptyrc-app`.
- `ptyrc-cli/tests/tapes.rs`: tapes of real programs replayed against
  their snapshots; `tests/tools.rs`: grab, click, host mouse, shot, attach on a pty;
  `tests/hotkey.rs`: the pair on a pty; `tests/manage.rs`: the verbs of
  `ptyrc manage` on a headless session, a stand-in app in Python.
- Python: `uv run pytest`, against a headless driver, session time 5
  times real time (`PTYRC_CLOCK_RATE=1` to run at real time); a test
  marked `realtime` (apps) keeps rate 1. A test that times what the
  driver or an app does measures with `ptyrc.clock`.
- `scripts/e2e-tmux.sh`: the driver inside a detached tmux, `tmux
  capture-pane` is the host's verdict.
