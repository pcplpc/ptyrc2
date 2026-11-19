// SPDX-License-Identifier: GPL-3.0-or-later
//! Drawn text reaches the host as glyphs only. Every string member of a
//! drawing request is fed hostile text; the bytes written to the host
//! must hold no control character, no escape sequence but the painter's
//! own, no hidden character, and leave the host's cells as the model
//! says (`Rig::check`).

use crate::screen_tests::Rig;
use ptyrc_vt::{VtEvent, drawn, is_hidden};
use serde_json::{Value, json};

/// Escapes, controls and hidden characters, each a way to lie or to
/// reach the host: clipboard, link, title, bell, DCS, C1, bidi.
const HOSTILE: &[&str] = &[
    "\x1b]52;c;SGVsbG8=\x07",
    "\x1b]52;c;SGVsbG8=\x1b\\",
    "\x1b]8;;https://example.invalid/\x1b\\L\x1b]8;;\x1b\\",
    "\x1b]0;pwned\x07",
    "\x1b]2;pwned\x1b\\",
    "\x07\x07",
    "\x1bP+q544e\x1b\\",
    "\x1b[2J\x1b[H\x1bc\x1b(0",
    "\x1b_apc\x1b\\\x1b^pm\x1b\\",
    "\u{9b}31m\u{9d}0;t\u{9c}\u{90}q\u{9c}\u{85}",
    "\r\n\x08\x0b\x0c\x0e\x0f\x7f\t\x00",
    "\u{202e}gpj.exe\u{202c}\u{2066}x\u{2069}\u{200f}",
    "a\u{200d}b\u{200b}c\u{2060}d\u{feff}e\u{ad}f",
    "\u{2764}\u{fe0f}\u{e0041}\u{e007f}\u{2028}\u{2029}",
];

/// The bytes written to the host hold nothing of `input` but glyphs.
fn assert_glyphs_only(out: &[u8], what: &str) {
    let mut i = 0;
    while i < out.len() {
        let b = out[i];
        match b {
            0x1b => {
                // The painter's own: CSI with digits and `;?`, or G0.
                match out.get(i + 1) {
                    Some(b'[') => {
                        let mut j = i + 2;
                        while j < out.len() && matches!(out[j], b'0'..=b'9' | b';' | b'?') {
                            j += 1;
                        }
                        assert!(
                            matches!(
                                out.get(j),
                                Some(b'H' | b'C' | b'G' | b'K' | b'X' | b'm' | b'h' | b'l')
                            ),
                            "{what}: CSI not the painter's at {i}: {:?}",
                            String::from_utf8_lossy(&out[i..out.len().min(i + 16)])
                        );
                        i = j + 1;
                        continue;
                    }
                    Some(b'(') => {
                        assert_eq!(out.get(i + 2), Some(&b'B'), "{what}: G0 at {i}");
                        i += 3;
                        continue;
                    }
                    other => panic!("{what}: ESC {other:?} at {i}"),
                }
            }
            0x0f => {}
            0x00..=0x1f | 0x7f => panic!("{what}: control {b:#04x} at {i}"),
            _ => {}
        }
        i += 1;
    }
    let text = std::str::from_utf8(out).unwrap_or_else(|e| panic!("{what}: not UTF-8: {e}"));
    for c in text.chars() {
        assert!(
            !('\u{80}'..='\u{9f}').contains(&c),
            "{what}: C1 {c:?} written"
        );
        assert!(!is_hidden(c), "{what}: hidden {c:?} written");
    }
}

/// The host after the paint: no title, no bell, no link, no control or
/// hidden character in a cell.
fn assert_host_clean(rig: &mut Rig, what: &str) {
    assert_eq!(rig.host.title(), "", "{what}: title set");
    for event in rig.host.take_events() {
        assert!(
            !matches!(event, VtEvent::Bell | VtEvent::Title),
            "{what}: {event:?}"
        );
    }
    let grid = rig.host.grid();
    for y in 0..grid.rows() {
        for cell in grid.row(y) {
            assert_eq!(cell.style.link, 0, "{what}: a link at row {y}");
            assert!(
                !cell.text.chars().any(|c| c.is_control() || is_hidden(c)),
                "{what}: cell {:?} at row {y}",
                cell.text
            );
        }
    }
}

/// Run `calls` on a fresh rig, then let it paint; the writes are checked.
/// An `"id": "$"` is the id the last call returned.
fn audit(what: &str, calls: &[(&str, Value)]) {
    let mut rig = Rig::new(60, 16);
    rig.burst(b"prompt $ ");
    let mut out = Vec::new();
    let mut last = Value::Null;
    for (method, params) in calls {
        let mut params = params.clone();
        if params["id"] == "$" {
            params["id"] = last.clone();
        }
        let r = rig.call(method, params);
        last = r
            .get("id")
            .or(r.pointer("/widget/id"))
            .cloned()
            .unwrap_or(last);
        out.extend(rig.tick());
    }
    out.extend(rig.advance(3000));
    assert!(!out.is_empty(), "{what}: nothing painted");
    assert_glyphs_only(&out, what);
    assert_host_clean(&mut rig, what);
}

/// A row of `content.grid`: a cell for each scalar of `text`, then
/// values that are no scalar; every other cell with a `bg`.
fn grid_row(text: &str) -> String {
    let mut bytes = Vec::new();
    let values = text
        .chars()
        .map(u32::from)
        .chain([0xd800, 0x11_0000, u32::MAX]);
    for (i, v) in values.enumerate() {
        bytes.extend(v.to_le_bytes());
        bytes.extend([3, 255, 0, 0]);
        bytes.extend(if i % 2 == 0 { [0; 4] } else { [2, 4, 0, 0] });
        bytes.extend([0x7f, 0, 0, 0]);
    }
    let mut s = String::new();
    ptyrc_core::scene::grid::encode(&bytes, &mut s);
    s
}

fn sprite(content: Value) -> Value {
    json!({"anchor": {"at": [2, 2], "space": "view"}, "content": content, "layer": "front"})
}

#[test]
fn sprite_text_is_glyphs_only() {
    for (n, bad) in HOSTILE.iter().enumerate() {
        let text = format!("<{bad}>");
        audit(
            &format!("{n}: rows"),
            &[("draw.put", sprite(json!({"rows": [text, bad]})))],
        );
        audit(
            &format!("{n}: cells"),
            &[(
                "draw.put",
                sprite(json!({"cells": [[{"t": text, "fg": "red"}, {"t": bad, "bg": "blue"}]]})),
            )],
        );
        audit(
            &format!("{n}: grid"),
            &[(
                "draw.put",
                sprite(json!({"grid": [grid_row(&text), grid_row(bad)]})),
            )],
        );
        audit(
            &format!("{n}: grid rows"),
            &[
                (
                    "draw.put",
                    sprite(
                        json!({"grid": [grid_row(&"plain".repeat(10)), "", ""], "opaque": true}),
                    ),
                ),
                (
                    "draw.update",
                    json!({"id": "$", "rows": {"0": grid_row(bad), "2": grid_row(&text)}}),
                ),
            ],
        );
        audit(
            &format!("{n}: opaque"),
            &[("draw.put", sprite(json!({"rows": [text], "opaque": true})))],
        );
        audit(
            &format!("{n}: frames"),
            &[(
                "draw.put",
                json!({"anchor": {"at": [0, 1], "space": "view"}, "layer": "front",
                       "frames": [{"rows": [text], "ms": 50}, {"rows": [bad], "ms": 50}]}),
            )],
        );
        audit(
            &format!("{n}: update"),
            &[
                ("draw.put", sprite(json!({"rows": ["plain"]}))),
                (
                    "draw.update",
                    json!({"id": "$", "content": {"rows": [text]}}),
                ),
            ],
        );
    }
}

#[test]
fn widget_text_is_glyphs_only() {
    for (n, bad) in HOSTILE.iter().enumerate() {
        let text = format!("<{bad}>");
        let declare = json!({"widgets": [
            {"tag": format!("w{bad}"), "size": [14, 2], "content": {"rows": [text, bad]}},
        ]});
        let tag = format!("w{bad}");
        audit(
            &format!("{n}: widget content, tag as title"),
            &[
                ("widget.declare", declare.clone()),
                ("widget.show", json!({"tag": tag})),
            ],
        );
        audit(
            &format!("{n}: widget title, border title"),
            &[
                ("widget.declare", declare.clone()),
                (
                    "widget.show",
                    json!({"tag": tag, "title": text, "modal": true,
                           "border": {"title": {"before": bad, "after": bad}}}),
                ),
            ],
        );
        audit(
            &format!("{n}: widget.set"),
            &[
                ("widget.declare", declare.clone()),
                ("widget.show", json!({"tag": tag})),
                (
                    "widget.set",
                    json!({"id": "$", "variant": "14x2", "title": bad,
                           "content": {"cells": [[{"t": text}, {"t": bad, "fg": "red"}]]}}),
                ),
            ],
        );
    }
}

/// Members that name or draw a frame refuse what is not a glyph.
#[test]
fn glyph_members_refuse_controls() {
    for bad in HOSTILE {
        let mut rig = Rig::new(60, 16);
        rig.call(
            "widget.declare",
            json!({"widgets": [{"tag": "w", "size": [6, 1], "content": {"rows": ["x"]}}]}),
        );
        let first = bad.chars().find(|c| drawn(*c) != Some(*c)).unwrap();
        let corners = format!("{first}+++");
        rig.fail(
            7,
            "widget.show",
            json!({"tag": "w", "border": {"corners": corners}}),
        );
        rig.fail(
            7,
            "widget.show",
            json!({"tag": "w", "border": {"top": bad}}),
        );
        rig.fail(
            7,
            "layer.define",
            json!({"name": format!("l{bad}"), "z": 5}),
        );
    }
}
