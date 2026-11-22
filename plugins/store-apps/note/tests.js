// SPDX-License-Identifier: GPL-3.0-or-later
// The unit tests of note, `ptyrc dev test --unit`: those of the twin
// (test_note.py) on the pure parts, the live parts on fake files, and
// the twin's looks (`twin.js`).

import { PtyrcError, assert, assertEqual, assertThrows, cells } from "ptyrc";
import { Board, edit, options, read } from "./app.js";
import { DIM, Desk, PAPER, PLAIN, POSTIT, SIZES, change, item, listed, look, named, note, parse, render, same, tally, title, trimmed, wrap } from "./note.js";
import { FACES, SCENARIOS, SOURCES, VIEWS } from "./twin.js";

const TEXT = SOURCES.text;
const SMALL = SOURCES.small;
const LOOKS = {
  plain: look(),
  all: look({ count: true, done: true, postit: true }),
  open: look({ open: true }),
  count: look({ count: true }),
  every: look({ count: true, done: true, open: true, postit: true }),
};

/** Rows of runs as the twin's script writes them: runs of one style
 * merged, empty runs out, `[t, fg, bg, bold, inverse]`. */
function norm(rows) {
  return rows.map((row) => {
    const out = [];
    for (const r of row) {
      const t = String(r.t);
      if (!t) continue;
      const style = [r.fg ?? null, r.bg ?? null, !!r.bold, !!r.inverse];
      const last = out[out.length - 1];
      if (last && JSON.stringify(last.slice(1)) === JSON.stringify(style)) last[0] += t;
      else out.push([t, ...style]);
    }
    return out;
  });
}

function fnv(text) {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 0x01000193) >>> 0;
  return h;
}

const digest = (rows) => fnv(JSON.stringify(norm(rows)));
const text = (rows) => rows.map((row) => row.map((r) => r.t).join(""));
const made = (source = TEXT, name = "TODO.md") => note(name, source);

export function test_twin_faces() {
  for (const f of FACES) {
    const n = f.source === "missing" ? note("gone.md") : made(SOURCES[f.source]);
    const lk = LOOKS[f.look];
    const at = `${f.source} ${f.look}`;
    assertEqual(title(n, lk), f.title, `${at}: title`);
    assertEqual(trimmed(n, lk), f.trimmed, `${at}: trimmed`);
    assertEqual(trimmed(n, lk, true), f.foot, `${at}: trimmed with the foot`);
    VIEWS.forEach(([w, h], i) => assertEqual(digest(render(n, w, h, lk)), f.views[i], `${at}: ${w}x${h}: ${JSON.stringify(norm(render(n, w, h, lk)))}`));
  }
}

export function test_twin_desk() {
  for (const s of SCENARIOS) {
    let source = SOURCES[s.source];
    const lk = LOOKS[s.look];
    const desk = new Desk();
    for (const [key, change_, after, sel, mode, views] of s.steps) {
      const at = `${s.source} ${s.look} ${JSON.stringify(key)}`;
      let what = null;
      if (Array.isArray(key)) desk.paste(key[1]);
      else what = desk.key(listed(made(source), lk), key);
      assertEqual(what, change_, `${at}: the change`);
      if (what !== null) source = change(source, what);
      assertEqual(source, after, `${at}: the text`);
      const n = made(source);
      VIEWS.forEach(([w, h], i) => assertEqual(digest(render(n, w, h, lk, desk)), views[i], `${at}: ${w}x${h}: ${JSON.stringify(norm(render(n, w, h, lk, desk)))}`));
      // After the renders, as the twin's script reads them: a render
      // brings the bar back onto the list.
      assertEqual([desk.sel, desk.mode], [sel, mode], `${at}: sel, mode`);
    }
  }
}

export function test_parse() {
  assertEqual(
    parse(TEXT).map((i) => [i.kind, i.text, i.indent]),
    [
      ["head", "Groceries", 0],
      ["blank", "", 0],
      ["done", "bread", 0],
      ["blank", "", 0],
      ["open", "milk, the whole one, from the shop at the corner", 0],
      ["done", "eggs", 0],
      ["open", "coffee", 0],
      ["open", "filters", 1],
      ["blank", "", 0],
      ["head", "Later", 0],
      ["blank", "", 0],
      ["bullet", "call the plumber", 0],
      ["text", "quoted", 0],
      ["text", "plain text", 0],
    ],
  );
  assertEqual([parse(""), parse("\n\n")], [[], []]);
  assertEqual([tally(made()), tally(made("words"))], [[2, 5], [0, 0]]);
}

export function test_wrap() {
  assertEqual(wrap("one two three", 7), ["one two", "three"]);
  assertEqual(wrap("abcdefghij kl", 4), ["abcd", "efgh", "ij", "kl"]);
  assertEqual(wrap("", 5), [""]);
}

export function test_one_line() {
  const n = made();
  assertEqual(text(render(n, 5, 1)), ["2/5"]);
  assertEqual(text(render(n, 18, 1)), ["2/5 milk, the who…"]);
  assertEqual(text(render(made("- [x] a\n- [x] b\n"), 18, 1)), ["2/2 all done"]);
  assertEqual(text(render(made("# Title\nwords"), 18, 1)), ["Title"]);
  assertEqual(text(render(made(""), 18, 1)), ["empty"]);
  assertEqual(text(render(note("x.md"), 18, 3)), ["no x.md"]);
  assertEqual(text(render(note("x.md"), 5, 1)), ["—"]);
}

export function test_fits_or_not() {
  const n = made();
  const full = text(render(n, 48, 16));
  assertEqual([full[0], full[2]], ["Groceries", "☑ bread"]);
  assertEqual(full.slice(4, 6), ["☐ milk, the whole one, from the shop at the", "  corner"]);
  assert(full.includes("  ☐ filters"));
  assertEqual(full.slice(-3), ["• call the plumber", "quoted", "plain text"]);
  // Too long: without what is done.
  const less = text(render(n, 48, 12));
  assert(!less.includes("☑ bread") && less.includes("Later") && less.length === 12);
  // Shorter yet: the open boxes alone; then cut.
  assertEqual(text(render(n, 30, 6)), ["☐ milk, the whole one, from", "  the shop at the corner", "☐ coffee", "  ☐ filters"]);
  assertEqual(text(render(n, 24, 3)), ["☐ milk, the whole one,", "  from the shop at the", "  corner …"]);
  assertEqual(text(render(made("a\nb\nc\nd"), 10, 2)), ["a", "b …"]);
}

export function test_looks() {
  const n = made();
  const bare = render(n, 48, 16);
  assertEqual([bare[0], bare[2]], [[{ t: "Groceries", bold: true }], [{ t: "☑ bread" }]]);
  assertEqual(render(n, 48, 16, look({ done: true }))[2][0].fg, DIM);
  const hidden = text(render(n, 48, 16, look({ open: true })));
  assert(!hidden.some((r) => r.includes("☑")) && hidden.includes("☐ coffee"));
  const counted = text(render(n, 20, 6, look({ count: true })));
  assertEqual(counted[0], "━━━━━━╾───────── 2/5");
  assert(counted[1].startsWith("☐ milk"));
  assertEqual(title(n, look({ count: true })), "TODO.md 2/5");
  assertEqual([title(n), title(made("x"), look({ count: true }))], ["TODO.md", "TODO.md"]);
  for (const row of render(n, 30, 6, look({ postit: true, done: true }))) {
    assertEqual(row.reduce((s, r) => s + [...r.t].length, 0), 30);
    assert(row.every((r) => r.bg === PAPER));
  }
  assertEqual([POSTIT.style, [...POSTIT.corners].length], [{ fg: PAPER }, 4]);
}

export function test_every_variant() {
  const sources = [TEXT, "", "word ".repeat(80), "- [x] a\n", "x".repeat(300), "- [ ] " + "y".repeat(90)];
  for (const lk of [LOOKS.plain, LOOKS.all, LOOKS.open]) {
    for (const n of [...sources.map((s) => made(s)), note("gone")]) {
      for (const [w, h] of [...SIZES, [1, 1], [3, 2], [7, 3], [120, 40]]) {
        const rows = render(n, w, h, lk);
        assert(rows.length > 0 && rows.length <= h, `${w}x${h}`);
        for (const row of text(rows)) assert(cells(row) <= w, `${w}x${h}: ${row}`);
      }
      const sizes = trimmed(n, lk);
      assertEqual(sizes.map(([w]) => w), SIZES.map(([w]) => w));
      sizes.forEach(([, h], i) => assert(h >= 1 && h <= SIZES[i][1]));
      assertEqual(Object.keys(named(sizes)).length, SIZES.length);
    }
  }
}

export function test_options() {
  const o = options({ files: [{ file: "f0", name: "TODO.md" }] });
  assertEqual([o.files, o.look, o.interactive, o.merged, o.cycle], [[{ file: "f0", name: "TODO.md" }], PLAIN, false, false, 10]);
  assertEqual(options({ files: [{ file: "f0" }], all: true }).look, LOOKS.all);
  const two = options({ files: [{ file: "f0" }, { file: "f1" }], open: true, merged: true, cycle: 0.5 });
  assert(two.look.open && two.merged && two.cycle === 0.5 && two.files.length === 2);
  for (const c of [0, -1, Infinity, NaN]) assertThrows(() => options({ files: [{ file: "f0" }], cycle: c }), RangeError);
  // -i is not in -a.
  assert(!options({ files: [{ file: "f0" }], all: true }).interactive);
  const i = options({ files: [{ file: "f0" }], interactive: true });
  assert(i.interactive && !i.look.count);
}

export function test_items_know_their_line() {
  assertEqual(
    parse(TEXT).filter((i) => i.kind !== "blank").map((i) => [i.line, i.raw]),
    [
      [0, "Groceries"],
      [2, "bread"],
      [5, "milk, the **whole** one, from the [shop](http://x) at the corner"],
      [6, "eggs"],
      [7, "coffee"],
      [8, "filters"],
      [10, "Later"],
      [12, "call the `plumber`"],
      [13, "quoted"],
      [14, "plain text"],
    ],
  );
  // Where it is written is not what it is.
  assert(same(item("open", "a", 0, 3, "a"), item("open", "a")));
  assertEqual(listed(made(), look({ open: true })).slice(0, 3).map((i) => i.text), ["Groceries", "milk, the whole one, from the shop at the corner", "coffee"]);
}

export function test_change() {
  const lines = SMALL.split("\n").slice(0, -1);
  const changed = (...what) => change(SMALL, what).split("\n").slice(0, -1);
  assertEqual(changed("tick", 2)[2], "- [ ] bread");
  assertEqual(changed("tick", 3)[3], "* [x] milk, the **whole** one");
  assertEqual(changed("tick", 5), lines);
  assertEqual(changed("words", 3, "cream")[3], "* [ ] cream");
  assertEqual(changed("words", 0, "Food")[0], "# Food");
  assertEqual(changed("words", 4, "paper")[4], "  1. [ ] paper");
  assertEqual(changed("words", 5, "bare")[5], "bare");
  assertEqual(changed("delete", 4), [...lines.slice(0, 4), ...lines.slice(5)]);
  assertEqual(changed("swap", 2, 3).slice(2, 4), ["* [ ] milk, the **whole** one", "- [x] bread"]);
  // Added: the indent and the mark of the line above, an open box.
  assertEqual(changed("add", 2, "jam").slice(2, 4), ["- [x] bread", "- [ ] jam"]);
  assertEqual(changed("add", 3, "jam")[4], "* [ ] jam");
  assertEqual(changed("add", 4, "jam")[5], "  1. [ ] jam");
  assertEqual(changed("add", 0, "jam")[1], "- [ ] jam");
  assertEqual(changed("add", -1, "jam"), [...lines, "- [ ] jam"]);
  // The ends of the lines are kept; a last line without one gets one.
  assertEqual(change("- a\r\n- b", ["add", 1, "c"]), "- a\r\n- b\n- [ ] c\n");
  assertEqual(change("- a\r\n- b\r\n", ["add", 0, "c"]), "- a\r\n- [ ] c\r\n- b\r\n");
  assertEqual(change("- a\r\n- b", ["swap", 0, 1]), "- b\r\n- a");
  assertEqual(change("", ["add", -1, "c"]), "- [ ] c\n");
  assertEqual(change("x", ["add", -1, "c"]), "x\n- [ ] c\n");
  assertEqual(change("> said\n", ["words", 0, "told"]), "> told\n");
  assertEqual(change("##   Head ##\n", ["words", 0, "New"]), "##   New\n");
}

function deskKeys(desk, source, ...names) {
  const out = [];
  for (const name of names) {
    const what = desk.key(listed(made(source)), name);
    if (what !== null) out.push(what);
  }
  return out;
}

export function test_desk() {
  let desk = new Desk();
  assertEqual(deskKeys(desk, SMALL, "down", "space"), [["tick", 2]]);
  assertEqual(deskKeys(desk, SMALL, "j", "enter", "end", "space"), [["tick", 3]]);
  assertEqual(desk.sel, 4);
  // A move: the bar follows its item.
  assertEqual(deskKeys(desk, SMALL, "shift+up", "K"), [["swap", 5, 4], ["swap", 4, 3]]);
  assertEqual(desk.sel, 2);
  assertEqual(deskKeys(desk, SMALL, "home", "shift+up", "J"), [["swap", 0, 2]]);
  assertEqual(desk.sel, 1);
  // Added under the bar; the bar goes on it. Nothing typed: nothing.
  assertEqual(deskKeys(desk, SMALL, "a", "j", "a", "space", "m", "enter"), [["add", 2, "ja m"]]);
  assertEqual([desk.sel, desk.mode], [2, null]);
  assertEqual(deskKeys(desk, SMALL, "a", "enter", "a", "x", "ctrl+c"), []);
  assertEqual([desk.mode, desk.editor.text], ["add", "x"]);
  // Cancelled: the line cleared, then enter.
  assertEqual(deskKeys(desk, SMALL, "ctrl+u", "enter"), []);
  assertEqual([desk.sel, desk.mode], [2, null]);
  assertEqual(deskKeys(desk, SMALL, "e", "ctrl+u", "enter"), []);
  assertEqual(desk.mode, null);
  // Edited: the words as written; the same words: nothing.
  assertEqual(deskKeys(desk, SMALL, "e", "enter"), []);
  assertEqual(deskKeys(desk, SMALL, "e", "ctrl+u", "o", "k", "enter"), [["words", 3, "ok"]]);
  desk.key(listed(made(SMALL)), "e");
  assertEqual(desk.editor.text, "milk, the **whole** one");
  desk.paste("more\nof it");
  assert(desk.editor.text.endsWith("onemore of it"));
  // The focus lost: dropped.
  assert(desk.rest() && desk.mode === null && !desk.rest());
  // Deleted when `y` follows; any other key keeps.
  assertEqual(deskKeys(desk, SMALL, "d", "n", "d", "y"), [["delete", 3]]);
  // An empty note: a line can be added, nothing else.
  desk = new Desk();
  assertEqual(deskKeys(desk, "", "space", "e", "d", "y", "J", "a", "x", "enter"), [["add", -1, "x"]]);
}

export function test_desk_every_size() {
  for (const lk of [PLAIN, LOOKS.every]) {
    for (const source of [SMALL, TEXT, ""]) {
      for (const mode of [null, "add", "edit", "delete"]) {
        const desk = new Desk();
        desk.sel = 2;
        desk.mode = mode;
        for (const [w, h] of [...SIZES, [1, 1], [3, 2], [4, 3], [60, 20]]) {
          const rows = render(made(source), w, h, lk, desk);
          assertEqual(rows.length, h);
          for (const row of text(rows)) assertEqual(cells(row), w, `${w}x${h}`);
          if (lk.postit) assert(rows.every((row) => row.every((r) => r.bg === PAPER)));
        }
      }
    }
  }
}

export function test_desk_render() {
  const desk = new Desk();
  desk.sel = 1;
  let rows = render(made(SMALL), 30, 7, PLAIN, desk);
  assertEqual(text(rows), ["List", "☑ bread", "☐ milk, the whole one", "  ☐ filters", "plain", "", "space tick  a add  esc leave"].map((t) => t.padEnd(30)));
  assertEqual(rows.map((row) => !!row[0].inverse), [false, true, false, false, false, false, false]);
  // Adding: the line typed is under the bar, with a cursor.
  for (const name of ["a", "j"]) desk.key(listed(made(SMALL)), name);
  rows = render(made(SMALL), 30, 7, PLAIN, desk);
  assertEqual(text(rows)[2], "☐ j".padEnd(30));
  assert(text(rows)[3].startsWith("☐ milk") && rows[2].some((r) => r.inverse));
  assert(text(rows)[6].startsWith("enter ok  empty: cancel"));
  desk.rest();
  // Too long for the rows: it scrolls with the bar; two rows: no foot.
  desk.sel = 4;
  assertEqual(text(render(made(SMALL), 12, 2, PLAIN, desk)), ["  ☐ filters ", "plain".padEnd(12)]);
  desk.key(listed(made(SMALL)), "d");
  assert(text(render(made(SMALL), 20, 3, PLAIN, desk))[2].startsWith("y delete"));
  // A row more for the foot, from two rows on.
  const bare = trimmed(made(SMALL));
  assertEqual(bare.map(([w, h], n) => [w, h + (h > 1 || n > 1 ? 1 : 0)]), trimmed(made(SMALL), PLAIN, true));
}

/** A file named, in memory: `stat`, `textWithMtime`, `write` as the
 * runner answers them; `text` null: absent; `writable` false: denied. */
class Fake {
  constructor(name, text, { writable = true } = {}) {
    this.name = name;
    this.content = text;
    this.mtime = text === null ? 0 : 1000;
    this.writable = writable;
    this.writes = 0;
  }
  stat() {
    return this.content === null ? { exists: false, size: 0, mtimeMs: 0 } : { exists: true, size: this.content.length, mtimeMs: this.mtime };
  }
  textWithMtime() {
    if (this.content === null) throw new PtyrcError({ code: "no-file", message: "file.read: absent" });
    return { text: this.content, mtimeMs: this.mtime };
  }
  write(text, { ifMtime } = {}) {
    if (!this.writable) throw new PtyrcError({ code: "denied", message: "file.write: denied" });
    if (ifMtime !== undefined && ifMtime !== this.mtime) throw new PtyrcError({ code: "not-usable", message: "file.write: changed since it was read" });
    this.writes += 1;
    this.set(text);
    return { size: text.length, mtimeMs: this.mtime };
  }
  /** Another program writes it. */
  set(text) {
    this.content = text;
    this.mtime += 1000;
  }
}

export function test_edit() {
  const f = new Fake("TODO.md", SMALL);
  const [was] = read(f);
  assert(edit(f, was, ["tick", 3]));
  assertEqual(f.content.split("\n")[3], "* [x] milk, the **whole** one");
  // The file is no longer as it was read: nothing is written.
  assert(!edit(f, was, ["tick", 3]));
  assert(!edit(f, was, ["swap", 2, 3]));
  assert(edit(f, was, ["tick", 2])); // that line stands
  f.set("- [ ] one\n");
  assert(!edit(f, was, ["delete", 4]));
  assertEqual([f.content, f.writes], ["- [ ] one\n", 2]);
  // A file that is not there is made by a line added.
  const fresh = new Fake("new.md", null);
  assert(edit(fresh, read(fresh)[0], ["add", -1, "first"]));
  assertEqual(fresh.content, "- [ ] first\n");
  // Not to be written: false.
  const denied = new Fake("x.md", null, { writable: false });
  assert(!edit(denied, note("x.md"), ["add", -1, "a"]));
}

export function test_live() {
  // A widget a file; a file written is read again, and never written by
  // the app.
  const one = new Fake("TODO.md", "- [ ] first thing\n- [x] second\n");
  const two = new Fake("ideas.md", "an idea\n");
  const b = new Board(options({ files: [{}, {}], count: true }), [one, two]);
  assertEqual([b.count, b.title(0), b.title(1)], [2, "TODO.md 1/2", "ideas.md"]);
  assert(!b.step(1, false));
  one.set("- [x] first thing\n- [x] second\n- [ ] a third, new\n");
  assert(b.step(2, false));
  assertEqual(b.title(0), "TODO.md 2/3");
  assert(text(render(b.notes[0], 48, 16)).includes("☐ a third, new"));
  two.content = null;
  assert(b.step(3, false));
  assertEqual(text(render(b.notes[1], 18, 3)), ["no ideas.md"]);
  assertEqual([one.writes, two.writes], [0, 0]);
}

export function test_live_merged() {
  const a = new Fake("a.md", "- [ ] alpha\n");
  const bee = new Fake("b.md", "beta\n");
  const b = new Board(options({ files: [{}, {}], merged: true, cycle: 0.5, postit: true }), [a, bee]);
  assertEqual(b.count, 1);
  const seen = new Set([b.title(0)]);
  assert(b.step(1, false));
  seen.add(b.title(0));
  assertEqual([...seen], ["a.md", "b.md"]);
  // Focused: the turn stands still.
  assert(!b.step(2, true) && b.title(0) === "b.md");
  assert(b.step(2.5, false) && b.title(0) === "a.md");
  // The sizes: the largest of each rank.
  const [wa, wb] = [trimmed(b.notes[0], b.o.look), trimmed(b.notes[1], b.o.look)];
  assertEqual(b.sizes(0), wa.map(([w, h], r) => [Math.max(w, wb[r][0]), Math.max(h, wb[r][1])]));
}

export function test_live_interactive() {
  // Focused, the keys write the file.
  const f = new Fake("TODO.md", "# List\n\n- [ ] first\n- [x] second\n");
  const b = new Board(options({ files: [{}], interactive: true, count: true }), [f]);
  const desk = new Desk();
  const typed = (...keys) => {
    for (const k of keys) b.key(0, desk, k);
    return f.content.split("\n").slice(0, -1);
  };
  assertEqual(typed("j", "space"), ["# List", "", "- [x] first", "- [x] second"]);
  assertEqual(b.title(0), "TODO.md 2/2");
  assertEqual(typed("a", "t", "h", "i", "r", "d", "enter"), ["# List", "", "- [x] first", "- [ ] third", "- [x] second"]);
  assertEqual(typed("shift+up"), ["# List", "", "- [ ] third", "- [x] first", "- [x] second"]);
  b.key(0, desk, "e");
  b.key(0, desk, "ctrl+u");
  desk.paste("3rd\n");
  assertEqual(typed("enter"), ["# List", "", "- [ ] 3rd", "- [x] first", "- [x] second"]);
  assertEqual(typed("j", "d", "y"), ["# List", "", "- [ ] 3rd", "- [x] second"]);
  // The focus lost while a line is typed: dropped.
  typed("a", "l", "o", "s", "t");
  assert(desk.rest());
  assertEqual(f.content, "# List\n\n- [ ] 3rd\n- [x] second\n");
  // Changed by another hand under the bar: the key is lost, the file
  // read again.
  f.set("# List\n\n- [ ] 3rd, changed\n- [x] second\n");
  assertEqual(typed("k", "space"), ["# List", "", "- [ ] 3rd, changed", "- [x] second"]);
  assertEqual(typed("space"), ["# List", "", "- [x] 3rd, changed", "- [x] second"]);
}
