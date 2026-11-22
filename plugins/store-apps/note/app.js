// SPDX-License-Identifier: GPL-3.0-or-later
// note: a file as a sticky note; with `-i`, one to write on.
//
//   ptyrc app ptyrc/note                     # TODO.md of the working directory
//   ptyrc app ptyrc/note TODO.md ideas.md    # a widget a file
//   ptyrc app ptyrc/note -a --prefer top-right
//   ptyrc app ptyrc/note a.md b.md --merged  # one widget, the files in turn
//   ptyrc app ptyrc/note -i                  # ctrl+t twice: tick, add, edit
//
// `-c`: the boxes ticked over all of them, in the title and as a bar;
// `-x`: what is done dimmed; `-o`: what is done left out; `-p`: a yellow
// ground and the border `POSTIT`; `-a`: `-c -x -p`. The faces: `note.js`.
//
// `-i`: the notes are quiet widgets that take keys. Focused, a note is a
// list, an item a row, with a bar (`Desk`): `space` or `enter` ticks a
// box, `a` adds one under the bar, `e` edits the words, `d` then `y`
// deletes, `shift+↑↓` (`K`, `J`) moves the item; ctrl+x closes the
// note, the app ends with the last. Each change is one of a line of the
// file (`change`), the whole file written at once (`file.write`, a
// rename by the runner); the other lines are kept as they are. A file
// whose lines named changed since it was read is read again, the key is
// lost. With `--merged` the turn stands still while the note is focused.
//
// A file is read again when its mtime changes, looked at each second.

import { PtyrcError, exit, file, log, period } from "ptyrc";
import { Deck } from "ptyrc/deck";
import { Desk, POSTIT, change, listed, look, merged, named, note, render, stands, title, trimmed } from "./note.js";

/** The options of `init`: `{files, look, interactive, merged, cycle}`.
 * Throws a `RangeError`, its message a line for the log. */
export function options(args) {
  const cycle = args.cycle ?? 10;
  if (!(cycle > 0 && cycle < Infinity)) throw new RangeError(`note: --cycle: not a positive number of seconds: ${cycle}`);
  const all = args.all === true;
  const files = args.files === undefined || args.files === null ? [] : [].concat(args.files);
  if (!files.length) throw new RangeError("note: no file");
  return {
    files,
    look: look({ count: all || args.count === true, done: all || args.done === true, open: args.open === true, postit: all || args.postit === true }),
    interactive: args.interactive === true,
    merged: args.merged === true,
    cycle,
  };
}

const quiet = (e) => {
  if (!(e instanceof PtyrcError)) throw e;
};

/** The note of a file named and its mtime; missing (mtime 0) when it
 * cannot be read. */
export function read(f) {
  try {
    const { text, mtimeMs } = f.textWithMtime();
    return [note(f.name, text), mtimeMs];
  } catch (e) {
    quiet(e);
    return [note(f.name), 0];
  }
}

/** The mtime of a file named; absent: 0. */
export function stamp(f) {
  try {
    const s = f.stat();
    return s.exists ? s.mtimeMs : 0;
  } catch (e) {
    quiet(e);
    return 0;
  }
}

/** A change of `Desk.key` made in the file, the whole written. False,
 * nothing written: the file is not as `n` was read (the lines named
 * differ), or cannot be written. Absent: made by a line added. */
export function edit(f, n, what) {
  let text = "";
  let mtime = null;
  try {
    ({ text, mtimeMs: mtime } = f.textWithMtime());
  } catch (e) {
    quiet(e);
    if (e.code !== "no-file") return false;
  }
  if (!stands(n, text, what)) return false;
  try {
    f.write(change(text, what), mtime === null ? {} : { ifMtime: mtime });
  } catch (e) {
    quiet(e);
    return false;
  }
  return true;
}

/** The notes of the files named, as shown: a widget a file, or one for
 * all (`merged`, `turn`). `files`: objects of `file(handle)`. */
export class Board {
  constructor(o, files) {
    this.o = o;
    this.files = files;
    const read0 = files.map(read);
    this.notes = read0.map(([n]) => n);
    this.stamps = read0.map(([, m]) => m);
    this.turn = 0;
    this.since = 0;
    this.count = o.merged ? 1 : files.length;
  }

  /** The note shown by widget `n`. */
  which(n) {
    return this.o.merged ? this.turn % this.files.length : n;
  }

  title(n) {
    return title(this.notes[this.which(n)], this.o.look);
  }

  /** The sizes of widget `n`, a row more for the foot with `-i`. */
  sizes(n) {
    const of = (m) => trimmed(this.notes[m], this.o.look, this.o.interactive);
    return this.o.merged ? merged(this.notes.map((_, m) => of(m))) : of(n);
  }

  /** The file `m` as it is now. */
  again(m) {
    [this.notes[m], this.stamps[m]] = read(this.files[m]);
  }

  /** A second passed, at session second `now`: the turn of `--merged`
   * (held while `focused`), the files changed read again. Whether
   * anything changed. */
  step(now, focused) {
    let changed = this.o.merged && this.files.length > 1 && now - this.since >= this.o.cycle;
    if (changed && focused) {
      // A focused note stays: the keys are for the one shown.
      changed = false;
      this.since = now;
    }
    if (changed) {
      this.turn += 1;
      this.since = now;
    }
    this.files.forEach((f, m) => {
      if (stamp(f) !== this.stamps[m]) {
        this.again(m);
        changed = true;
      }
    });
    return changed;
  }

  /** A key to widget `n` and its desk: the change written, the file
   * read again. */
  key(n, desk, name) {
    const m = this.which(n);
    const what = desk.key(listed(this.notes[m], this.o.look), name);
    if (what === null) return false;
    edit(this.files[m], this.notes[m], what);
    this.again(m);
    return true;
  }
}

// -- live --------------------------------------------------------------------

let board;
let deck;
let widgets = [];
let desks = [];
let declared = [];
let due = 1000;
/** The notes closed by their key (ctrl+x): out of the deck. */
const gone = new Set();

/** Each widget declared again when its sizes changed; its title. */
function sync() {
  widgets.forEach((w, n) => {
    if (gone.has(n)) return;
    const sizes = board.sizes(n);
    if (JSON.stringify(sizes) !== JSON.stringify(declared[n])) {
      declared[n] = sizes;
      deck.resize(w, named(sizes));
    }
    const t = board.title(n);
    if (w.options.title !== t) {
      w.options.title = t;
      w.set(null, { title: t });
    }
  });
}

export function init(args) {
  let o;
  try {
    o = options(args);
  } catch (e) {
    if (!(e instanceof RangeError)) throw e;
    log(e.message);
    exit(2);
  }
  board = new Board(o, o.files.map((h) => file(h)));
  deck = new Deck({ border: "round" });
  for (let n = 0; n < board.count; n++) {
    const tag = n === 0 ? "note" : `note-${n + 1}`;
    const handlers = { title: board.title(n) };
    if (o.look.postit) handlers.border = POSTIT;
    if (o.interactive) {
      const desk = new Desk();
      desks.push(desk);
      handlers.key = (name) => {
        if (board.key(n, desk, name)) sync();
      };
      handlers.paste = (text) => desk.paste(text);
      handlers.closed = () => {
        gone.add(n);
        if (!deck.widgets.length) exit(0);
      };
    }
    declared.push(board.sizes(n));
    const draw = (w, h, widget) => render(board.notes[board.which(n)], w, h, o.look, widget.focused ? desks[n] : null);
    widgets.push(deck.add(tag, named(declared[n]), draw, handlers));
  }
  period(1000);
}

export function tick(now) {
  if (now >= due) {
    due = now + 1000;
    if (board.step(now / 1000, widgets.some((w) => w.focused))) sync();
  }
  deck.tick(now);
}

export function event(topic, data) {
  const was = widgets.map((w) => w.focused);
  deck.event(topic, data);
  // The focus lost: what was typed is dropped.
  desks.forEach((desk, n) => {
    if (was[n] && !widgets[n].focused && desk.rest()) deck.draw(widgets[n]);
  });
}
