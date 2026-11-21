# The deck of widgets, script SDK (`sdk/js`, module `"ptyrc/deck"`)

Settled 2026-11-20 (agent `sdk-js`). The Rust SDK mirrors the names.
Typed in `sdk/js/ptyrc.d.ts`; the model app: `sdk/js/examples/clockface`.

## Names

| name | what |
|---|---|
| `new Deck({border, title, frame, prefer, fill, margin, layer, modal, every, hoverTimeout, call})` | defaults of `widget.show` for every widget; `every`: `tick` renders each variant, not only the one shown; `hoverTimeout` ms (1500; null: never); `call` the request function (a fake one under `--unit`) [as built 2026-11-20] |
| `deck.add(tag, sizes, render, {key, paste, hover, leave, closed, ask, ...show}) -> Widget` | declare (each variant rendered once) and show; `sizes`: `[[w, h], ...]` (named `WxH`) or `{name: [w, h]}`; first call subscribes `widgets`, `move`; `hover`/`leave`: subscribes `hover`; `key`/`paste`: `widget.ask {quiet: true}` (`ask: {quiet, escape, close}` overrides) |
| `render(w, h, widget)` | the content of a variant: a string (lines by `\n`), rows (strings, or arrays of runs `{t, fg, bg, bold, ...}`), or a `Canvas` |
| `deck.tick(now)` | the periodic draw: `deck.now = now`; a hover past its timeout ends; every widget shown rendered, the changed contents sent in ONE `batch` (one by one where `batch` answers `headless`) |
| `deck.event(topic, data) -> boolean` | `widgets` (info; a widget whose variant, focus or `asks` changed is drawn at once), `move` (`{frame}`: every tag shown again there), `hover`, `input` (keys of its focused widget); false: not the deck's |
| `deck.draw(widget?, every?)` | render now, one batch |
| `deck.resize(widget, sizes)`, `deck.move(frame)`, `deck.drop(widget)`, `deck.leave()` | other sizes; another frame; out of the deck (`closed(widget)` called); end a hover |
| `deck.widgets`, `deck.now` | the widgets; the session time of the last `tick` |
| handlers | `key(name, widget)`, `paste(text, widget)`, `hover(x, y, widget)` (cell of the inner box), `click(x, y, button, widget)` (a `press` or `wheel` on the inner box; subscribes `mouse`) [as built 2026-11-20], `leave(widget)`, `closed(widget)`; returning `false`: nothing changed, not drawn again |
| `Widget` | `tag`, `info`, `id`, `variant`, `size`, `rect`, `placed`, `over`, `asks`, `focused`, `hidden`; `set(content, {variant, title})`, `hide()`, `ask({quiet, escape, close})`, `release()` |
| `Keys` | `feed(data)` of an `input` event: `{key}`, `{paste}` or null; `Keys.name(bytes)`: `shift+up`, ... for modified arrows and tilde keys; `reset()` |
| `Field` | a line and its cursor (`ptyrc.ui.Editor`): `text`, `cursor`, `insert(text)`, `key(name) -> boolean`, `render(w, focused, placeholder, style) -> runs` |
| `Listing` | a bar on a list (`ptyrc.ui.Listing`): `count`, `sel`, `top`, `key(name, letters) -> boolean`, `window(h) -> indices`, `mark(h)` |
| `cut(text, n)`, `fit(row, w)`, `wrap(text, w, n)` | cells: cut with `…`; a row of runs in exactly `w` cells; lines cut at blanks |

In `"ptyrc"`: `cells(text)` (columns), `Canvas.runs()` (rows of runs: the
`cells` form of a content; a hole is a blank).

## Example

```js
import { period } from "ptyrc";
import { Deck } from "ptyrc/deck";

let deck, paused = false;
export function init() {
  deck = new Deck({ border: "round", prefer: "top-right" });
  deck.add("clock", { long: [8, 1], short: [5, 1] },
    (w) => [(w >= 8 ? "00:00:" : "") + String(Math.floor(deck.now / 1000) % 60).padStart(2, "0")],
    { key: (name) => (name === "space" ? void (paused = !paused) : false) });
  period(1000);
}
export function tick(now) { if (!paused) deck.tick(now); }
export function event(topic, data) { deck.event(topic, data); }
```

## The driver

- `widget.set {content: {grid}}`: taken as the cells it spells since
  6e84193 [as built 2026-11-20]; a `Canvas` goes as `cells` (runs).
- Headless: no `widgets` event after a widget is placed, `batch` and
  `widget.ask` answer `headless`. Until a `widgets` event names the
  variant shown, `tick` renders each variant.
