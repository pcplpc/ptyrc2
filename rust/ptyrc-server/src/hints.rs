// SPDX-License-Identifier: GPL-3.0-or-later
//! What to try after an error: the valid values, the nearest one.
//!
//! One pass over the errors of the engine's methods, so that the CLI, the
//! Python pilot and any other client read the same hint.

use crate::screen::ScreenEngine;
use ptyrc_core::OwnerId;
use ptyrc_core::widgets::WidgetId;
use ptyrc_proto::{ErrorCode, RpcError, suggest};

/// Every method of the protocol, for the nearest-name hint.
const METHODS: [&str; 50] = [
    "session.info",
    "session.resize",
    "session.colors",
    "screen.text",
    "screen.cells",
    "screen.find",
    "screen.cursor",
    "screen.wait",
    "input.send",
    "input.click",
    "input.grab",
    "input.release",
    "tape.status",
    "tape.step",
    "tape.seek",
    "tape.play",
    "tape.pause",
    "tape.quit",
    "frame.list",
    "frame.text",
    "frame.heat",
    "frame.idle",
    "frame.get",
    "frame.at",
    "draw.put",
    "draw.update",
    "draw.remove",
    "draw.clear",
    "draw.list",
    "draw.ext",
    "draw.exts",
    "layer.define",
    "layer.list",
    "widget.declare",
    "widget.show",
    "widget.set",
    "widget.hide",
    "widget.list",
    "widget.borders",
    "app.list",
    "app.start",
    "app.stop",
    "app.pause",
    "app.resume",
    "app.restart",
    "app.forget",
    "batch",
    "subscribe",
    "debug.overlay",
    "debug.dump",
];

const FRAME_NAMES: [&str; 5] = ["raw", "active", "largest", "quietest", "idle"];
const COLOURS: [&str; 10] = [
    "default", "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white", "grey",
];
const BORDER: &str = r#"{"base":NAME,"corners":"┌┐└┘","top":"─","bottom":"─","left":"│","right":"│","style":{...},"title":{"side":"top"|"bottom","align":"left"|"center"|"right","before":" ","after":" ","style":{...}}}"#;
const ANCHORS: &str = r#"one of {"at":[x,y],"space":"frame"|"view"}, {"text":"...","side":"right"|"left"|"above"|"below"|"over"}, {"auto":{"prefer":"top-left"|"top"|"top-right"|"left"|"center"|"right"|"bottom-left"|"bottom"|"bottom-right"|"any"}}, {"cursor":{"dx":1,"dy":0}}"#;

pub(crate) fn unknown_method(method: &str) -> RpcError {
    RpcError::unknown_method(method).with_hint(format!(
        "{}methods are listed in plan/design.md, \"Protocol\"",
        suggest::did_you_mean(method, METHODS)
    ))
}

/// The word after the last `: ` of a message.
fn subject(message: &str) -> &str {
    message.rsplit(": ").next().unwrap_or("").trim()
}

fn no_frame(engine: &ScreenEngine, message: &str) -> String {
    let frames: Vec<String> = engine
        .frames
        .list()
        .iter()
        .map(|f| match &f.key {
            Some(key) => format!("{} ({key})", f.id),
            None => f.id.to_string(),
        })
        .collect();
    format!(
        "{}frames: {}; names: {}",
        suggest::did_you_mean(subject(message), FRAME_NAMES),
        frames.join(", "),
        FRAME_NAMES.join(", ")
    )
}

fn no_sprite(engine: &ScreenEngine) -> String {
    let ids: Vec<String> = engine
        .scene
        .list()
        .iter()
        .map(|s| s.id.to_string())
        .collect();
    if ids.is_empty() {
        "no sprite is up".into()
    } else {
        format!("sprites: {}", ids.join(", "))
    }
}

fn bad_params(engine: &ScreenEngine, error: &mut RpcError) -> Option<String> {
    let message = error.message.clone();
    let word = subject(&message);
    if message.starts_with("bad colour") {
        return Some(format!(
            "{}a colour is a name ({}, bright-<name>), 0..255, or #rrggbb",
            suggest::did_you_mean(word, COLOURS),
            COLOURS.join(", ")
        ));
    }
    if let Some(rest) = message.strip_prefix("bad layer: ") {
        let name = rest.split(" (").next().unwrap_or(rest);
        let layers = engine.scene.layers();
        error.message = format!("bad layer: {name}");
        let fixed = ptyrc_core::scene::Layer::NAMES;
        let declared: Vec<String> = layers
            .iter()
            .filter(|(n, _)| !fixed.iter().any(|(f, _)| f == n))
            .map(|(n, z)| format!("{n} ({z})"))
            .collect();
        return Some(format!(
            "{}layers: wallpaper, back, front, hud{}{}, or an integer other than 0; \
             `layer.define` declares a name",
            suggest::did_you_mean(name, layers.iter().map(|(n, _)| n.as_str())),
            if declared.is_empty() {
                ""
            } else {
                "; declared: "
            },
            declared.join(", ")
        ));
    }
    if message.contains("untagged enum Border") {
        error.message = "bad border".into();
        return Some(format!(
            "a preset name ({}), or {}",
            engine.widgets.borders().join(", "),
            BORDER
        ));
    }
    if message.contains("untagged enum Anchor") {
        error.message = "bad anchor".into();
        return Some(ANCHORS.into());
    }
    if let Some(name) = message.strip_prefix("no draw extension ") {
        let names: Vec<&str> = engine.exts.iter().map(|e| e.name()).collect();
        return Some(format!(
            "{}extensions: {}",
            suggest::did_you_mean(name, names.iter().copied()),
            names.join(", ")
        ));
    }
    if let Some(name) = message.strip_prefix("no function ") {
        let all: Vec<String> = engine
            .exts
            .iter()
            .map(|e| {
                let names: Vec<&str> = e.functions().iter().map(|f| f.name).collect();
                format!("{}: {}", e.name(), names.join(" "))
            })
            .collect();
        let names: Vec<&str> = engine
            .exts
            .iter()
            .flat_map(|e| e.functions())
            .map(|f| f.name)
            .collect();
        return Some(format!(
            "{}functions: {}",
            suggest::did_you_mean(name, names),
            all.join("; ")
        ));
    }
    if message.starts_with("unknown effect") {
        let effects = engine.scene.effects();
        return Some(format!(
            "{}effects: {}",
            suggest::did_you_mean(word, effects.iter().map(String::as_str)),
            effects.join(", ")
        ));
    }
    if message.starts_with("what: ") {
        error.message = format!("no such overlay: {word}");
        return Some("overlays: frames, heat, off".into());
    }
    if message.starts_with("invalid type") || message.starts_with("invalid length") {
        return Some(r#"params is a JSON object, as in {"frame": "raw"}"#.into());
    }
    if message.starts_with("regex parse error") || message.contains("regex") {
        return Some("without `regex` (-E) the pattern is searched as it is written".into());
    }
    None
}

/// The hint of an error of a `widget.*` method of `owner`; `id`: the
/// widget named, for its variants.
pub(crate) fn widget(
    engine: &ScreenEngine,
    owner: OwnerId,
    id: Option<WidgetId>,
    mut error: RpcError,
) -> RpcError {
    let message = error.message.clone();
    let word = subject(&message);
    let list = |names: &[String]| names.join(", ");
    error.hint = if message.starts_with("no such widget") {
        let ids: Vec<String> = engine
            .widgets
            .ids_of(owner)
            .iter()
            .map(|id| id.to_string())
            .collect();
        Some(if ids.is_empty() {
            "this connection shows no widget: `widget.show` first".into()
        } else {
            format!("widgets of this connection: {}", list(&ids))
        })
    } else if message.starts_with("no such tag") {
        let tags = engine.widgets.tags_of(owner);
        Some(if tags.is_empty() {
            "this connection declared no tag: `widget.declare` first".into()
        } else {
            format!(
                "{}tags of this connection: {}",
                suggest::did_you_mean(word, tags.iter().map(String::as_str)),
                list(&tags)
            )
        })
    } else if message.starts_with("no such border") {
        let names = engine.widgets.borders();
        Some(format!(
            "{}borders: {}",
            suggest::did_you_mean(word, names.iter().map(String::as_str)),
            list(&names)
        ))
    } else if message.starts_with("no such variant") || error.code == ErrorCode::NotUsable {
        let names = id
            .map(|id| engine.widgets.variants_named(id))
            .unwrap_or_default();
        let near = if error.code == ErrorCode::NotUsable {
            String::new()
        } else {
            suggest::did_you_mean(word, names.iter().map(String::as_str))
        };
        Some(format!("{near}variants: {}", list(&names)))
    } else if ["size of", "tag", "variant"]
        .iter()
        .any(|p| message.starts_with(p))
        || message.contains("declared twice")
    {
        Some(
            r#"a variant: {"tag", "name", "size": [w, h], "content"}; names unique in a tag; 16 variants a tag, 64 tags a connection, sizes 1x1 to 200x60"#
                .into(),
        )
    } else if message.starts_with("border ") {
        Some(
            "corners: four glyphs; an edge: a pattern of one glyph or more; glyphs of one cell"
                .into(),
        )
    } else if message.starts_with("fill") {
        Some("fill: the share of the room the widget takes, 0.1 to 1 (0.5)".into())
    } else {
        None
    };
    explain(engine, error)
}

/// Add a hint to an error of the engine that has none.
pub(crate) fn explain(engine: &ScreenEngine, mut error: RpcError) -> RpcError {
    if error.hint.is_some() {
        return error;
    }
    error.hint = match error.code {
        ErrorCode::NoFrame => Some(no_frame(engine, &error.message)),
        ErrorCode::NoSprite => Some(no_sprite(engine)),
        ErrorCode::BadParams => bad_params(engine, &mut error),
        ErrorCode::NotUsable => Some("frames on screen: `frame.list` (ptyrc frames)".into()),
        _ => None,
    };
    error
}
