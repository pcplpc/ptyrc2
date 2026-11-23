// SPDX-License-Identifier: GPL-3.0-or-later
//! The answers cut to a degree. See `plan/design.md`, "Permissions",
//! and `plan/store-model.md`, "A no lowers, it does not break". Pure:
//! what to ask the engine in place of the request, and what to make of
//! its answer.

use crate::grants::{Degree, Hand};
use ptyrc_core::FrameId;
use ptyrc_core::RAW;
use ptyrc_proto::{ErrorCode, RpcError};
use ptyrc_vt::char_width;
use serde_json::{Map, Value, json};

/// What becomes of a request of an app that holds `degree`, below
/// `text`.
#[derive(Clone, Debug, PartialEq)]
pub enum Plan {
    /// As asked: nothing of it is above the degree.
    Pass,
    /// Ask the engine this, then cut its answer.
    Ask(&'static str, Value, Cut),
    /// Answered here; the engine is not asked.
    Answer(Value),
}

/// What is made of the engine's answer.
#[derive(Clone, Debug, PartialEq)]
pub enum Cut {
    /// Kept whole: the request was cut.
    Mark,
    /// `lines` of `screen.shape`, each emptied when `blank`.
    Lines { blank: bool },
    /// `lines` of `screen.shape` as the `rows` of `screen.cells`: one
    /// run a row, no style.
    Runs { blank: bool },
    /// `lines` of `frame.text`, as their shape; these words shown.
    Mask(Vec<String>),
    /// `frame.heat`: all cold.
    Cold,
    /// `{"frame": info}`: the info cut.
    Frame,
    /// `{"frames": [...]}`: each info cut.
    Frames,
    /// `{"frame": info}` of the surface, as the answer of `frame.list`.
    Surface,
    /// `session.info`: the size and what tells nothing.
    Info,
}

/// The plan of `method` for an app that holds `degree` and the words
/// `held` (empty below `words`), the user having given `hand`. `Pass`
/// for what the degree opens whole and for what the degrees do not
/// rule. `surface`: the frame read below `layout`, whatever was named
/// (`surface::Surface::read`).
pub fn plan(
    method: &str,
    params: &Value,
    degree: Degree,
    held: &[String],
    hand: &Hand,
    surface: FrameId,
) -> Plan {
    if degree >= Degree::Text {
        return Plan::Pass;
    }
    let has = |member: &str| params.get(member).is_some_and(|v| !v.is_null());
    // The user's pattern, exactly: answered as under `text`.
    if let ("screen.find" | "screen.wait", Some(pattern)) =
        (method, params.get("pattern").and_then(Value::as_str))
    {
        let regex = params.get("regex") == Some(&Value::Bool(true));
        if hand
            .patterns
            .iter()
            .any(|(p, r)| p == pattern && *r == regex)
        {
            let mut out = params.as_object().cloned().unwrap_or_default();
            if degree < Degree::Layout {
                out.remove("frame");
                if surface != RAW {
                    out.insert("frame".into(), json!(surface));
                }
            }
            let method = match method {
                "screen.find" => "screen.find",
                _ => "screen.wait",
            };
            return Plan::Ask(method, Value::Object(out), Cut::Mark);
        }
    }
    let shape = degree >= Degree::Shape;
    let found = found(degree, held, hand);
    let shown = shown(degree, held, hand);
    let held: &[String] = &shown;
    // Below `layout` there is one frame: the surface.
    let frame = |out: &mut Map<String, Value>| {
        if degree >= Degree::Layout && has("frame") {
            out.insert("frame".into(), params["frame"].clone());
        } else if degree < Degree::Layout && surface != RAW {
            out.insert("frame".into(), json!(surface));
        }
    };
    let view = |words: Vec<String>| {
        let mut out = Map::new();
        frame(&mut out);
        if has("rect") {
            out.insert("rect".into(), params["rect"].clone());
        }
        if !words.is_empty() {
            out.insert("words".into(), json!(words));
        }
        Value::Object(out)
    };
    let only_frame = || {
        let mut out = Map::new();
        frame(&mut out);
        Value::Object(out)
    };
    match method {
        "screen.text" => Plan::Ask(
            "screen.shape",
            view(held.to_vec()),
            Cut::Lines { blank: !shape },
        ),
        "screen.cells" => Plan::Ask(
            "screen.shape",
            view(held.to_vec()),
            Cut::Runs { blank: !shape },
        ),
        "screen.shape" => {
            let asked = params.get("words").and_then(Value::as_array);
            let asked: Vec<&str> = asked
                .map(|words| words.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let kept: Vec<String> = asked
                .iter()
                .filter(|word| holds(held, word))
                .map(|word| word.to_string())
                .collect();
            // The user's words below `words`: asked again, not passed,
            // since the gate lowers a shape with words there.
            if shape && kept.len() == asked.len() && (asked.is_empty() || degree >= Degree::Words) {
                return Plan::Pass;
            }
            Plan::Ask("screen.shape", view(kept), Cut::Lines { blank: !shape })
        }
        "frame.text" if !shape => Plan::Answer(json!({"lines": [], "y0": 0})),
        "frame.text" => {
            let mut out = Map::new();
            frame(&mut out);
            for member in ["y0", "y1"] {
                if has(member) {
                    out.insert(member.into(), params[member].clone());
                }
            }
            Plan::Ask("frame.text", Value::Object(out), Cut::Mask(held.to_vec()))
        }
        "screen.cursor" if !shape => {
            let mut out = json!({"x": 0, "y": 0, "visible": false, "frame": null, "pos": null});
            if params.get("trail") == Some(&Value::Bool(true)) {
                out["trail"] = json!([]);
            }
            Plan::Answer(out)
        }
        "frame.heat" if !shape => Plan::Ask("frame.heat", only_frame(), Cut::Cold),
        "frame.idle" if !shape => Plan::Answer(still()),
        "screen.find" => match word_query(params, &found) {
            Some(query) => {
                let mut out = query;
                frame(&mut out);
                if has("known") {
                    out.insert("known".into(), params["known"].clone());
                }
                Plan::Ask("screen.find", Value::Object(out), Cut::Mark)
            }
            None => Plan::Answer(json!({"matches": []})),
        },
        "screen.wait" if has("pattern") => match word_query(params, &found) {
            Some(query) => {
                let mut out = query;
                frame(&mut out);
                for member in ["gone", "timeout_ms"] {
                    if has(member) {
                        out.insert(member.into(), params[member].clone());
                    }
                }
                Plan::Ask("screen.wait", Value::Object(out), Cut::Mark)
            }
            // No match: gone at once, never there.
            None => {
                let gone = params.get("gone") == Some(&Value::Bool(true));
                Plan::Answer(json!({"matched": gone, "match": null}))
            }
        },
        // A blank screen is still and idle.
        "screen.wait" if !shape => {
            let mut out = json!({"matched": true, "match": null});
            if params.get("idle") == Some(&Value::Bool(true)) {
                out["frame"] = json!(RAW);
                out["idle"] = still();
            }
            Plan::Answer(out)
        }
        "frame.list" if degree < Degree::Layout => {
            Plan::Ask("frame.get", only_frame(), Cut::Surface)
        }
        "frame.list" => Plan::Ask("frame.list", json!({}), Cut::Frames),
        "frame.get" => Plan::Ask("frame.get", only_frame(), Cut::Frame),
        "frame.at" if degree < Degree::Layout => {
            let mut out = json!({"frame": RAW});
            if let (Some(x), Some(y)) = (params.get("x"), params.get("y"))
                && x.is_u64()
                && y.is_u64()
            {
                out["pos"] = json!([x, y]);
            }
            Plan::Answer(out)
        }
        "session.info" => Plan::Ask("session.info", json!({}), Cut::Info),
        _ => Plan::Pass,
    }
}

/// The words found by `screen.find`, `screen.wait`: those of `words`
/// (`held`) from `words`; the user's at any degree.
pub fn found(degree: Degree, held: &[String], hand: &Hand) -> Vec<String> {
    let mut found: Vec<String> = match degree >= Degree::Words {
        true => held.to_vec(),
        false => Vec::new(),
    };
    found.extend(hand.words.iter().cloned());
    found
}

/// The words shown in a shape: those found, from `shape`.
pub fn shown(degree: Degree, held: &[String], hand: &Hand) -> Vec<String> {
    match degree >= Degree::Shape {
        true => found(degree, held, hand),
        false => Vec::new(),
    }
}

/// Whether `word` is among `words`, case ignored.
pub fn holds(words: &[String], word: &str) -> bool {
    let word = word.to_lowercase();
    words.iter().any(|h| h.to_lowercase() == word)
}

/// The frame of a sprite placed on a frame gone: shown nowhere.
pub const NOWHERE: FrameId = FrameId::MAX;

/// Whether `method` composes the screen with the sprites (`sprites:
/// true`): of an app that sees its own, its own sprites alone.
pub fn composes_own(method: &str) -> bool {
    matches!(method, "screen.text" | "screen.cells")
}

/// Of an app's own sprite in `draw.list`: where an anchor placed by
/// what the app may not read landed, told at the degree that would
/// have told it; hidden below it, as a sprite not placed yet. `cut`:
/// its degree, its words, the user's; `None`: all told. `surface`: of
/// a store app, the frame told in place of `NOWHERE`, and of every
/// frame below `layout`: 0. The degree it needed, when hidden.
pub fn landing(
    item: &mut Value,
    cut: Option<(Degree, &[String], &Hand)>,
    surface: Option<FrameId>,
) -> Option<Degree> {
    let nowhere = item["frame"] == json!(NOWHERE);
    let Some((degree, words, hand)) = cut else {
        if let Some(surface) = surface.filter(|_| nowhere) {
            item["frame"] = json!(surface);
        }
        return None;
    };
    let anchor = &item["anchor"];
    let among = |list: &[String], text: &str| list.iter().any(|w| w.eq_ignore_ascii_case(text));
    let needs = if let Some(text) = anchor.get("text").and_then(Value::as_str) {
        let literal = anchor.get("regex") != Some(&Value::Bool(true));
        match () {
            _ if literal && among(words, text) => Degree::Words,
            _ if literal && among(&hand.words, text) => Degree::Shape,
            _ => Degree::Text,
        }
    } else if anchor.get("cursor").is_some() || anchor.get("auto").is_some() {
        Degree::Shape
    } else {
        Degree::None
    };
    if let Some(surface) = surface {
        if degree < Degree::Layout {
            item["frame"] = json!(0);
        } else if nowhere {
            item["frame"] = json!(surface);
        }
    }
    if degree >= needs {
        return None;
    }
    item["screen"] = Value::Null;
    item["visible"] = json!(false);
    item["covered"] = json!(0.0);
    Some(needs)
}

/// `frame.idle` of a blank screen.
fn still() -> Value {
    json!({"score": 1.0, "idle": true, "free": 1.0})
}

/// The query of a held word, whole and literal, case ignored. None:
/// the pattern is not one of the words held.
fn word_query(params: &Value, held: &[String]) -> Option<Map<String, Value>> {
    let pattern = params.get("pattern")?.as_str()?;
    if params.get("regex") == Some(&Value::Bool(true)) || !holds(held, pattern) {
        return None;
    }
    // Letters and digits only (`Words`): nothing to escape.
    if !pattern.chars().all(char::is_alphanumeric) {
        return None;
    }
    let mut out = Map::new();
    out.insert("pattern".into(), json!(format!(r"\b{pattern}\b")));
    out.insert("regex".into(), json!(true));
    out.insert("ignore_case".into(), json!(true));
    Some(out)
}

/// The shape of a line of text: a blank is a space, ink `#` (a wide
/// glyph `##`), a whole word of `words` as it is.
pub fn mask(line: &str, words: &[String]) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_alphanumeric() {
            let end = (i..chars.len())
                .find(|&j| !chars[j].is_alphanumeric())
                .unwrap_or(chars.len());
            let word: String = chars[i..end].iter().collect();
            let narrow = chars[i..end].iter().all(|&c| char_width(c) == 1);
            if narrow && holds(words, &word) {
                out.push_str(&word);
            } else {
                chars[i..end].iter().for_each(|&c| ink(c, &mut out));
            }
            i = end;
        } else {
            ink(c, &mut out);
            i += 1;
        }
    }
    out.truncate(out.trim_end_matches(' ').len());
    out
}

fn ink(c: char, out: &mut String) {
    match (c.is_whitespace(), char_width(c)) {
        (true, _) => out.push(' '),
        (false, 2) => out.push_str("##"),
        (false, 0) => {}
        (false, _) => out.push('#'),
    }
}

/// A frame info for an app below `text`: an id and a rectangle. No
/// key, no support; below `shape`, idle, nothing remembered, never
/// scrolled (`origin` at 0, 0).
pub fn frame_info(info: &mut Value, degree: Degree) {
    let Some(object) = info.as_object_mut() else {
        return;
    };
    for member in ["key", "support"] {
        if object.contains_key(member) {
            object.insert(member.into(), Value::Null);
        }
    }
    if degree < Degree::Shape {
        object.insert("idle".into(), json!(1.0));
        object.insert("is_idle".into(), json!(true));
        let rows = object.get("rect").and_then(|r| r.get(3)).cloned();
        if object.contains_key("origin") {
            object.insert("origin".into(), json!([0, 0]));
        }
        if let Some(rows) = rows.as_ref().and_then(Value::as_i64) {
            object.insert("extent".into(), json!([0, (rows - 1).max(0)]));
        }
    }
}

/// Below `layout`, an answer read on the surface, its view at `at` on
/// the screen, told as of frame 0 at the top-left: a frame info's `id`
/// 0, its `rect` at 0, 0; a match's `frame` 0, a `frame` beside `x`,
/// `y` 0; each `screen`, `rect`, those `x`, `y` less `at`.
pub fn on_surface(answer: &mut Value, at: [u16; 2]) {
    let less = |cell: Option<&mut Value>| {
        let Some(cell) = cell else {
            return;
        };
        for (i, by) in at.iter().enumerate() {
            if let Some(v) = cell.get(i).and_then(Value::as_u64) {
                cell[i] = json!(v.saturating_sub(u64::from(*by)));
            }
        }
    };
    let Some(object) = answer.as_object_mut() else {
        return;
    };
    // Each object of `one`, and of the list `many`.
    let mut each = |one: &str, many: &str, f: &mut dyn FnMut(&mut Map<String, Value>)| {
        if let Some(item) = object.get_mut(one).and_then(Value::as_object_mut) {
            f(item);
        }
        let items = object.get_mut(many).and_then(Value::as_array_mut);
        for item in (items.into_iter().flatten()).filter_map(Value::as_object_mut) {
            f(item);
        }
    };
    each("frame", "frames", &mut |info| {
        info.insert("id".into(), json!(0));
        less(info.get_mut("rect"));
    });
    each("match", "matches", &mut |found| {
        found.insert("frame".into(), json!(0));
        less(found.get_mut("screen"));
    });
    each("", "sprites", &mut |sprite| less(sprite.get_mut("screen")));
    less(object.get_mut("rect"));
    // A screen cell (`screen.cursor`, `mouse`, `hover`), its frame's.
    if object.contains_key("x") && object.contains_key("y") {
        for (member, by) in [("x", at[0]), ("y", at[1])] {
            if let Some(v) = object.get(member).and_then(Value::as_u64) {
                object.insert(member.into(), json!(v.saturating_sub(u64::from(by))));
            }
        }
        if object.get("frame").is_some_and(Value::is_u64) {
            object.insert("frame".into(), json!(0));
        }
        // `pos`: the cell of frame 0, that is `x`, `y`.
        if object.get("pos").is_some_and(Value::is_array) {
            let pos = json!([object["x"], object["y"]]);
            object.insert("pos".into(), pos);
        }
    }
}

/// Below `layout`, a widget of `surface = "widget"` is told in its own
/// cells, wherever it is placed: of a widget info, `rect` at 0, 0.
pub fn own_widget(info: &mut Value) {
    if let Some(rect) = info.get_mut("rect").and_then(Value::as_array_mut)
        && rect.len() == 4
    {
        rect[0] = json!(0);
        rect[1] = json!(0);
    }
}

/// Below `layout`, a pointer report to an app of `surface = "widget"`,
/// in the cells of one of its widgets, whose outer box has its corner
/// at `corner` on the screen: `x`, `y` less `corner` (negative left of
/// it or above), `pos` the same, `frame` 0.
pub fn on_widget(report: &mut Value, corner: [i64; 2]) {
    let Some(object) = report.as_object_mut() else {
        return;
    };
    for (member, by) in [("x", corner[0]), ("y", corner[1])] {
        if let Some(v) = object.get(member).and_then(Value::as_i64) {
            object.insert(member.into(), json!(v - by));
        }
    }
    if object.contains_key("pos") {
        let pos = json!([object["x"], object["y"]]);
        object.insert("pos".into(), pos);
    }
    if object.contains_key("frame") {
        object.insert("frame".into(), json!(0));
    }
}

/// Below `layout`, every `hover` and `mouse` report to an app of
/// `surface = "widget"`, on its widget or elsewhere, by
/// [`on_widget`]. `placed`: the sprite and the screen corner of each
/// of its widgets placed, in the order of `widget.list`. `own`: the
/// report names a widget sprite of its own (a release or a drag off it
/// included): that widget's corner; else (another's sprite, its own
/// behind the text, a leave) the first's. False: that widget, or every
/// one, not placed; not told.
pub fn in_widget(report: &mut Value, own: bool, placed: &[(u64, [i64; 2])]) -> bool {
    let corner = match own {
        true => {
            let sprite = report["sprite"].as_u64();
            (placed.iter()).find(|(s, _)| Some(*s) == sprite)
        }
        false => placed.first(),
    };
    let Some(&(_, corner)) = corner else {
        return false;
    };
    on_widget(report, corner);
    true
}

/// Below `shape`, a frame is told unscrolled: its cells are those of
/// the view. Of a request's `anchor` in frame cells (`at`, `space`
/// `frame` or absent): put in view cells; a negative coordinate, off
/// the view, in frame cells from `origin`, that of its frame.
pub fn in_view(params: &mut Value, origin: [i64; 2]) {
    let Some(anchor) = params.get_mut("anchor").and_then(Value::as_object_mut) else {
        return;
    };
    let space = anchor.get("space").filter(|s| !s.is_null());
    if space.is_some_and(|s| s != "frame") {
        return;
    }
    let at = anchor.get("at").and_then(Value::as_array);
    let Some([x, y]) = at.and_then(|at| <[Value; 2]>::try_from(at.clone()).ok()) else {
        return;
    };
    let (Some(x), Some(y)) = (x.as_i64(), y.as_i64()) else {
        return;
    };
    if x >= 0 && y >= 0 {
        anchor.insert("space".into(), json!("view"));
    } else {
        anchor.insert("at".into(), json!([x + origin[0], y + origin[1]]));
        anchor.insert("space".into(), json!("frame"));
    }
}

/// Below `shape`, an answer or a pointer report told unscrolled: each
/// cell of a frame (a match's `x`, `y`; `pos` beside `frame`) less the
/// `origin` of that frame (`origin_of`), a cell of its view.
pub fn unscrolled(answer: &mut Value, origin_of: &dyn Fn(u64) -> Option<[i64; 2]>) {
    let Some(object) = answer.as_object_mut() else {
        return;
    };
    let less = |cell: &mut Value, at: [i64; 2]| {
        for (i, by) in at.iter().enumerate() {
            if let Some(v) = cell.get(i).and_then(Value::as_i64) {
                cell[i] = json!(v - by);
            }
        }
    };
    if let Some(origin) = object
        .get("frame")
        .and_then(Value::as_u64)
        .and_then(origin_of)
        && let Some(pos) = object.get_mut("pos").filter(|p| p.is_array())
    {
        less(pos, origin);
    }
    let mut found: Vec<&mut Value> = Vec::new();
    for (member, item) in object.iter_mut() {
        match (member.as_str(), item) {
            ("match", item) => found.push(item),
            ("matches", Value::Array(items)) => found.extend(items.iter_mut()),
            _ => {}
        }
    }
    for found in found.into_iter().filter_map(Value::as_object_mut) {
        let Some(origin) = found
            .get("frame")
            .and_then(Value::as_u64)
            .and_then(origin_of)
        else {
            continue;
        };
        for (member, by) in [("x", origin[0]), ("y", origin[1])] {
            if let Some(v) = found.get(member).and_then(Value::as_i64) {
                found.insert(member.into(), json!(v - by));
            }
        }
    }
}

/// The data of an event of `topic` for an app that holds `degree`.
pub fn event(topic: &str, mut data: Value, degree: Degree) -> Value {
    if topic == "frames" && degree < Degree::Text {
        frames(&mut data, degree);
        mark(&mut data, degree);
    }
    data
}

fn frames(answer: &mut Value, degree: Degree) {
    if let Some(Value::Array(infos)) = answer.get_mut("frames") {
        infos.iter_mut().for_each(|info| frame_info(info, degree));
    }
}

fn mark(answer: &mut Value, degree: Degree) {
    if let Some(object) = answer.as_object_mut() {
        object.insert("degree".into(), json!(degree.name()));
    }
}

fn lines_of(answer: &Value) -> Vec<&str> {
    let lines = answer.get("lines").and_then(Value::as_array);
    lines
        .map(|lines| lines.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

impl Cut {
    /// The engine's answer, cut and marked. Built from the members
    /// known, never by removing from the whole: a member added to the
    /// engine's answer later is not told by default.
    pub fn apply(&self, answer: Value, degree: Degree) -> Value {
        let keep = |members: &[&str]| {
            let mut out = Map::new();
            for member in members {
                if let Some(value) = answer.get(*member) {
                    out.insert((*member).into(), value.clone());
                }
            }
            out
        };
        let mut out = match self {
            Cut::Mark => answer.clone(),
            Cut::Lines { blank } => {
                let mut out = keep(&["size", "rect"]);
                let lines: Vec<&str> = lines_of(&answer)
                    .into_iter()
                    .map(|line| if *blank { "" } else { line })
                    .collect();
                out.insert("lines".into(), json!(lines));
                Value::Object(out)
            }
            Cut::Runs { blank } => {
                let mut out = keep(&["size", "rect"]);
                let rows: Vec<Value> = lines_of(&answer)
                    .into_iter()
                    .map(|line| match *blank || line.is_empty() {
                        true => json!([]),
                        false => json!([{"t": line}]),
                    })
                    .collect();
                out.insert("rows".into(), json!(rows));
                Value::Object(out)
            }
            Cut::Mask(words) => {
                let mut out = keep(&["y0"]);
                let lines: Vec<String> = lines_of(&answer)
                    .into_iter()
                    .map(|line| mask(line, words))
                    .collect();
                out.insert("lines".into(), json!(lines));
                Value::Object(out)
            }
            Cut::Cold => {
                let fill = |member: &str, with: &str| -> Vec<String> {
                    let rows = answer.get(member).and_then(Value::as_array);
                    rows.map(|rows| {
                        rows.iter()
                            .filter_map(Value::as_str)
                            .map(|row| with.repeat(row.chars().count()))
                            .collect()
                    })
                    .unwrap_or_default()
                };
                json!({"rows": fill("rows", "9"), "classes": fill("classes", ".")})
            }
            Cut::Frame => {
                let mut info = answer.get("frame").cloned().unwrap_or(Value::Null);
                frame_info(&mut info, degree);
                json!({"frame": info})
            }
            Cut::Frames => {
                let mut out = Value::Object(keep(&["frames", "layout"]));
                frames(&mut out, degree);
                out
            }
            Cut::Surface => {
                let mut info = answer.get("frame").cloned().unwrap_or(Value::Null);
                frame_info(&mut info, degree);
                json!({"frames": [info], "layout": 0})
            }
            Cut::Info => {
                let mut members = vec!["protocol", "server", "size", "headless"];
                if degree >= Degree::Shape {
                    members.push("host_colors");
                }
                Value::Object(keep(&members))
            }
        };
        mark(&mut out, degree);
        out
    }
}

/// An error for an app below `text`: a hint that lists frames or
/// sprites would tell what the app may not read.
pub fn scrub(mut error: RpcError) -> RpcError {
    if scrubbed(error.code.as_str()) {
        error.hint = None;
    }
    error
}

/// Whether the hint of an error of `code` is dropped (`scrub`).
pub fn scrubbed(code: &str) -> bool {
    [ErrorCode::NoFrame, ErrorCode::NoSprite]
        .iter()
        .any(|c| c.as_str() == code)
}

/// The `hello` of a connection of an app that holds `degree`.
pub fn hello(hello: Value, degree: Degree) -> Value {
    match degree >= Degree::Text {
        true => hello,
        false => Cut::Info.apply(hello, degree),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Degree::{Layout, None as Nothing, Shape, Text, Words};

    fn words() -> Vec<String> {
        vec!["fish".into(), "food".into()]
    }

    fn ask(method: &str, params: Value, degree: Degree) -> Plan {
        plan(method, &params, degree, &words(), &Hand::default(), RAW)
    }

    #[test]
    fn text_passes_everything() {
        for method in ["screen.text", "screen.find", "frame.list", "session.info"] {
            let params = json!({"pattern": "x"});
            assert_eq!(ask(method, params, Text), Plan::Pass);
        }
    }

    #[test]
    fn text_and_cells_are_the_shape() {
        let params = json!({"frame": 2, "rect": [0, 0, 4, 1], "ansi": true, "sprites": true});
        let shape = json!({"frame": 2, "rect": [0, 0, 4, 1]});
        assert_eq!(
            ask("screen.text", params.clone(), Shape),
            Plan::Ask("screen.shape", shape.clone(), Cut::Lines { blank: false })
        );
        let with = json!({"frame": 2, "rect": [0, 0, 4, 1], "words": ["fish", "food"]});
        assert_eq!(
            ask("screen.cells", params.clone(), Words),
            Plan::Ask("screen.shape", with, Cut::Runs { blank: false })
        );
        // Below `shape`: blank; below `layout`: the surface.
        assert_eq!(
            ask("screen.text", params.clone(), Layout),
            Plan::Ask("screen.shape", shape, Cut::Lines { blank: true })
        );
        assert_eq!(
            ask("screen.text", params, Nothing),
            Plan::Ask(
                "screen.shape",
                json!({"rect": [0, 0, 4, 1]}),
                Cut::Lines { blank: true }
            )
        );
    }

    #[test]
    fn the_cuts_of_the_view() {
        let answer = json!({"lines": ["## fish", ""], "size": [9, 2], "rect": [0, 0, 9, 2],
                            "more": "x"});
        let lines = Cut::Lines { blank: false }.apply(answer.clone(), Words);
        assert_eq!(
            lines,
            json!({"lines": ["## fish", ""], "size": [9, 2], "rect": [0, 0, 9, 2],
                   "degree": "words"})
        );
        let blank = Cut::Lines { blank: true }.apply(answer.clone(), Nothing);
        assert_eq!(blank["lines"], json!(["", ""]));
        assert_eq!(blank["degree"], "none");
        let runs = Cut::Runs { blank: false }.apply(answer.clone(), Shape);
        assert_eq!(runs["rows"], json!([[{"t": "## fish"}], []]));
        assert_eq!(runs["size"], json!([9, 2]));
        let none = Cut::Runs { blank: true }.apply(answer, Layout);
        assert_eq!(none["rows"], json!([[], []]));
    }

    #[test]
    fn the_shape_shows_held_words_alone() {
        let params = json!({"words": ["Fish", "toy"]});
        assert_eq!(
            ask("screen.shape", params.clone(), Words),
            Plan::Ask(
                "screen.shape",
                json!({"words": ["Fish"]}),
                Cut::Lines { blank: false }
            )
        );
        assert_eq!(
            ask("screen.shape", params.clone(), Shape),
            Plan::Ask("screen.shape", json!({}), Cut::Lines { blank: false })
        );
        assert_eq!(
            ask("screen.shape", params, Nothing),
            Plan::Ask("screen.shape", json!({}), Cut::Lines { blank: true })
        );
        assert_eq!(
            ask("screen.shape", json!({"words": ["food"]}), Words),
            Plan::Pass
        );
        assert_eq!(ask("screen.shape", json!({}), Shape), Plan::Pass);
    }

    #[test]
    fn below_shape_the_screen_is_blank_cold_and_still() {
        for degree in [Nothing, Layout] {
            assert_eq!(
                ask("screen.cursor", json!({"trail": true}), degree),
                Plan::Answer(json!({"x": 0, "y": 0, "visible": false, "frame": null,
                                    "pos": null, "trail": []}))
            );
            assert_eq!(ask("frame.idle", json!({}), degree), Plan::Answer(still()));
            assert_eq!(
                ask("frame.text", json!({}), degree),
                Plan::Answer(json!({"lines": [], "y0": 0}))
            );
            assert_eq!(
                ask("screen.wait", json!({"stable_ms": 500}), degree),
                Plan::Answer(json!({"matched": true, "match": null}))
            );
            let idle = ask(
                "screen.wait",
                json!({"idle": true, "frame": "largest"}),
                degree,
            );
            let Plan::Answer(idle) = idle else {
                panic!("{idle:?}")
            };
            assert_eq!(idle["frame"], 0);
            assert_eq!(idle["idle"]["idle"], true);
        }
        assert_eq!(
            ask("frame.heat", json!({"frame": 3}), Layout),
            Plan::Ask("frame.heat", json!({"frame": 3}), Cut::Cold)
        );
        assert_eq!(
            ask("frame.heat", json!({"frame": 3}), Nothing),
            Plan::Ask("frame.heat", json!({}), Cut::Cold)
        );
        let cold = Cut::Cold.apply(
            json!({"rows": ["0379", "9"], "classes": ["htp.", "h"]}),
            Layout,
        );
        assert_eq!(
            cold,
            json!({"rows": ["9999", "9"], "classes": ["....", "."], "degree": "layout"})
        );
        for method in ["screen.cursor", "frame.heat", "frame.idle"] {
            assert_eq!(ask(method, json!({}), Shape), Plan::Pass, "{method}");
        }
        assert_eq!(ask("screen.wait", json!({"idle": true}), Shape), Plan::Pass);
    }

    #[test]
    fn the_users_pattern_opens_itself_alone() {
        let hand = Hand {
            words: vec!["Rose".into()],
            patterns: vec![("TODO:".into(), false), (r"\d+ ms".into(), true)],
        };
        let ask =
            |method: &str, params: Value, degree| plan(method, &params, degree, &[], &hand, RAW);
        let none = Plan::Answer(json!({"matches": []}));
        for degree in [Nothing, Layout, Shape, Words] {
            // Exactly the pattern given, `known` too: as under `text`.
            let given = json!({"pattern": "TODO:", "known": true, "ignore_case": true, "frame": 2});
            let Plan::Ask("screen.find", asked, Cut::Mark) = ask("screen.find", given, degree)
            else {
                panic!("{degree:?}")
            };
            assert_eq!(asked["pattern"], "TODO:");
            assert_eq!(asked["known"], true);
            assert_eq!(asked.get("frame").is_some(), degree >= Layout);
            let regex = json!({"pattern": r"\d+ ms", "regex": true, "timeout_ms": 5});
            assert!(matches!(
                ask("screen.wait", regex, degree),
                Plan::Ask("screen.wait", _, Cut::Mark)
            ));
            // Anything else: as before. Not a regex of the literal, not
            // the literal of the regex, not a piece of it, not the rest
            // of the screen.
            for other in [
                json!({"pattern": "TODO:", "regex": true}),
                json!({"pattern": r"\d+ ms"}),
                json!({"pattern": "TODO"}),
                json!({"pattern": ".*", "regex": true}),
                json!({"pattern": "password"}),
            ] {
                assert_eq!(ask("screen.find", other.clone(), degree), none, "{other}");
            }
            // The user's word: found whole, at any degree.
            let Plan::Ask("screen.find", asked, Cut::Mark) =
                ask("screen.find", json!({"pattern": "rose"}), degree)
            else {
                panic!()
            };
            assert_eq!(asked["pattern"], r"\brose\b");
            // The text itself stays cut.
            assert_ne!(ask("screen.text", json!({}), degree), Plan::Pass);
        }
        // Shown in the shape from `shape`; not at `none`.
        assert_eq!(
            ask("screen.shape", json!({"words": ["rose"]}), Shape),
            Plan::Ask(
                "screen.shape",
                json!({"words": ["rose"]}),
                Cut::Lines { blank: false }
            )
        );
        assert_ne!(
            ask("screen.shape", json!({"words": ["fish"]}), Shape),
            Plan::Pass
        );
        let Plan::Ask("screen.shape", asked, _) = ask("screen.text", json!({}), Shape) else {
            panic!()
        };
        assert_eq!(asked["words"], json!(["Rose"]));
        let Plan::Ask("screen.shape", asked, _) = ask("screen.text", json!({}), Nothing) else {
            panic!()
        };
        assert!(asked.get("words").is_none());
    }

    #[test]
    fn find_and_wait_are_held_to_the_words() {
        let query = json!({"pattern": r"\bFISH\b", "regex": true, "ignore_case": true,
                           "known": true});
        assert_eq!(
            ask(
                "screen.find",
                json!({"pattern": "FISH", "known": true}),
                Words
            ),
            Plan::Ask("screen.find", query, Cut::Mark)
        );
        let none = Plan::Answer(json!({"matches": []}));
        let find = |params: Value, degree| ask("screen.find", params, degree);
        assert_eq!(find(json!({"pattern": "fish"}), Shape), none);
        assert_eq!(find(json!({"pattern": "fis"}), Words), none);
        assert_eq!(find(json!({"pattern": "fish", "regex": true}), Words), none);
        assert_eq!(find(json!({"pattern": "password"}), Words), none);
        assert_eq!(find(json!({}), Words), none);
        let wait = |params: Value, degree| ask("screen.wait", params, degree);
        assert_eq!(
            wait(json!({"pattern": "secret"}), Words),
            Plan::Answer(json!({"matched": false, "match": null}))
        );
        assert_eq!(
            wait(json!({"pattern": "secret", "gone": true}), Nothing),
            Plan::Answer(json!({"matched": true, "match": null}))
        );
        let Plan::Ask("screen.wait", asked, Cut::Mark) = wait(
            json!({"pattern": "food", "timeout_ms": 9, "frame": 1}),
            Words,
        ) else {
            panic!()
        };
        assert_eq!(asked["pattern"], r"\bfood\b");
        assert_eq!(asked["timeout_ms"], 9);
        assert_eq!(asked["frame"], 1);
    }

    #[test]
    fn what_the_frames_remember_is_masked() {
        let held = words();
        assert_eq!(
            mask("the fish_food, fishing!  ", &held),
            "### fish#food# ########"
        );
        assert_eq!(mask("FISH 魚 a\tb", &held), "FISH ## # #");
        assert_eq!(mask("fish", &[]), "####");
        assert_eq!(mask("   ", &held), "");
        let cut = Cut::Mask(held).apply(json!({"lines": ["my food", "x"], "y0": -3}), Words);
        assert_eq!(
            cut,
            json!({"lines": ["## food", "#"], "y0": -3, "degree": "words"})
        );
        let Plan::Ask("frame.text", asked, Cut::Mask(shown)) =
            ask("frame.text", json!({"frame": 1, "y0": 2, "y1": 9}), Shape)
        else {
            panic!()
        };
        assert_eq!(asked, json!({"frame": 1, "y0": 2, "y1": 9}));
        assert!(shown.is_empty());
    }

    fn info() -> Value {
        json!({"id": 3, "kind": "pane", "state": "stable", "usable": true,
               "rect": [0, 1, 80, 10], "origin": [0, 40], "extent": [0, 90],
               "key": "ranger:/home/me", "support": "vim", "idle": 0.2, "is_idle": false})
    }

    #[test]
    fn a_frame_is_an_id_and_a_rectangle() {
        let cut = Cut::Frame.apply(json!({"frame": info()}), Shape);
        let frame = &cut["frame"];
        assert_eq!(frame["key"], Value::Null);
        assert_eq!(frame["support"], Value::Null);
        assert_eq!(frame["rect"], json!([0, 1, 80, 10]));
        assert_eq!(frame["extent"], json!([0, 90]));
        assert_eq!(frame["is_idle"], false);
        assert_eq!(cut["degree"], "shape");
        let cut = Cut::Frames.apply(json!({"frames": [info()], "layout": 7}), Layout);
        let frame = &cut["frames"][0];
        assert_eq!(
            (&frame["is_idle"], &frame["idle"]),
            (&json!(true), &json!(1.0))
        );
        assert_eq!(frame["extent"], json!([0, 9]));
        assert_eq!(frame["origin"], json!([0, 0]));
        assert_eq!(cut["layout"], 7);
        assert!(!cut.to_string().contains("ranger"));
    }

    #[test]
    fn below_layout_the_surface_is_the_one_frame() {
        assert_eq!(
            ask("frame.list", json!({}), Nothing),
            Plan::Ask("frame.get", json!({}), Cut::Surface)
        );
        assert_eq!(
            ask("frame.get", json!({"frame": "largest"}), Nothing),
            Plan::Ask("frame.get", json!({}), Cut::Frame)
        );
        assert_eq!(
            ask("frame.get", json!({"frame": 4}), Layout),
            Plan::Ask("frame.get", json!({"frame": 4}), Cut::Frame)
        );
        assert_eq!(
            ask("frame.at", json!({"x": 3, "y": 4}), Nothing),
            Plan::Answer(json!({"frame": 0, "pos": [3, 4]}))
        );
        assert_eq!(
            ask("frame.at", json!({"name": "vim"}), Nothing),
            Plan::Answer(json!({"frame": 0}))
        );
        assert_eq!(ask("frame.at", json!({"x": 3, "y": 4}), Layout), Plan::Pass);
        let cut = Cut::Surface.apply(json!({"frame": info()}), Nothing);
        assert_eq!(cut["frames"].as_array().unwrap().len(), 1);
        assert_eq!(cut["layout"], 0);
        assert_eq!(cut["degree"], "none");
    }

    /// Below `layout` on a pane: read there, told as frame 0 at the
    /// top-left, the coordinates of its draws.
    #[test]
    fn below_layout_the_surface_is_read_and_told_as_frame_0() {
        let hand = Hand {
            words: vec!["fish".into()],
            patterns: vec![("a b".into(), false)],
        };
        let on = |method: &str, params: Value, degree| plan(method, &params, degree, &[], &hand, 7);
        for method in ["frame.get", "frame.list"] {
            let Plan::Ask(_, asked, _) = on(method, json!({"frame": 0}), Nothing) else {
                panic!("{method}");
            };
            assert_eq!(asked, json!({"frame": 7}), "{method}");
        }
        let Plan::Ask(_, asked, _) = on("screen.find", json!({"pattern": "fish"}), Nothing) else {
            panic!("screen.find");
        };
        assert_eq!(asked["frame"], 7);
        let Plan::Ask(_, asked, _) = on(
            "screen.find",
            json!({"pattern": "a b", "frame": 0}),
            Nothing,
        ) else {
            panic!("the user's pattern");
        };
        assert_eq!(asked["frame"], 7);
        // From `layout`: as named.
        let Plan::Ask(_, asked, _) = on("frame.get", json!({"frame": 3}), Layout) else {
            panic!("layout");
        };
        assert_eq!(asked, json!({"frame": 3}));

        let mut found = json!({"matches": [{"frame": 7, "x": 2, "y": 1, "screen": [32, 21]},
                                           {"frame": 7, "x": 2, "y": 90, "screen": null}]});
        on_surface(&mut found, [30, 20]);
        assert_eq!(
            found["matches"][0],
            json!({"frame": 0, "x": 2, "y": 1, "screen": [2, 1]})
        );
        assert_eq!(found["matches"][1]["screen"], Value::Null);
        let mut got = json!({"frame": {"id": 7, "rect": [30, 20, 30, 10], "origin": [0, 0]}});
        on_surface(&mut got, [30, 20]);
        assert_eq!(got["frame"]["id"], 0);
        assert_eq!(got["frame"]["rect"], json!([0, 0, 30, 10]));
        let mut listed = json!({"frames": [{"id": 7, "rect": [30, 20, 30, 10]}], "layout": 0});
        on_surface(&mut listed, [30, 20]);
        assert_eq!(
            listed["frames"][0],
            json!({"id": 0, "rect": [0, 0, 30, 10]})
        );
        let mut read = json!({"lines": [], "size": [30, 10], "rect": [30, 20, 30, 10],
                              "sprites": [{"screen": [31, 22]}]});
        on_surface(&mut read, [30, 20]);
        assert_eq!(read["rect"], json!([0, 0, 30, 10]));
        assert_eq!(read["sprites"][0]["screen"], json!([1, 2]));
    }

    /// Below `shape`, told unscrolled: frame cells are view cells, in
    /// what it draws, reads and hears.
    #[test]
    fn below_shape_a_frame_is_its_view() {
        let put = |anchor: Value| {
            let mut params = json!({"frame": 7, "anchor": anchor});
            in_view(&mut params, [0, 21]);
            params["anchor"].clone()
        };
        let view = json!({"at": [2, 3], "space": "view"});
        assert_eq!(put(json!({"at": [2, 3], "space": "frame"})), view);
        assert_eq!(put(json!({"at": [2, 3]})), view);
        // Off the view: in frame cells, from its origin.
        assert_eq!(
            put(json!({"at": [-2, 3]})),
            json!({"at": [-2, 24], "space": "frame"})
        );
        let hud = json!({"at": [-1, 0], "space": "view"});
        assert_eq!(put(hud.clone()), hud);
        assert_eq!(put(json!({"text": "x"})), json!({"text": "x"}));

        let origin = |f: u64| (f == 7).then_some([0, 21]);
        let mut at = json!({"frame": 7, "pos": [2, 24]});
        unscrolled(&mut at, &origin);
        assert_eq!(at, json!({"frame": 7, "pos": [2, 3]}));
        let mut found = json!({"matches": [{"frame": 7, "x": 1, "y": 22, "screen": [1, 1]},
                                           {"frame": 0, "x": 1, "y": 2, "screen": [1, 2]}]});
        unscrolled(&mut found, &origin);
        assert_eq!(
            (
                found["matches"][0]["x"].clone(),
                found["matches"][0]["y"].clone()
            ),
            (json!(1), json!(1))
        );
        assert_eq!(found["matches"][1]["y"], 2);
        // A report of the pointer below `layout`: of the surface at 30, 20.
        let mut hover = json!({"sprite": 4, "at": [1, 0], "x": 33, "y": 23, "frame": 7,
                               "pos": [3, 24], "mods": []});
        unscrolled(&mut hover, &origin);
        on_surface(&mut hover, [30, 20]);
        assert_eq!(
            hover,
            json!({"sprite": 4, "at": [1, 0], "x": 3, "y": 3, "frame": 0, "pos": [3, 3],
                   "mods": []})
        );
        let mut left = json!({"sprite": null, "at": null, "x": 33, "y": 23, "frame": null,
                              "pos": null});
        on_surface(&mut left, [30, 20]);
        assert_eq!(
            (left["x"].clone(), left["frame"].clone()),
            (json!(3), Value::Null)
        );
        // Over another pane: `pos` of frame 0 still, not of that pane.
        let mut other = json!({"x": 50, "y": 21, "frame": 9, "pos": [2, 1]});
        on_surface(&mut other, [30, 20]);
        assert_eq!(other, json!({"x": 20, "y": 1, "frame": 0, "pos": [20, 1]}));
    }

    #[test]
    fn a_widget_of_surface_widget_is_told_in_its_own_cells() {
        let mut info = json!({"id": 1, "frame": 7, "rect": [41, 3, 22, 8], "size": [20, 6]});
        own_widget(&mut info);
        assert_eq!(info["rect"], json!([0, 0, 22, 8]));
        let mut hidden = json!({"rect": null});
        own_widget(&mut hidden);
        assert_eq!(hidden["rect"], Value::Null);
        // Its corner at 41, 3: on it, and dragged off it to the left.
        let mut on = json!({"sprite": 4, "at": [2, 1], "x": 43, "y": 4, "frame": 7,
                            "pos": [12, 4]});
        on_widget(&mut on, [41, 3]);
        assert_eq!(
            on,
            json!({"sprite": 4, "at": [2, 1], "x": 2, "y": 1, "frame": 0, "pos": [2, 1]})
        );
        let mut off = json!({"sprite": 4, "at": null, "x": 39, "y": 4, "frame": 2,
                             "pos": [39, 4]});
        on_widget(&mut off, [41, 3]);
        assert_eq!(
            (off["x"].clone(), off["pos"].clone()),
            (json!(-2), json!([-2, 1]))
        );
    }

    #[test]
    fn every_pointer_report_to_a_widget_app_is_in_its_widgets_cells() {
        // Two widgets placed, corners 41, 3 and 2, 10.
        let placed = [(4, [41, 3]), (9, [2, 10])];
        let told = |mut report: Value, own: bool, placed: &[(u64, [i64; 2])]| {
            in_widget(&mut report, own, placed).then_some(report)
        };
        // On its second widget: that one's corner.
        let on = json!({"sprite": 9, "at": [1, 1], "x": 3, "y": 11, "frame": 2, "pos": [3, 1]});
        assert_eq!(
            told(on, true, &placed),
            Some(json!({"sprite": 9, "at": [1, 1], "x": 1, "y": 1, "frame": 0, "pos": [1, 1]}))
        );
        // Elsewhere, on another app's sprite left of and above it: the
        // first's corner, negative; `at` of that sprite as it is.
        let other = json!({"sprite": 12, "at": [0, 0], "x": 30, "y": 1, "frame": 5,
                           "pos": [30, 1], "mods": []});
        assert_eq!(
            told(other.clone(), false, &placed),
            Some(
                json!({"sprite": 12, "at": [0, 0], "x": -11, "y": -2, "frame": 0,
                        "pos": [-11, -2], "mods": []})
            )
        );
        // A leave: no frame there; told in its cells all the same.
        let leave = json!({"sprite": null, "at": null, "x": 50, "y": 3, "frame": null,
                           "pos": null, "mods": []});
        assert_eq!(
            told(leave, false, &placed),
            Some(
                json!({"sprite": null, "at": null, "x": 9, "y": 0, "frame": 0,
                        "pos": [9, 0], "mods": []})
            )
        );
        // Not placed: no corner, not told; its own widget gone mid-drag,
        // the same, though another is placed.
        assert_eq!(told(other, false, &[]), None);
        let held = json!({"sprite": 4, "at": null, "x": 1, "y": 1, "frame": null, "pos": null});
        assert_eq!(told(held, true, &placed[1..]), None);
    }

    #[test]
    fn the_session_is_its_size() {
        let whole = json!({"protocol": 1, "server": "ptyrc/2", "session": "s", "pid": 4,
                           "driver_pid": 5, "argv": ["vim", "secret.txt"], "size": [80, 24],
                           "headless": false, "active_at": 12, "title": "secret.txt",
                           "host_colors": {"fg": "#fff"}});
        assert_eq!(
            ask("session.info", json!({}), Nothing),
            Plan::Ask("session.info", json!({}), Cut::Info)
        );
        let cut = Cut::Info.apply(whole.clone(), Layout);
        assert_eq!(
            cut,
            json!({"protocol": 1, "server": "ptyrc/2", "size": [80, 24], "headless": false,
                   "degree": "layout"})
        );
        assert_eq!(hello(whole.clone(), Words)["host_colors"]["fg"], "#fff");
        assert!(!hello(whole.clone(), Words).to_string().contains("secret"));
        assert_eq!(hello(whole.clone(), Text), whole);
    }

    #[test]
    fn frames_events_are_cut_and_hints_scrubbed() {
        let data = event("frames", json!({"frames": [info()]}), Shape);
        assert_eq!(data["frames"][0]["key"], Value::Null);
        assert_eq!(data["degree"], "shape");
        let whole = json!({"frames": [info()]});
        assert_eq!(event("frames", whole.clone(), Text), whole);
        assert_eq!(event("cursor", json!({"x": 1}), Shape), json!({"x": 1}));
        let error = RpcError::new(ErrorCode::NoFrame, "no such frame: x").with_hint("frames: a");
        assert_eq!(scrub(error).hint, None);
        let error = RpcError::bad_params("x").with_hint("y");
        assert_eq!(scrub(error).hint.as_deref(), Some("y"));
    }

    /// Every method the table lowers has a plan at every degree below
    /// the one that opens it: nothing falls through to the engine whole.
    #[test]
    fn every_read_below_its_degree_is_planned() {
        use crate::grants::{Gate, METHODS};
        for (method, gate) in METHODS {
            let Gate::Read(opens) = gate else { continue };
            for degree in [Nothing, Layout, Shape, Words] {
                if degree >= *opens {
                    continue;
                }
                let planned = ask(method, json!({"pattern": "x"}), degree);
                assert_ne!(planned, Plan::Pass, "{method} at {degree:?}");
            }
        }
    }

    // The cases of the driver's `own.rs` and of the box's `grants.rs`.
    #[test]
    fn where_an_anchor_landed_is_told_at_its_degree() {
        let sprite = |anchor: Value| {
            json!({"anchor": anchor, "screen": [3, 4], "visible": true, "covered": 0.5,
                   "frame": NOWHERE})
        };
        let hand = Hand {
            words: vec!["Rose".into()],
            ..Default::default()
        };
        let words = ["fish".to_string()];
        let told = |anchor: Value, degree: Degree| {
            let mut item = sprite(anchor);
            let hidden = landing(&mut item, Some((degree, &words, &hand)), Some(5));
            assert_eq!(hidden.is_some(), item["screen"].is_null());
            (item["screen"] != Value::Null, item["frame"].clone(), hidden)
        };
        let text = |t: &str| json!({"text": t});
        assert_eq!(told(text("fish"), Shape), (false, json!(5), Some(Words)));
        assert_eq!(told(text("fish"), Words), (true, json!(5), None));
        assert_eq!(told(text("cat"), Words).2, Some(Text));
        assert!(told(text("cat"), Text).0);
        assert!(!told(json!({"text": "fish", "regex": true}), Words).0);
        assert!(told(text("rose"), Shape).0);
        let cursor = json!({"cursor": {"dx": 1}});
        assert_eq!(
            told(cursor.clone(), Nothing),
            (false, json!(0), Some(Shape))
        );
        assert!(told(cursor, Shape).0);
        assert!(!told(json!({"auto": {}}), Layout).0);
        assert_eq!(told(json!({"at": [1, 1]}), Nothing), (true, json!(0), None));
        // The whole screen held: all told; `NOWHERE` is still its frame.
        let mut item = sprite(text("cat"));
        assert_eq!(landing(&mut item, None, Some(5)), None);
        assert_eq!(
            (&item["visible"], &item["frame"]),
            (&json!(true), &json!(5))
        );
        // Not a store app: the frame as it is.
        let mut item = sprite(text("cat"));
        landing(&mut item, Some((Nothing, &words, &hand)), None);
        assert_eq!(item["frame"], json!(NOWHERE));
    }

    #[test]
    fn found_and_shown() {
        let hand = Hand {
            words: vec!["Rose".into()],
            ..Default::default()
        };
        let held = words();
        assert_eq!(found(Shape, &held, &hand), ["Rose"]);
        assert_eq!(found(Words, &held, &hand), ["fish", "food", "Rose"]);
        assert!(shown(Layout, &held, &hand).is_empty());
        assert_eq!(shown(Shape, &held, &hand), ["Rose"]);
        assert!(holds(&held, "FISH") && !holds(&held, "fis"));
        assert!(composes_own("screen.cells") && !composes_own("screen.shape"));
    }
}
