// SPDX-License-Identifier: GPL-3.0-or-later
//! The answers cut to a degree. See `plan/design.md`, "Permissions",
//! and `plan/store-model.md`, "A no lowers, it does not break". Pure:
//! what to ask the engine in place of the request, and what to make of
//! its answer.

use crate::grants::Degree;
use ptyrc_core::RAW;
use ptyrc_proto::{ErrorCode, RpcError};
use ptyrc_vt::char_width;
use serde_json::{Map, Value, json};

/// What becomes of a request of an app that holds `degree`, below
/// `text`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Plan {
    /// As asked: nothing of it is above the degree.
    Pass,
    /// Ask the engine this, then cut its answer.
    Ask(&'static str, Value, Cut),
    /// Answered here; the engine is not asked.
    Answer(Value),
}

/// What is made of the engine's answer.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Cut {
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
/// `held` (empty below `words`). `Pass` for what the degree opens whole
/// and for what the degrees do not rule.
pub(crate) fn plan(method: &str, params: &Value, degree: Degree, held: &[String]) -> Plan {
    if degree >= Degree::Text {
        return Plan::Pass;
    }
    let held: &[String] = match degree >= Degree::Words {
        true => held,
        false => &[],
    };
    let shape = degree >= Degree::Shape;
    let has = |member: &str| params.get(member).is_some_and(|v| !v.is_null());
    // Below `layout` there is one frame: the surface.
    let frame = |out: &mut Map<String, Value>| {
        if degree >= Degree::Layout && has("frame") {
            out.insert("frame".into(), params["frame"].clone());
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
            if shape && kept.len() == asked.len() {
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
        "screen.find" => match word_query(params, held) {
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
        "screen.wait" if has("pattern") => match word_query(params, held) {
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
        "frame.list" if degree < Degree::Layout => Plan::Ask("frame.get", json!({}), Cut::Surface),
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

/// `frame.idle` of a blank screen.
fn still() -> Value {
    json!({"score": 1.0, "idle": true, "free": 1.0})
}

fn holds(held: &[String], word: &str) -> bool {
    let word = word.to_lowercase();
    held.iter().any(|h| h.to_lowercase() == word)
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
pub(crate) fn mask(line: &str, words: &[String]) -> String {
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
pub(crate) fn frame_info(info: &mut Value, degree: Degree) {
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

/// The data of an event of `topic` for an app that holds `degree`.
pub(crate) fn event(topic: &str, mut data: Value, degree: Degree) -> Value {
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
pub(crate) fn scrub(mut error: RpcError) -> RpcError {
    if matches!(error.code, ErrorCode::NoFrame | ErrorCode::NoSprite) {
        error.hint = None;
    }
    error
}

/// The `hello` of a connection of an app that holds `degree`.
pub(crate) fn hello(hello: Value, degree: Degree) -> Value {
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
        plan(method, &params, degree, &words())
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
}
