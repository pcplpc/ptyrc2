// SPDX-License-Identifier: GPL-3.0-or-later
//
// The script API of ptyrc store apps, version 1. The whole of it.
//
// An app is `app.js` and the files it imports by relative path. It
// exports functions the host calls, one at a time:
//
//   import { Canvas, period } from "ptyrc";
//   let canvas;
//   export function init(args) { canvas = new Canvas(20, 3); period(50); }
//   export function tick(now) {
//     canvas.clear().text(0, 0, `t = ${now} ms`, { fg: "#ffaa00" });
//     canvas.present({ anchor: { at: [2, 1], space: "view" } });
//   }
//   export function event(topic, data) {}
//   export function stop() {}
//
// - `init(args)`: once, first. `args`: the arguments and options, parsed.
//   2 s at most.
// - `tick(now)`: at the period asked. `now`: session time in ms since
//   the app started. 50 ms at most; three late in a row: paused.
// - `event(topic, data)`: an event of a topic subscribed with
//   `call("subscribe", { topics: [...] })`.
// - `stop()`: optional; the app is told to end. 500 ms.
// At least one of `init`, `tick`, `event` is exported. They may be
// `async`; nothing awaited outlives the call.
//
// Widgets the driver places and sizes: the module "ptyrc/deck", below.
// Braille dots, half blocks, colours: "ptyrc/canvas". The shape of the
// view kept by the topics: "ptyrc/sight".
// Unit tests: `tests.js`, its exports `test_*` (`ptyrc dev test --unit`),
// `assert`, `assertEqual`, `assertThrows`.
//
// Bytes: `Uint8Array.fromBase64(s)`, `bytes.toBase64()`, `atob`,
// `btoa`; `TextEncoder`, `TextDecoder` (UTF-8 only) are global.
//
// There is no timer, no network, no path, no console, no `require`:
// `tick` is the only clock. `Date.now()` is session time (0 at the
// start); the wall clock is `now()` (permission `clock`). A file is one
// the user named (`file(handle)`); state that outlives a run goes to
// `storage` (permission `storage`); the app's own files are `asset`s.
// `Math.random()` draws from the host. Modules: "ptyrc", "ptyrc/deck",
// "ptyrc/canvas", "ptyrc/sight", and files of the app: "./lib/util.js".
//
// An error left uncaught is logged with its file and line. A
// `PtyrcError` (a request refused) left uncaught in `tick` or `event`
// does not end the app; any other error does.

declare module "ptyrc" {
  /**
   * A request of the ptyrc protocol, answered before it returns:
   * `call("subscribe", { topics: ["size"] })`. The methods, topics and
   * the permission each needs: `PROTOCOL.md`. At most 1000 a second.
   * @throws PtyrcError when the request is refused.
   */
  export function call(method: string, params?: object): any;

  /** A request refused. */
  export class PtyrcError extends Error {
    /**
     * The code of the protocol: `denied` (a permission the app does
     * not hold: go on without), `limit` (too many calls a second),
     * `bad-params`, `no-sprite`, `not-usable`, `unknown-method`,
     * `headless` (a method a test run does not answer), ...
     * `PROTOCOL.md`, Errors.
     */
    code: string;
    /** What would cure it, when the host knows. */
    hint?: string;
  }

  /**
   * A colour: `"#rrggbb"`; `rgb(r, g, b)`; `0..255`, the palette of
   * the terminal; `"default"`, the terminal's own; `null`, that of the
   * cell below.
   */
  export type Color = string | number | null;

  /** A colour from three bytes. Faster than a string in a loop. */
  export function rgb(r: number, g: number, b: number): Color;

  export interface StyleSpec {
    fg?: Color;
    bg?: Color;
    bold?: boolean;
    dim?: boolean;
    italic?: boolean;
    underline?: boolean;
    inverse?: boolean;
    strike?: boolean;
    blink?: boolean;
  }

  /** A style checked once. Make it outside the loop, use it inside. */
  export class Style {
    constructor(spec?: StyleSpec);
  }

  /** `new Style(spec)`. */
  export function style(spec?: StyleSpec): Style;

  /** Where and how a canvas shows: the members of a sprite. */
  export interface Place {
    /**
     * `{ at: [x, y], space: "view" }`: fixed on the surface, negative
     * from the right and the bottom; `space: "frame"`: moves with the
     * scroll. `{ auto: { prefer: "top-right" } }`: a quiet corner.
     * `{ cursor: { dx: 1, dy: 0 } }`: follows the cursor.
     * `{ text: "word", side: "right" }`: next to a match.
     * Default: `{ at: [0, 0], space: "view" }`.
     */
    anchor?: object;
    /** Order among the canvases of the app. */
    z?: number;
    /** Opacity of the whole canvas, 0 to 1. */
    alpha?: number;
    /** Removed after that long. */
    ttl_ms?: number;
    /** Other members of a sprite: `layer`, `motion`, ... (`PROTOCOL.md`). */
    [member: string]: unknown;
  }

  /**
   * A grid of cells, `x` right, `y` down, from 0. Nothing shows until
   * `present`. A cell never set is a hole: what is below shows. A
   * space is a hole unless it has a `bg`. All canvases of an app hold
   * four screens of cells at most.
   */
  export class Canvas {
    /** @throws RangeError when refused: no cell, or too many in all. */
    constructor(width: number, height: number);
    readonly width: number;
    readonly height: number;
    /**
     * One cell. `ch`: a string (its first character) or a scalar
     * value; `""` or 0 empties the cell. A wide glyph takes the next
     * cell too. Outside the canvas: nothing.
     */
    set(x: number, y: number, ch: string | number, style?: Style | StyleSpec): this;
    /**
     * A line of text from `(x, y)`, clipped; no wrap, no newline.
     * Returns the `x` after its last glyph.
     */
    text(x: number, y: number, text: string, style?: Style | StyleSpec): number;
    /** A rectangle of one glyph. */
    fill(x: number, y: number, width: number, height: number, ch: string | number, style?: Style | StyleSpec): this;
    /** Empties every cell. */
    clear(): this;
    /**
     * The cells as rows of runs, the `cells` form of a content
     * (`widget.set`); a hole is a blank. A deck takes the canvas itself.
     */
    runs(): Run[][];
    /**
     * Shows the canvas as it is now, at `place`; again after changes.
     * Only what changed is sent. `false`: refused (the log says why).
     */
    present(place?: Place): boolean;
    /** Removes the picture and gives the cells back. */
    release(): void;
  }

  /**
   * The `tick` wanted, every `ms`: 33 at least; 0: none (the default).
   */
  export function period(ms: number): void;

  /** A line of the app's log (`ptyrc app log`). 1 MiB in all. */
  export function log(...values: unknown[]): void;

  /** Random bytes from the host: 65536 at most. */
  export function random(length: number): Uint8Array;

  /** What the app may know of its manifest, as the runner holds it. */
  export interface Manifest {
    id: string;
    version: string;
    /** As declared; `words`: those of the permission `words`. */
    permissions: { name: string; words?: string[] }[];
    /** The permissions held now. */
    granted: string[];
    /** `[limits]` granted, each up to its ceiling: `{"tick-ms": 80}`. */
    limits: { [key: string]: number };
  }

  /**
   * The manifest of the app (a copy); `null` in a run given none
   * (`ptyrc-box` without `--manifest`).
   */
  export function manifest(): Manifest | null;

  /** Ends the app. Does not return. */
  export function exit(code?: number): never;

  /**
   * The wall clock (`clock.now`). Without `clock` held: session time as
   * the epoch, UTC, `degree: "none"`. `Date.now()` stays session time.
   */
  export function now(): { unixMs: number; zone: string; offsetS: number; abbr: string; next: number | null; degree?: "none" };
  /**
   * The offsets of `zone` (an IANA name, or `"local"`) at each instant
   * of `at` (ms, 1 to 64; absent: now) (`clock.zone`).
   * @throws PtyrcError `bad-params`: no such zone.
   */
  export function zone(
    zone: string,
    at?: number | number[],
  ): { zone: string; offsets: { at: number; offsetS: number; abbr: string; next: number | null }[]; degree?: "none" };
  /**
   * The `period` to the next multiple of `everyMs` of the wall clock
   * after `unixMs` (`now().unixMs`), plus `everyMs` while under 33 (a
   * `period` under 33 is 33). At each tick: `period(until(now().unixMs,
   * 1000))` ticks on the second. Pure.
   */
  export function until(unixMs: number, everyMs: number): number;
  /** The date and time of `unixMs` at `offsetS` east of UTC; `weekday` 0: Sunday. */
  export function civil(
    unixMs: number,
    offsetS?: number,
  ): { year: number; month: number; day: number; hour: number; minute: number; second: number; weekday: number };

  /**
   * The app's named values (`storage.*`): strings, 256 KiB each, 64
   * keys, 1 MiB in all. A key: `[A-Za-z0-9._-]`, 1 to 64 bytes.
   * @throws PtyrcError `denied` without `storage`; `not-usable` when full.
   */
  export const storage: {
    get(key: string): string | null;
    /** The bytes used after it. */
    set(key: string, value: string): number;
    remove(key: string): void;
    keys(): string[];
  };

  /** An asset of `[app] assets` as text (UTF-8, invalid bytes replaced). @throws PtyrcError `no-file` */
  export function asset(name: string): string;
  /** An asset's bytes. @throws PtyrcError `no-file` */
  export function assetBytes(name: string): Uint8Array;
  /** The names of the assets, relative to `[app] assets`: `ship.pic`, `sub/a.pic`. */
  export function assets(): string[];

  /**
   * A file or directory the user named at the start (`file.*`), by its
   * handle: `"f0"`, or the value `init(args)` gets for a file option,
   * `{file: "f0", name: "TODO.md"}` (a list: the first).
   */
  export function file(handle: string | { file: string; name?: string } | { file: string; name?: string }[]): File;
  export interface File {
    readonly handle: string;
    /** The last component, as typed. */
    readonly name: string;
    /** UTF-8, invalid bytes replaced. @throws PtyrcError `no-file` */
    text(): string;
    bytes(): Uint8Array;
    /**
     * `text()` and the mtime it was read at: the `ifMtime` of a `write`
     * that must not clobber a change made since. @throws PtyrcError
     * `not-usable`: changed between two pieces of 1 MiB.
     */
    textWithMtime(): { text: string; mtimeMs: number };
    /** `bytes()` and the mtime they were read at. */
    bytesWithMtime(): { bytes: Uint8Array; mtimeMs: number };
    /** A file absent: `exists: false`. */
    stat(): { exists: boolean; size: number; mtimeMs: number };
    /**
     * The whole content, 1 MiB at most. Needs `write = true` and
     * `files-write`, else `denied`; `ifMtime`: `not-usable` when the file
     * changed since.
     */
    write(content: string | Uint8Array, options?: { ifMtime?: number }): { size: number; mtimeMs: number };
    /** The regular files of a directory, by name. */
    list(): string[];
    /** The file `name` of this directory. */
    child(name: string): File;
  }

  /**
   * What `ptyrc app ID --wait` prints, and its exit status (0..255)
   * (`app.result`). The last one holds; then `exit`.
   */
  export function result(value: unknown, status?: number): void;

  /** A run of a row: text in one style (`PROTOCOL.md`, `cells`). */
  export interface Run {
    t: string;
    fg?: Color;
    bg?: Color;
    bold?: boolean;
    dim?: boolean;
    italic?: boolean;
    underline?: boolean;
    inverse?: boolean;
    strike?: boolean;
    blink?: boolean;
  }

  /** Columns of `text` on a terminal: a wide glyph 2, a mark 0. */
  export function cells(text: string): number;

  /** What `assert*` throw. */
  export class AssertionError extends Error {}
  /** @throws AssertionError when `value` is falsy. */
  export function assert(value: unknown, message?: string): void;
  /**
   * Deep equality: arrays, plain objects, typed arrays, `Map`, `Set`
   * by their contents; numbers by `Object.is`.
   * @throws AssertionError naming both.
   */
  export function assertEqual(actual: unknown, expected: unknown, message?: string): void;
  /**
   * `f()` throws, an instance of `kind` when given. Returns the error.
   * @throws AssertionError when it does not.
   */
  export function assertThrows(f: () => unknown, kind?: Function, message?: string): Error;
}

/**
 * Widgets: small windows the driver places where there is room, in one
 * of the sizes the app gives (its variants), and moves or resizes when
 * the room changes. A deck keeps them drawn.
 *
 *   import { period } from "ptyrc";
 *   import { Deck } from "ptyrc/deck";
 *   let deck;
 *   export function init() {
 *     deck = new Deck({ border: "round", prefer: "top-right" });
 *     deck.add("clock", { long: [8, 1], short: [5, 1] },
 *       (w, h) => [w >= 8 ? `t=${deck.now} ms` : `${deck.now / 1000 | 0}s`]);
 *     period(1000);
 *   }
 *   export function tick(now) { deck.tick(now); }
 *   export function event(topic, data) { deck.event(topic, data); }
 *
 * Give the deck every `tick` and every `event`: it follows the topics
 * `widgets` (the variant shown), `move`, `hover`, `mouse`, `input`.
 * Under `--unit`, `new Deck({call})` with a host of the test's own.
 */
declare module "ptyrc/deck" {
  import { Canvas, Run, StyleSpec } from "ptyrc";

  /** A variant's content: lines (a string, `\n` between), rows of strings or runs, or a canvas. */
  export type Content = string | (string | Run[])[] | Canvas;

  /** Inner sizes `[w, h]`, 1x1 to 200x60: named `WxH`, or by name. At most 16. */
  export type Sizes = [number, number][] | { [name: string]: [number, number] };

  /** Members of `widget.show` (`PROTOCOL.md`). */
  export interface Show {
    /** A preset (`round`, `heavy`, `double`, `ascii`, `none`, `painting`, ...) or its object. */
    border?: string | object;
    /** On the bottom edge; the tag when absent; `""`: none. */
    title?: string;
    /** A frame id or name. */
    frame?: number | string;
    /** `top-left`, `top`, `top-right`, `left`, `center`, `right`, `bottom-left`, `bottom`, `bottom-right`, `any`. */
    prefer?: string;
    /** About that much of the room, 0.1 to 1 (0.5). */
    fill?: number;
    /** Cells kept from the edges, 0 to 4 (1). */
    margin?: number;
    /** `back` (default); `front` needs `front-widget`. */
    layer?: number | string;
    /** Always shown, in front; needs `keys-grab`. */
    modal?: boolean;
  }

  export interface DeckOptions extends Show {
    /** `tick` renders every variant, not only the one shown (the driver keeps each). */
    every?: boolean;
    /** Session ms without a move after which a hover ends: 1500; null: never. */
    hoverTimeout?: number | null;
    /** The request function: `call` of "ptyrc"; a fake one makes a deck testable under `--unit`. */
    call?: (method: string, params?: object) => any;
  }

  /** What a handler returns: `false` when nothing changed (not drawn again). */
  type Changed = boolean | void;

  export interface AddOptions extends Show {
    /**
     * A key of the host keyboard while the user focused the widget
     * (ctrl+t twice): `a`, `space`, `enter`, `backspace`, `up`,
     * `shift+up`, `ctrl+a`, ... The widget asks for the keyboard
     * (quietly) when `key` or `paste` is given. Escape (`esc`) and
     * close (`ctrl+x`) never come: the driver takes them.
     */
    key?(name: string, widget: Widget): Changed;
    /** A bracketed paste, its lines parted by `\n`. */
    paste?(text: string, widget: Widget): Changed;
    /** The pointer on the cell `(x, y)` of the inner box (the border is not in it). */
    hover?(x: number, y: number, widget: Widget): Changed;
    /**
     * A button pressed, or a wheel turned, on the cell `(x, y)` of the
     * inner box: `button` `left`, `middle`, `right`, `wheel-up`,
     * `wheel-down`, `wheel-left`, `wheel-right`. Releases and drags are
     * not told. Subscribes `mouse`.
     */
    click?(x: number, y: number, button: string, widget: Widget): Changed;
    /** The pointer left, the widget moved, or `hoverTimeout` passed. */
    leave?(widget: Widget): Changed;
    /** Closed by the user (ctrl+x): out of the deck. */
    closed?(widget: Widget): void;
    /** `widget.ask`: `quiet` (true: waits for no answer), `escape`, `close` (key names, or false: none). */
    ask?: { quiet?: boolean; escape?: string | false; close?: string | false };
  }

  export class Deck {
    /** Defaults for every widget's `widget.show`. */
    constructor(options?: DeckOptions);
    readonly widgets: Widget[];
    /** Session time of the last `tick`. */
    readonly now: number;
    hoverTimeout: number | null;
    /** The request function of the deck and its widgets. */
    readonly call: (method: string, params?: object) => any;
    /**
     * Declares the tag, each variant rendered once, and shows it.
     * `render(w, h, widget)`: the content of the variant of that size;
     * `widget.focused` tells the focus.
     * @throws PtyrcError when refused (16 widgets, sizes out of bounds, ...).
     */
    add(tag: string, sizes: Sizes, render: (w: number, h: number, widget: Widget) => Content, options?: AddOptions): Widget;
    /**
     * At each `tick`: renders the variant shown of every widget (each
     * variant with `every`; each until the driver told which shows) and
     * sends what changed in one `batch`. Ends a hover past its timeout.
     */
    tick(now: number): void;
    /** Takes `widgets`, `move`, `hover`, `mouse`, `input`; false: not the deck's. */
    event(topic: string, data: any): boolean;
    /** Renders now: `widget` alone, or all; `every`: each variant. */
    draw(widget?: Widget, every?: boolean): void;
    /** Other sizes: declared again, laid out again. */
    resize(widget: Widget, sizes: Sizes): void;
    /** Every widget shown again on `frame` (the topic `move` does it). */
    move(frame: number | string): void;
    /** Out of the deck; the driver is not told (`widget.hide` first). */
    drop(widget: Widget): void;
    /** Ends the hover under way. */
    leave(): void;
  }

  /** A widget of a deck: `info` is what the driver last told. */
  export class Widget {
    readonly tag: string;
    /** `{id, tag, state, variant, size, rect, sprite, title, variants, focused, asks, ...}`: `PROTOCOL.md`. */
    info: any;
    readonly id: number | null;
    /** The variant shown; null while hidden or not told yet. */
    readonly variant: string | null;
    /** Inner size of the variant shown. */
    readonly size: [number, number] | null;
    /** Outer rectangle, screen cells. */
    readonly rect: [number, number, number, number] | null;
    readonly placed: boolean;
    /** A modal over the text, for want of room. */
    readonly over: boolean;
    /** `loud`, `quiet`, or null: it does not ask the keyboard. */
    readonly asks: string | null;
    /** The keys of the host keyboard are its own. */
    readonly focused: boolean;
    /** Hidden by `hide()`. */
    readonly hidden: boolean;
    /** New content of a variant (the one shown when not named), and, or, a title. */
    set(content: Content | null, options?: { variant?: string; title?: string }): void;
    hide(): void;
    /** Asks for the keyboard; the user gives it. */
    ask(options?: { quiet?: boolean; escape?: string | false; close?: string | false }): void;
    release(): void;
  }

  /** The `input` events of a focused widget as keys and pastes; one a widget. */
  export class Keys {
    /** `{key}`, `{paste}`, or null: nothing yet, or no name. */
    feed(data: { key?: string; bytes?: string; paste?: string }): { key: string } | { paste: string } | null;
    /** A paste under way dropped: the focus went. */
    reset(): void;
    /** `shift+up`, `ctrl+alt+end`, ... of a modified arrow, home, end, insert, delete, page key; else null. */
    static name(bytes: Uint8Array | string): string | null;
  }

  /** A line of text and its cursor (code units). */
  export class Field {
    constructor(text?: string);
    text: string;
    cursor: number;
    /** Typed at the cursor; a control character is a blank. */
    insert(text: string): void;
    /**
     * `left`, `right`, `home`, `end`, `backspace`, `delete`, `ctrl+a`,
     * `ctrl+e`, `ctrl+b`, `ctrl+f`, `ctrl+d`, `ctrl+h`, `ctrl+u`,
     * `ctrl+k`, `ctrl+w`, `space`; a character is typed. False: not one.
     */
    key(name: string): boolean;
    /**
     * `w` cells. Focused: the cursor an inverse cell, scrolled to it; else
     * the start, or `placeholder`, dim. A text report of `dev test` shows
     * the inverse cell only with `--styles`.
     */
    render(w: number, focused: boolean, placeholder?: string, style?: StyleSpec): Run[];
  }

  /** A bar on a list of `count` rows, of which a window shows. */
  export class Listing {
    constructor(count?: number, sel?: number);
    count: number;
    sel: number;
    top: number;
    /** `up`, `down`, `tab`, `home`, `end`, `pageup`, `pagedown`; `j`, `k` with `letters`. */
    key(name: string, letters?: boolean): boolean;
    /** The indices shown in `h` rows; the bar among them. */
    window(h: number): number[];
    /** `3/12` when the list does not fit `h` rows; else `""`. */
    mark(h: number): string;
  }

  /** `text` in `n` cells at most, cut with `…`. */
  export function cut(text: string, n: number): string;
  /** A row in exactly `w` cells: cut, padded with blanks. */
  export function fit(row: string | Run[], w: number): Run[];
  /** `text` on `n` lines of `w` cells, cut at blanks; the rest ends the last line with `…`. */
  export function wrap(text: string, w: number, n: number): string[];
}

/**
 * Pictures finer than a cell, pure: no call. A colour here is the
 * number of `rgb(r, g, b)`; `"#rrggbb"` is taken too. A dot, a pixel or
 * a cell outside is ignored; read outside: `false`, `null`.
 *
 *   import { Canvas, rgb } from "ptyrc";
 *   import { Braille, Half } from "ptyrc/canvas";
 *   const dots = new Braille(20, 5);          // 40 by 20 dots
 *   dots.line(0, 0, 39, 19);
 *   deck.add("plot", [[20, 5]], () => dots.rows());
 *   dots.draw(canvas, 0, 0, { fg: 6 });       // or on a Canvas
 *
 * Cost (`sdk/js/README.md`, Speed): `set` about 0.45 µs; at 200x60,
 * 96000 dots set one by one take about 45 ms, near the 50 ms of a tick.
 * `rows` and `draw` of 200x60: about 2 ms. A picture that sets every
 * dot of the frame each tick: write `bits`, or go to Rust.
 */
declare module "ptyrc/canvas" {
  import { Canvas, Run, Style, StyleSpec } from "ptyrc";

  /** A colour of this module: `rgb(r, g, b)`, or `"#rrggbb"` as input. */
  export type RGB = number | string;

  /** `DOTS[y][x]`: the bit of the dot `(x, y)` of a cell; the glyph is U+2800 plus the bits. */
  export const DOTS: readonly (readonly [number, number])[];

  /** `2 * width` by `4 * height` dots, on or off, in `width` by `height` cells. */
  export class Braille {
    /** All off. @throws RangeError unless two integers, 1 at least. */
    constructor(width: number, height: number);
    readonly width: number;
    readonly height: number;
    /** The dots of the cell `(cx, cy)` at `cy * width + cx`, `DOTS` bits. */
    readonly bits: Uint8Array;
    /** The dot `(x, y)` on, or off with `false`. */
    set(x: number, y: number, on?: boolean): void;
    get(x: number, y: number): boolean;
    /**
     * Every dot from `(x0, y0)` to `(x1, y1)`: `max(1, round(max(|dx|,
     * |dy|)))` steps, each rounded to a dot.
     */
    line(x0: number, y0: number, x1: number, y1: number, on?: boolean): void;
    /** Every dot off. */
    clear(): this;
    /** The glyph of the cell; `""` when no dot of it is on. */
    glyph(cx: number, cy: number): string;
    /** The cells `[cx, cy]` with a dot on, row by row. */
    cells(): [number, number][];
    /** A string a row of cells, a blank where no dot is on: a widget's content, `draw.*` rows. */
    rows(): string[];
    /** Each row on `canvas` from `(x, y)`, as `canvas.text`: a blank is a hole unless `style` has a `bg`. */
    draw(canvas: Canvas, x?: number, y?: number, style?: Style | StyleSpec): this;
  }

  /** Two pixels are alike when their channels differ by this much at most, summed: 24. */
  export const NEAR: number;

  /**
   * `width` by `2 * height` pixels in `width` by `height` cells: a cell
   * is `▀`, the top pixel its foreground, the bottom its background.
   * Glyphs go over the pixels.
   */
  export class Half {
    /** Every pixel `color` (black). @throws RangeError unless two integers, 1 at least. */
    constructor(width: number, height: number, color?: RGB);
    readonly width: number;
    readonly height: number;
    /** Every pixel; the glyphs stay. */
    fill(color: RGB): this;
    get(x: number, y: number): number | null;
    /** A pixel; `alpha` under 1: mixed over the one there. */
    set(x: number, y: number, color: RGB, alpha?: number): void;
    rect(x: number, y: number, w: number, h: number, color: RGB, alpha?: number): void;
    /** The pixels whose centre `(x + .5, y + .5)` is within `r` of `(cx, cy)`. */
    disc(cx: number, cy: number, r: number, color: RGB, alpha?: number): void;
    /** Glyphs from the cell `(cx, cy)`, one a code point, cut at the edge; `bg` null: the mean of the two pixels. */
    put(cx: number, cy: number, text: string, fg?: RGB | null, bg?: RGB | null): void;
    /**
     * The cells of `braille` with a dot on, where no glyph is and the two
     * pixels are alike (`NEAR`): the glyph over the pixel under most of
     * its dots, `fg` mixed over that by `tint`. Where they differ, an
     * edge: no glyph; `fg` mixed by `rough` into each pixel under a dot.
     */
    ink(braille: Braille, fg?: RGB | null, tint?: number, rough?: number): void;
    /**
     * Rows of runs `{t, fg?, bg}`, colours of the 256 (`xterm`); `deep`:
     * `"#rrggbb"`. Cells alike are one run; a blank joins the run before
     * it. Ink over two pixels the 256 tell apart is left out.
     */
    rows(deep?: boolean): Run[][];
    /** The cells on `canvas` from `(x, y)`. */
    draw(canvas: Canvas, x?: number, y?: number, deep?: boolean): this;
  }

  /** From `a` (`t` 0) to `b` (`t` 1), `t` clamped, each channel rounded. */
  export function mix(a: RGB, b: RGB, t: number): number;
  export function channels(c: RGB): [number, number, number];
  /** `"#rrggbb"`. */
  export function hex(c: RGB): string;
  /** The nearest of the 240 fixed colours of the 256 (16..255); a grey only for a grey. */
  export function xterm(c: RGB): number;
  /** The colour `n` of the 256, 16..255, as `rgb`. @throws RangeError */
  export function palette(n: number): number;
  /**
   * The palette number, 0..15, of `black`, `red`, `green`, `yellow`,
   * `blue`, `magenta`, `cyan`, `white`, or `bright-` one of them: the 16
   * the terminal sets. @throws RangeError
   */
  export function named(name: string): number;
}

/**
 * The shape of the view of a frame, kept current by the topics: the
 * view is read again only when an event says it changed.
 *
 *   import { Sight } from "ptyrc/sight";
 *   let sight;
 *   export function init() { sight = new Sight({ cursor: true }); period(50); }
 *   export function tick(now) {
 *     if (!sight.look()) return;          // off screen
 *     if (sight.changed) walls = ...sight.rows;
 *   }
 *   export function event(topic, data) { sight.event(topic, data); }
 *
 * What it holds at each degree (`PROTOCOL.md`, cut answers): `layout`
 * refused, the raw frame and no `frames` event; `shape` refused, blank
 * rows of the size, `cursor` null, `degree` `"none"`; `words` alone, the
 * held words among `words`; `text: true` without `text`, the shape.
 */
declare module "ptyrc/sight" {
  export interface SightOptions {
    /** The frame read: an id or a name (`"raw"`). A `move` event changes it. */
    frame?: number | string;
    /** Words shown as on screen in the shape (`screen.shape {words}`). */
    words?: string[];
    /** The text (`screen.text`), not the shape. */
    text?: boolean;
    /** Subscribe `cursor` too. */
    cursor?: boolean;
    /** The request function: `call` of "ptyrc"; a fake one in unit tests. */
    call?: (method: string, params?: object) => any;
  }

  export class Sight {
    /** Subscribes `frames`, `text`, `size`, `move` (and `cursor`); reads `frame.get`. */
    constructor(options?: SightOptions);
    /** The frame read. */
    frame: number | string;
    /** Its info as last told (`frame.get`, `frames`). */
    readonly info: any;
    /** The view, a string a row, padded to the width: `#` ink, a blank, a word of `words` as shown; or the text. */
    readonly rows: string[];
    /** The size of the view. */
    readonly size: [number, number];
    /** The frame on the screen, `[x, y, w, h]`. */
    readonly rect: [number, number, number, number] | null;
    /** In view cells; null: hidden, not told, or `shape` not held. */
    readonly cursor: [number, number] | null;
    /** What the last read was cut to: `"none"`, `"shape"`, `"words"`; null: whole. */
    readonly degree: string | null;
    /** The last `look` read the view. */
    readonly changed: boolean;
    /** The frame is on screen. */
    readonly usable: boolean;
    /**
     * Takes `frames`, `text`, `cursor`: true. Reads `size` and `move`
     * (the frame named is read from then on) and returns false: the
     * app or its deck wants them too. Any other: false.
     */
    event(topic: string, data: any): boolean;
    /** At the tick: the view read again when told; false while the frame is off screen. */
    look(): boolean;
    /** Read again at the next `look`. */
    touch(): void;
    /** `rows[y][x]` is not a blank; false outside. */
    ink(x: number, y: number): boolean;
  }
}
