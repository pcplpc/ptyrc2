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
// There is no timer, no network, no file, no console, no `require`:
// `tick` is the only clock. `Date.now()` is session time (0 at the
// start), not the wall clock. `Math.random()` draws from the host.
// Modules: "ptyrc", and files of the app: "./lib/util.js". State does
// not survive a restart.
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

  /** Ends the app. Does not return. */
  export function exit(code?: number): never;
}
