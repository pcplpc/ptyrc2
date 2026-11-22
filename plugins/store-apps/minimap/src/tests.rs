// SPDX-License-Identifier: GPL-3.0-or-later
//! The tests of the twin (`test_minimap.py`), and its faces
//! (`twin.json`, made by
//! `plan/sessions/22112026-01-migration/files-minimap/twin.py`).

use std::collections::HashMap;

use ptyrc_app::{Color, Run, Value, json};

use crate::map::{
    BOTH, CODE, CURSOR, Doc, EDITED, GROUND, HEIGHTS, INKS, Look, NOTE, VISITED, face, heights,
    inks, line_at, scale, title,
};
use crate::{Options, View, merge, tip_at, tip_text};

fn plain(rows: &[Vec<Run>]) -> Vec<String> {
    rows.iter()
        .map(|row| row.iter().map(|r| r.t.as_str()).collect())
        .collect()
}

fn num(c: Color) -> Option<u8> {
    match c {
        Color::Palette(n) => Some(n),
        _ => None,
    }
}

/// `(fg, bg)` of each cell.
fn cells(rows: &[Vec<Run>]) -> Vec<Vec<(Option<u8>, Option<u8>)>> {
    (rows.iter())
        .map(|row| {
            (row.iter())
                .flat_map(|r| r.t.chars().map(|_| (num(r.fg), num(r.bg))))
                .collect()
        })
        .collect()
}

fn doc(lines: &[&str], width: i64) -> Doc {
    Doc {
        lines: lines.iter().map(|l| l.to_string()).collect(),
        width,
        ..Doc::default()
    }
}

fn xs(n: usize) -> Vec<String> {
    vec!["x".to_string(); n]
}

fn look() -> Look {
    Look::default()
}

#[test]
fn test_scale() {
    assert_eq!(scale(0, 80, 6, 8), (7, 1));
    assert_eq!(scale(32, 80, 6, 8), (7, 1), "a line a dot row");
    assert_eq!(scale(33, 80, 6, 8), (7, 2));
    assert_eq!(scale(1000, 80, 6, 24), (7, 11));
    assert_eq!(scale(10, 12, 6, 8), (1, 1));
    assert_eq!(scale(10, 400, 10, 8), (5, 1), "cut at WIDTH");
}

#[test]
fn test_inks() {
    assert_eq!(inks("", 2, 3), 0);
    assert_eq!(inks("ab", 2, 3), 0b1);
    assert_eq!(inks("    x", 2, 3), 0b100);
    assert_eq!(inks("a b c d e f g", 2, 3), 0b111111, "the rest is cut");
    assert_eq!(inks("\t x", 1, 2), 0b0100);
    assert_eq!(inks(&"x".repeat(80), 1, 40), (1u128 << 80) - 1, "40 cells");
}

#[test]
fn test_face_dots() {
    // A column a dot, a line a dot row: the text is the picture.
    let d = doc(&["xx", "x", " x", "", "x"], 4);
    assert_eq!(plain(&face(&d, 2, 2, &look(), 0.0)), ["⠫ ", "⠁ "]);
    // Two lines a dot row: on when one of the two holds ink.
    let d = doc(&["x", "", "", "", "x", "x", "", ""], 4);
    assert_eq!(plain(&face(&d, 2, 1, &look(), 0.0)), ["⠅ "]);
    // Every row is as wide as the map, whatever is known.
    for count in [0, 1, 7, 300] {
        let d = Doc {
            lines: vec!["x".repeat(9); count],
            ..Doc::default()
        };
        let rows = plain(&face(&d, 6, 8, &look(), 0.0));
        assert_eq!(rows.len(), 8);
        assert!(rows.iter().all(|r| r.chars().count() == 6));
    }
}

#[test]
fn test_face_view() {
    let mut d = Doc {
        y0: 100,
        lines: xs(32),
        top: 108,
        view: 8,
        width: 4,
        ..Doc::default()
    };
    let got = cells(&face(&d, 2, 8, &look(), 0.0));
    let grounds: Vec<_> = got.iter().map(|r| r[0].1).collect();
    let g = Some(GROUND);
    assert_eq!(grounds, [None, None, g, g, None, None, None, None]);
    let fgs: Vec<_> = got.iter().map(|r| r[0].0.unwrap()).collect();
    let (out, inn) = (CODE[0], CODE[1]);
    assert_eq!(fgs, [out, out, inn, inn, out, out, out, out]);
    // A view smaller than a cell still shows.
    (d.top, d.view) = (105, 1);
    let rows = face(&d, 2, 8, &look(), 0.0);
    let grounds: Vec<_> = cells(&rows).iter().map(|r| r[0].1).collect();
    assert_eq!(grounds, [None, g, None, None, None, None, None, None]);
    // One run a row where the colours are the same.
    assert!(rows.iter().all(|r| r.len() == 1));
}

#[test]
fn test_face_cursor() {
    let d = Doc {
        lines: xs(32),
        top: 8,
        view: 8,
        width: 4,
        cursor: Some(9),
        ..Doc::default()
    };
    let g = Some(GROUND);
    let grounds = |l: &Look| -> Vec<_> {
        cells(&face(&d, 2, 8, l, 0.0))
            .iter()
            .map(|r| r[0].1)
            .collect()
    };
    assert_eq!(grounds(&look()), [None, None, g, g, None, None, None, None]);
    let l = Look {
        cursor: true,
        ..look()
    };
    assert_eq!(
        grounds(&l),
        [None, None, Some(CURSOR), g, None, None, None, None]
    );
}

#[test]
fn test_face_notes() {
    let mut d = doc(&["# a", "# b", "# c", "# d", "x = 1", "# e", "", ""], 4);
    let fgs = |d: &Doc, l: &Look| -> Vec<_> {
        (cells(&face(d, 2, 2, l, 0.0)).iter())
            .map(|r| r[0].0.unwrap())
            .collect()
    };
    assert_eq!(fgs(&d, &look()), [CODE[0], CODE[0]]);
    let l = Look {
        notes: true,
        ..look()
    };
    // A cell that holds code is of the colour of code.
    assert_eq!(fgs(&d, &l), [NOTE[0], CODE[0]]);
    d.view = 4;
    assert_eq!(fgs(&d, &l), [NOTE[1], CODE[0]]);
}

#[test]
fn test_face_marks() {
    let mut lines = xs(31);
    lines.push("      ERROR".into());
    let mut d = Doc {
        lines,
        view: 4,
        width: 12,
        ..Doc::default()
    };
    // The first pattern found: that of `screen.find` 1 (`ERR`, not `WARN`).
    d.marks.insert(31, 1);
    let rows = face(&d, 2, 2, &look(), 0.0); // four lines a dot row
    let c = cells(&rows);
    assert!(c[1].iter().all(|(fg, _)| *fg == Some(INKS[1])));
    assert!(c[0].iter().all(|(fg, _)| *fg == Some(CODE[1])));
    // The marked line is drawn alone: one in four, it still shows.
    assert_eq!(plain(&rows)[1], "⡇⣀");
    d.marks.clear();
    assert_eq!(plain(&face(&d, 2, 2, &look(), 0.0))[1], "⡇ ");
}

#[test]
fn test_face_heat() {
    let mut d = Doc {
        lines: xs(32),
        top: 28,
        view: 4,
        width: 4,
        ..Doc::default()
    };
    d.edited = HashMap::from([(0, 100.0), (4, 70.0)]);
    d.visited = HashMap::from([(4, 100.0), (8, 60.0), (12, 30.0)]);
    let l = Look {
        heat: true,
        fade: 60.0,
        ..look()
    };
    let fgs = |d: &Doc, l: &Look| -> Vec<u8> {
        (cells(&face(d, 2, 8, l, 100.0)).iter())
            .map(|r| r[0].0.unwrap())
            .collect()
    };
    assert!(fgs(&d, &look())[..7].iter().all(|&f| f == CODE[0]));
    let want = [
        EDITED[0], BOTH[1], // edited 30 s ago, visited now
        VISITED[2], CODE[0], // visited 70 s ago: cold
        CODE[0], CODE[0], CODE[0], CODE[1], // the view
    ];
    assert_eq!(fgs(&d, &l), want);
    d.forget(100.0, 60.0);
    assert_eq!(d.edited, HashMap::from([(0, 100.0), (4, 70.0)]));
    assert_eq!(d.visited, HashMap::from([(4, 100.0), (8, 60.0)]));
    d.visit(130.0);
    assert_eq!((d.visited[&28], d.visited[&31]), (130.0, 130.0));
    assert!(!d.visited.contains_key(&32));
}

#[test]
fn test_line_at() {
    let mut lines: Vec<String> = ["", "", "  a", "b"].iter().map(|s| s.to_string()).collect();
    lines.extend(vec!["c".to_string(); 60]);
    let mut d = Doc {
        y0: 50,
        lines,
        width: 4,
        ..Doc::default()
    };
    assert_eq!(line_at(&d, 2, 4, 0), Some(52), "16 lines a cell");
    assert_eq!(line_at(&d, 2, 4, 1), Some(66));
    for l in &mut d.lines[16..32] {
        l.clear();
    }
    assert_eq!(line_at(&d, 2, 4, 1), None);
    assert_eq!(line_at(&d, 2, 4, 9), None);
}

#[test]
fn test_title() {
    let n = |count| Doc {
        lines: vec![String::new(); count],
        ..Doc::default()
    };
    assert_eq!(title(&n(412)), "412");
    assert_eq!(title(&n(3449)), "3.4k");
}

#[test]
fn test_heights() {
    assert_eq!(heights(0), [4]);
    assert_eq!(heights(16), [4]);
    assert_eq!(heights(17), [4, 5]);
    assert_eq!(heights(50), [4, 6, 8, 12, 13]);
    assert_eq!(heights(115), [4, 6, 8, 12, 16, 24, 29]);
    assert_eq!(heights(1_000_000), HEIGHTS);
}

/// `test_print`'s refusals: the bounds of `--width` are the manifest's;
/// `--fade 0` refused in `init`; a bad `--mark` by `screen.find` (cases).
#[test]
fn test_options() {
    let o =
        Options::of(&json!({"all": true, "width": 8, "fade": 5.0, "mark": ["a", "b"]})).unwrap();
    assert!(o.look.cursor && o.look.heat && o.look.notes);
    assert_eq!((o.width, o.look.fade, o.marks.len()), (8, 5.0, 2));
    let o = Options::of(&json!({"notes": true})).unwrap();
    assert!(!o.look.cursor && !o.look.heat && o.look.notes);
    assert_eq!((o.width, o.look.fade), (12, 60.0));
    assert!(Options::of(&json!({"fade": 0.0})).is_err());
    assert!(Options::of(&json!({"fade": -1.0})).is_err());
}

#[test]
fn test_merge() {
    // Remembered 10..20, the view 15..20 shown: the view as shown.
    let v = View {
        origin: [0, 15],
        rect: [0, 0, 40, 5],
    };
    let known: Vec<String> = (10..20).map(|i| format!("old {i}")).collect();
    let shown: Vec<String> = (15..20).map(|i| format!("new {i}")).collect();
    let (y0, lines) = merge(10, known.clone(), &v, shown.clone());
    assert_eq!((y0, lines.len()), (10, 10));
    assert_eq!((lines[4].as_str(), lines[5].as_str()), ("old 14", "new 15"));
    // The view below what is remembered: blank between.
    let v2 = View {
        origin: [0, 25],
        ..v
    };
    let (y0, lines) = merge(10, known.clone(), &v2, shown.clone());
    assert_eq!(
        (y0, lines.len(), lines[12].as_str(), lines[15].as_str()),
        (10, 20, "", "new 15")
    );
    // Above it: from the view's top.
    let v3 = View {
        origin: [0, 2],
        ..v
    };
    let (y0, lines) = merge(10, known, &v3, shown);
    assert_eq!(
        (y0, lines.len(), lines[0].as_str(), lines[7].as_str()),
        (2, 18, "new 15", "")
    );
    assert_eq!(lines[8], "old 10");
    assert_eq!(v.to_frame(3, 2), Some((3, 17)));
    assert_eq!(v.to_frame(3, 5), None);
}

#[test]
fn test_tip() {
    assert_eq!(tip_text("  a\t b  "), "a b");
    let long = tip_text(&"w".repeat(60));
    assert_eq!((long.chars().count(), long.ends_with('…')), (48, true));
    // Left of the widget; no room there: right of it.
    assert_eq!(tip_at([60, 2, 14, 8], (12, 6), 0, "abc"), (54, 3));
    assert_eq!(tip_at([2, 2, 14, 8], (12, 6), 3, "abc"), (17, 6));
}

// ---- the twin's faces ----

fn fnv(text: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in text.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn canon(rows: &[Vec<Run>]) -> String {
    (rows.iter())
        .map(|row| {
            (row.iter())
                .map(|r| {
                    let bg = num(r.bg).map(|b| b.to_string()).unwrap_or_default();
                    format!("{}\x1f{}\x1f{bg}\x1e", r.t, num(r.fg).unwrap())
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn doc_of(v: &Value) -> Doc {
    let i = |v: &Value| v.as_i64().unwrap();
    let pairs = |v: &Value| -> Vec<(i64, f64)> {
        (v.as_array().unwrap().iter())
            .map(|p| (i(&p[0]), p[1].as_f64().unwrap()))
            .collect()
    };
    Doc {
        y0: i(&v["y0"]),
        lines: (v["lines"].as_array().unwrap().iter())
            .map(|l| l.as_str().unwrap().to_string())
            .collect(),
        top: i(&v["top"]),
        view: i(&v["view"]),
        width: i(&v["width"]),
        cursor: v["cursor"].as_i64(),
        marks: pairs(&v["marks"])
            .into_iter()
            .map(|(y, p)| (y, p as usize))
            .collect(),
        edited: pairs(&v["edited"]).into_iter().collect(),
        visited: pairs(&v["visited"]).into_iter().collect(),
    }
}

#[test]
fn test_twin() {
    let twin = ptyrc_app::json::parse(include_str!("twin.json")).unwrap();
    let sizes: Vec<(u32, u32)> = (twin["sizes"].as_array().unwrap().iter())
        .map(|s| (s[0].as_u64().unwrap() as u32, s[1].as_u64().unwrap() as u32))
        .collect();
    let mut docs = HashMap::new();
    for (name, t) in twin["docs"].as_object().unwrap() {
        let d = doc_of(&t["doc"]);
        assert_eq!(title(&d), t["title"].as_str().unwrap(), "{name}");
        let hs: Vec<u64> = heights(d.lines.len())
            .iter()
            .map(|&h| u64::from(h))
            .collect();
        let want: Vec<u64> = (t["heights"].as_array().unwrap().iter())
            .map(|h| h.as_u64().unwrap())
            .collect();
        assert_eq!(hs, want, "{name}");
        for (k, &(w, h)) in sizes.iter().enumerate() {
            let (c, r) = scale(d.lines.len(), d.width, w, h);
            assert_eq!(
                [c as u64, r as u64],
                [0, 1].map(|j| t["scale"][k][j].as_u64().unwrap())
            );
            for y in 0..h {
                let want = t["line_at"][k][y as usize].as_i64();
                assert_eq!(line_at(&d, w, h, y), want, "{name} {w}x{h} {y}");
            }
        }
        docs.insert(name.clone(), d);
    }
    let mut count = 0;
    for f in twin["faces"].as_array().unwrap() {
        let d = &docs[f["doc"].as_str().unwrap()];
        let l = &twin["looks"][f["look"].as_str().unwrap()];
        let l = Look {
            cursor: l[0] == true,
            heat: l[1] == true,
            notes: l[2] == true,
            fade: l[3].as_f64().unwrap(),
        };
        let (w, h) = (
            f["size"][0].as_u64().unwrap() as u32,
            f["size"][1].as_u64().unwrap() as u32,
        );
        let rows = face(d, w, h, &l, f["now"].as_f64().unwrap());
        let what = format!("{} {} {} {w}x{h}", f["doc"], f["look"], f["now"]);
        if let Some(want) = f["rows"].as_array() {
            let got: Vec<Vec<Value>> = (rows.iter())
                .map(|row| {
                    (row.iter())
                        .map(|r| json!([(r.t.as_str()), (num(r.fg)), (num(r.bg))]))
                        .collect()
                })
                .collect();
            assert_eq!(json!(got), json!(want.clone()), "{what}");
        }
        assert_eq!(fnv(&canon(&rows)), f["hash"].as_str().unwrap(), "{what}");
        count += 1;
    }
    assert!(count > 1000, "{count} faces");
}
