// SPDX-License-Identifier: GPL-3.0-or-later
// ask: a question in a widget, the answer to the caller.
//
//   ptyrc app ptyrc/ask "Deploy?" yes no later   # a choice
//   ptyrc app ptyrc/ask -m "Which?" a b c        # some of them, one a line
//   ptyrc app ptyrc/ask -o "Which?" a b          # or another, typed
//   ptyrc app ptyrc/ask "Name?"                  # a line
//   ptyrc app ptyrc/ask -t "Why?"                # several lines
//
// The widget asks for the keyboard: the user focuses it (ctrl+t twice),
// answers, `enter` validates. The answer is `app.result`, status 0; the
// widget closed (ctrl+x) or `--timeout` past: no result, status 1.

import { exit, log, period, result } from "ptyrc";
import { Deck } from "ptyrc/deck";
import { Choice, DONE, Line, Text } from "./ui.js";

/** The options of `init`: `{question, choices, multi, other, text,
 * default, title, timeout}`. Throws a `RangeError`, its message a line
 * for the log. */
export function options(args) {
  const words = Array.isArray(args.words) ? args.words : [args.words];
  if (!words.length || typeof words[0] !== "string") throw new RangeError("ask: a QUESTION");
  const timeout = args.timeout ?? null;
  if (timeout !== null && !(timeout > 0 && timeout < Infinity)) throw new RangeError(`ask: --timeout: not a positive number of seconds: ${timeout}`);
  return {
    question: words[0],
    choices: words.slice(1),
    multi: args.multi === true,
    other: args.other === true,
    text: args.text === true,
    default: args.default ?? "",
    title: args.title ?? null,
    timeout,
  };
}

/** The model the options name: a `Choice` with choices, else a `Text`
 * (`--text`) or a `Line`; `--default` the text to start from, or the
 * choice the bar starts on (ticked with `--multi`). */
export function model(o) {
  if (o.choices.length) {
    const choice = new Choice(o.question, o.choices, { multi: o.multi, other: o.other });
    const at = o.choices.indexOf(o.default);
    if (at >= 0) {
      choice.listing.sel = at;
      if (o.multi) choice.ticked.add(at);
    }
    return choice;
  }
  if (o.text) return new Text(o.question, { text: o.default });
  return new Line(o.question, { text: o.default });
}

// -- live --------------------------------------------------------------------

let o;
let asked;
let deck;
let widget;

/** The widget released and hidden, then the end: `value` the result,
 * status 0; null: none, status 1. */
function finish(value) {
  widget.release();
  widget.hide();
  if (value === null) exit(1);
  result(value, 0);
  exit(0);
}

export function init(args) {
  try {
    o = options(args);
  } catch (e) {
    if (!(e instanceof RangeError)) throw e;
    log(e.message);
    exit(2);
  }
  asked = model(o);
  deck = new Deck({ border: "round", prefer: "center", fill: 1 });
  const handlers = {
    key: (name) => {
      if (asked.key(name) === DONE) finish(asked.result);
    },
    paste: (text) => asked.paste(text),
    closed: () => exit(1),
    ask: { quiet: false },
  };
  if (o.title !== null) handlers.title = o.title;
  widget = deck.add("ask", asked.sizes(), (w, h, wd) => asked.render(w, h, wd.focused), handlers);
  if (o.timeout !== null) period(Math.max(33, Math.round(o.timeout * 1000)));
}

export function tick(now) {
  if (o.timeout !== null && now >= o.timeout * 1000) finish(null);
}

export function event(topic, data) {
  deck.event(topic, data);
}
