// SPDX-License-Identifier: GPL-3.0-or-later
// Clockface: a stopwatch in a widget, the model of a deck app.
//
// Two sizes: block digits (17x3, a Canvas) where there is room, a line
// (5x1) where not; the driver picks. The widget asks for the keyboard,
// quietly: focused (ctrl+t twice), `space` pauses, `r` resets. Time is
// session time, the `now` of `tick`.

import { period } from "ptyrc";
import { Deck } from "ptyrc/deck";
import { face, SIZES } from "./face.js";

let deck;
let shown = 0; // ms on the face when it last paused, or at `since`
let since = 0; // session time it last started; null: paused

function elapsed(now) {
  return since === null ? shown : shown + now - since;
}

function key(name) {
  if (name === "space") {
    if (since === null) since = deck.now;
    else (shown = elapsed(deck.now)), (since = null);
  } else if (name === "r") {
    shown = 0;
    since = since === null ? null : deck.now;
  } else {
    return false; // not ours: nothing to draw
  }
}

export function init(args) {
  deck = new Deck({ border: "round", prefer: "top-right", every: args.every });
  deck.add("clockface", SIZES, (w, h) => face(elapsed(deck.now), w, h, since === null), { key });
  period(1000);
}

export function tick(now) {
  deck.tick(now);
}

export function event(topic, data) {
  deck.event(topic, data);
}
