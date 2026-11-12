// SPDX-License-Identifier: GPL-3.0-or-later
//! Protocol methods of the screen engine. See `plan/design.md`,
//! "Protocol".

use crate::engine::{ConnId, Ctx, Reply, Token};
use crate::screen::{DEBUG_OWNER, Overlay, ScreenEngine, Wait};
use ptyrc_core::scene::{ColorSpec, SpritePatch, SpriteSpec};
use ptyrc_core::{FrameId, FrameKind, FrameSel, FrameState, RAW, Surfaces, TextMatch, TextQuery};
use ptyrc_proto::{ErrorCode, RpcError};
use ptyrc_vt::{Attrs, Cell, Color, Grid, Rect, Style, render_ansi};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use std::time::Duration;

type Res = Result<Value, RpcError>;

fn parse<T: DeserializeOwned>(params: Value) -> Result<T, RpcError> {
    let params = if params.is_null() { json!({}) } else { params };
    serde_json::from_value(params).map_err(|e| RpcError::bad_params(e.to_string()))
}

fn frame_of(engine: &ScreenEngine, sel: &Option<FrameSel>) -> Result<FrameId, RpcError> {
    match sel {
        None => Ok(RAW),
        Some(sel) => {
            let id = engine.frames.resolve(sel).ok_or_else(|| match sel {
                FrameSel::Id(id) => ScreenEngine::no_frame(id),
                FrameSel::Name(name) => ScreenEngine::no_frame(name),
            })?;
            engine
                .frames
                .info(id)
                .map(|_| id)
                .ok_or_else(|| ScreenEngine::no_frame(id))
        }
    }
}

/// Screen rect of a frame's view, optionally narrowed by a rect relative
/// to it.
fn view_rect(engine: &ScreenEngine, id: FrameId, rect: Option<[u16; 4]>) -> Result<Rect, RpcError> {
    let info = engine
        .frames
        .info(id)
        .ok_or_else(|| ScreenEngine::no_frame(id))?;
    if info.state == FrameState::Inactive {
        return Err(RpcError::new(
            ErrorCode::NotUsable,
            format!("frame {id} is not on screen"),
        ));
    }
    let view = info.rect.intersect(&engine.app.grid().rect());
    Ok(match rect {
        None => view,
        Some([x, y, w, h]) => {
            Rect::new(view.x.saturating_add(x), view.y.saturating_add(y), w, h).intersect(&view)
        }
    })
}

fn rect_text(grid: &Grid, rect: Rect) -> Vec<String> {
    (rect.y..rect.bottom())
        .map(|row| {
            let text: String = (rect.x..rect.right())
                .map(|col| grid.cell(col, row))
                .filter(|c| c.width != 0)
                .map(|c| c.text.as_str())
                .collect();
            text.trim_end_matches(' ').to_string()
        })
        .collect()
}

fn color_json(color: Color) -> Value {
    serde_json::to_value(ColorSpec(color)).unwrap_or(Value::Null)
}

fn style_json(style: &Style, run: &mut Map<String, Value>) {
    if style.fg != Color::Default {
        run.insert("fg".into(), color_json(style.fg));
    }
    if style.bg != Color::Default {
        run.insert("bg".into(), color_json(style.bg));
    }
    for (bit, name) in [
        (Attrs::BOLD, "bold"),
        (Attrs::DIM, "dim"),
        (Attrs::ITALIC, "italic"),
        (Attrs::UNDERLINE, "underline"),
        (Attrs::INVERSE, "inverse"),
        (Attrs::STRIKE, "strike"),
        (Attrs::BLINK, "blink"),
    ] {
        if style.attrs.contains(bit) {
            run.insert(name.into(), Value::Bool(true));
        }
    }
}

/// A row as runs of equal style.
fn row_runs(cells: &[&Cell]) -> Value {
    let mut runs: Vec<Value> = Vec::new();
    let mut text = String::new();
    let mut style: Option<Style> = None;
    let mut flush = |text: &mut String, style: &Style| {
        let mut run = Map::new();
        run.insert("t".into(), Value::String(std::mem::take(text)));
        style_json(style, &mut run);
        runs.push(Value::Object(run));
    };
    for cell in cells.iter().filter(|c| c.width != 0) {
        match style {
            Some(s) if s == cell.style => {}
            Some(s) => {
                flush(&mut text, &s);
                style = Some(cell.style);
            }
            None => style = Some(cell.style),
        }
        text.push_str(&cell.text);
    }
    if let Some(s) = style {
        flush(&mut text, &s);
    }
    Value::Array(runs)
}

fn match_json(m: &TextMatch) -> Value {
    json!({
        "frame": m.frame, "x": m.x, "y": m.y, "len": m.len, "text": m.text,
        "screen": m.screen.map(|(c, r)| json!([c, r])),
    })
}

#[derive(Deserialize)]
struct ViewParams {
    frame: Option<FrameSel>,
    rect: Option<[u16; 4]>,
    #[serde(default)]
    ansi: bool,
}

fn screen_text(engine: &ScreenEngine, params: Value) -> Res {
    let p: ViewParams = parse(params)?;
    let id = frame_of(engine, &p.frame)?;
    let rect = view_rect(engine, id, p.rect)?;
    let lines = if p.ansi {
        let all = render_ansi(engine.app.grid(), Some(&engine.app));
        if rect == engine.app.grid().rect() {
            all
        } else {
            // Styled text of a sub-rect: render a cropped copy.
            let mut crop = Grid::new(rect.w, rect.h);
            for row in 0..rect.h {
                for col in 0..rect.w {
                    *crop.cell_mut(col, row) =
                        engine.app.grid().cell(rect.x + col, rect.y + row).clone();
                }
            }
            render_ansi(&crop, Some(&engine.app))
        }
    } else {
        rect_text(engine.app.grid(), rect)
    };
    Ok(json!({"lines": lines, "size": [rect.w, rect.h], "rect": [rect.x, rect.y, rect.w, rect.h]}))
}

fn screen_cells(engine: &ScreenEngine, params: Value) -> Res {
    let p: ViewParams = parse(params)?;
    let id = frame_of(engine, &p.frame)?;
    let rect = view_rect(engine, id, p.rect)?;
    let grid = engine.app.grid();
    let rows: Vec<Value> = (rect.y..rect.bottom())
        .map(|row| {
            let cells: Vec<&Cell> = (rect.x..rect.right()).map(|c| grid.cell(c, row)).collect();
            row_runs(&cells)
        })
        .collect();
    Ok(json!({"rows": rows, "size": [rect.w, rect.h]}))
}

#[derive(Deserialize)]
struct FindParams {
    #[serde(flatten)]
    query: TextQuery,
    frame: Option<FrameSel>,
    #[serde(default)]
    known: bool,
}

fn screen_find(engine: &ScreenEngine, params: Value) -> Res {
    let p: FindParams = parse(params)?;
    let id = frame_of(engine, &p.frame)?;
    let matches: Vec<Value> = engine
        .frames
        .try_find(id, &p.query, p.known)
        .map_err(RpcError::bad_params)?
        .iter()
        .map(match_json)
        .collect();
    Ok(json!({"matches": matches}))
}

fn screen_cursor(engine: &ScreenEngine, params: Value) -> Res {
    let trail = params
        .get("trail")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let cursor = engine.app.cursor();
    let frame = engine.frames.at(cursor.col, cursor.row).map(|(id, _)| id);
    let mut out = json!({
        "x": cursor.col, "y": cursor.row, "visible": cursor.visible, "frame": frame,
    });
    if trail {
        let samples: Vec<Value> = engine
            .frames
            .cursor_trail()
            .iter()
            .map(|s| json!([s.at.as_millis() as u64, s.col, s.row]))
            .collect();
        out["trail"] = Value::Array(samples);
    }
    Ok(out)
}

#[derive(Deserialize)]
struct WaitParams {
    pattern: Option<String>,
    #[serde(default)]
    regex: bool,
    #[serde(default)]
    ignore_case: bool,
    #[serde(default)]
    gone: bool,
    stable_ms: Option<u64>,
    timeout_ms: Option<u64>,
    frame: Option<FrameSel>,
}

/// The result of a wait if it is satisfied now.
pub(crate) fn check_wait(engine: &ScreenEngine, wait: &Wait, now: Duration) -> Option<Value> {
    if let Some(stable) = wait.stable
        && now < engine.last_change + stable
    {
        return None;
    }
    match &wait.query {
        None => Some(json!({"matched": true, "match": null})),
        Some(query) => {
            let found = engine.frames.find(wait.frame, query, false);
            match (found.first(), wait.gone) {
                (Some(m), false) => Some(json!({"matched": true, "match": match_json(m)})),
                (None, true) => Some(json!({"matched": true, "match": null})),
                _ => None,
            }
        }
    }
}

fn screen_wait(
    engine: &mut ScreenEngine,
    conn: ConnId,
    token: Token,
    params: Value,
    ctx: &mut Ctx,
) -> Reply {
    let p: WaitParams = match parse(params) {
        Ok(p) => p,
        Err(e) => return Reply::Err(e),
    };
    let frame = match frame_of(engine, &p.frame) {
        Ok(id) => id,
        Err(e) => return Reply::Err(e),
    };
    if p.pattern.is_none() && p.stable_ms.is_none() {
        return Reply::Err(RpcError::bad_params("pattern or stable_ms is required"));
    }
    let now = ctx.now();
    let wait = Wait {
        token,
        conn,
        frame,
        query: p.pattern.map(|pattern| TextQuery {
            pattern,
            regex: p.regex,
            ignore_case: p.ignore_case,
        }),
        gone: p.gone,
        stable: p.stable_ms.map(Duration::from_millis),
        deadline: now + Duration::from_millis(p.timeout_ms.unwrap_or(5000)),
    };
    if let Some(query) = &wait.query
        && let Err(e) = engine.frames.try_find(frame, query, false)
    {
        return Reply::Err(RpcError::bad_params(e));
    }
    match check_wait(engine, &wait, now) {
        Some(result) => Reply::Ok(result),
        None => {
            engine.waits.push(wait);
            ctx.request_repaint();
            Reply::Pending
        }
    }
}

#[derive(Deserialize)]
struct FrameTextParams {
    frame: Option<FrameSel>,
    y0: Option<i64>,
    y1: Option<i64>,
}

fn frame_text(engine: &ScreenEngine, params: Value) -> Res {
    let p: FrameTextParams = parse(params)?;
    let id = frame_of(engine, &p.frame)?;
    let (lo, hi) = engine
        .frames
        .list()
        .iter()
        .find(|f| f.id == id)
        .map(|f| (f.extent[0], f.extent[1]))
        .ok_or_else(|| ScreenEngine::no_frame(id))?;
    let y0 = p.y0.unwrap_or(lo).max(lo);
    let y1 = p.y1.unwrap_or(hi + 1).min(hi + 1).max(y0);
    if y1 - y0 > 100_000 {
        return Err(RpcError::bad_params("more than 100000 rows"));
    }
    Ok(json!({"lines": engine.frames.text(id, y0, y1), "y0": y0}))
}

fn frame_heat(engine: &ScreenEngine, params: Value) -> Res {
    let p: ViewParams = parse(params)?;
    let id = frame_of(engine, &p.frame)?;
    let heat = engine
        .frames
        .heat(id)
        .ok_or_else(|| ScreenEngine::no_frame(id))?;
    Ok(json!({"rows": heat.digits()}))
}

fn frame_at(engine: &ScreenEngine, params: Value) -> Res {
    if let Some(name) = params.get("name").and_then(Value::as_str) {
        let id = frame_of(engine, &Some(FrameSel::Name(name.to_string())))?;
        return Ok(json!({"frame": id}));
    }
    let coord = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_u64)
            .and_then(|v| u16::try_from(v).ok())
            .ok_or_else(|| RpcError::bad_params(format!("{key}: a screen coordinate")))
    };
    let (x, y) = (coord("x")?, coord("y")?);
    if !engine.app.grid().rect().contains(x, y) {
        return Err(RpcError::bad_params("outside the screen"));
    }
    let (id, pos) = engine
        .frames
        .at(x, y)
        .ok_or_else(|| RpcError::bad_params("outside the screen"))?;
    Ok(json!({"frame": id, "pos": [pos.x, pos.y]}))
}

/// Refuse a sprite on a frame id that does not exist.
fn check_frame(engine: &ScreenEngine, sel: &FrameSel) -> Result<(), RpcError> {
    match sel {
        FrameSel::Id(id) if engine.frames.info(*id).is_none() => Err(ScreenEngine::no_frame(id)),
        _ => Ok(()),
    }
}

fn scene_err(e: ptyrc_core::scene::SceneError) -> RpcError {
    let code = match e.code() {
        "no-frame" => ErrorCode::NoFrame,
        "no-sprite" => ErrorCode::NoSprite,
        _ => ErrorCode::BadParams,
    };
    RpcError::new(code, e.to_string())
}

fn draw_put(engine: &mut ScreenEngine, conn: ConnId, params: Value, now: Duration) -> Res {
    let spec: SpriteSpec = parse(params)?;
    check_frame(engine, &spec.frame)?;
    let id = engine.scene.add(conn.0, spec, now).map_err(scene_err)?;
    Ok(json!({"id": id}))
}

fn draw_update(engine: &mut ScreenEngine, mut params: Value, now: Duration) -> Res {
    let id = params
        .as_object_mut()
        .and_then(|o| o.remove("id"))
        .and_then(|v| v.as_u64())
        .ok_or_else(|| RpcError::bad_params("id: required"))?;
    let patch: SpritePatch = parse(params)?;
    if let Some(sel) = &patch.frame {
        check_frame(engine, sel)?;
    }
    engine.scene.update(id, patch, now).map_err(scene_err)?;
    Ok(json!({}))
}

fn draw_ext(engine: &mut ScreenEngine, conn: ConnId, params: Value, now: Duration) -> Res {
    let Value::Object(mut fields) = params else {
        return Err(RpcError::bad_params("an object is required"));
    };
    let name = fields
        .remove("name")
        .and_then(|v| v.as_str().map(String::from));
    let function = fields
        .remove("function")
        .and_then(|v| v.as_str().map(String::from));
    let (Some(name), Some(function)) = (name, function) else {
        return Err(RpcError::bad_params("name and function are required"));
    };
    let ext_params = fields.remove("params").unwrap_or_else(|| json!({}));
    let ext = engine
        .exts
        .iter()
        .find(|e| e.name() == name)
        .ok_or_else(|| RpcError::bad_params(format!("no draw extension {name}")))?;
    let spec = ext
        .draw(&function, &ext_params)
        .map_err(RpcError::bad_params)?;
    // The remaining members are sprite fields laid over what the
    // extension made: frame, anchor, z, blend...
    let mut merged =
        serde_json::to_value(&spec).map_err(|e| RpcError::bad_params(e.to_string()))?;
    if let Value::Object(target) = &mut merged {
        target.retain(|_, v| !v.is_null());
        target.extend(fields);
    }
    draw_put(engine, conn, merged, now)
}

fn draw_exts(engine: &ScreenEngine) -> Res {
    let exts: Vec<Value> = engine
        .exts
        .iter()
        .map(|e| json!({"name": e.name(), "functions": e.functions()}))
        .collect();
    Ok(json!({"exts": exts, "effects": engine.scene.effects()}))
}

fn set_overlay(engine: &mut ScreenEngine, params: Value, now: Duration) -> Res {
    let what = params
        .get("what")
        .and_then(Value::as_str)
        .unwrap_or("frames");
    engine.overlay = match what {
        "frames" => Overlay::Frames,
        "heat" => Overlay::Heat,
        "off" => Overlay::Off,
        other => return Err(RpcError::bad_params(format!("what: {other}"))),
    };
    refresh_overlay(engine, now);
    Ok(json!({}))
}

const HEAT_BG: [u8; 10] = [52, 88, 124, 130, 136, 100, 64, 28, 22, 22];

/// Rebuild the sprites of the debug overlay from the frames as they are.
pub(crate) fn refresh_overlay(engine: &mut ScreenEngine, now: Duration) {
    engine.scene.clear(Some(DEBUG_OWNER));
    if engine.overlay == Overlay::Off {
        return;
    }
    let infos: Vec<_> = engine
        .frames
        .list()
        .iter()
        .filter_map(|f| engine.frames.info(f.id).map(|s| (s, f.support.clone())))
        .filter(|(s, _)| s.state != FrameState::Inactive && s.kind != FrameKind::Raw)
        .collect();
    for (info, support) in infos {
        if info.rect.w < 4 || info.rect.h == 0 {
            continue;
        }
        let (bg, fg) = match (info.kind, info.state) {
            (FrameKind::Chrome, _) => ("bright-black", "white"),
            (_, FrameState::Stable) => ("green", "black"),
            _ => ("yellow", "black"),
        };
        if engine.overlay == Overlay::Heat
            && info.kind == FrameKind::Pane
            && let Some(heat) = engine.frames.heat(info.id)
        {
            let rows: Vec<Value> = heat
                .digits()
                .iter()
                .map(|row| {
                    let runs: Vec<Value> = row
                        .bytes()
                        .map(|d| {
                            let level = (d.saturating_sub(b'0') as usize).min(9);
                            json!({"t": "·", "bg": HEAT_BG[level]})
                        })
                        .collect();
                    Value::Array(runs)
                })
                .collect();
            let spec = json!({
                "frame": info.id, "anchor": {"at": [0, 0], "space": "view"},
                "content": {"cells": rows}, "blend": "tint", "z": 9000, "persist": true,
            });
            if let Ok(spec) = serde_json::from_value::<SpriteSpec>(spec) {
                let _ = engine.scene.add(DEBUG_OWNER, spec, now);
            }
        }
        let mut label = format!(
            " #{} {}x{}+{},{} ",
            info.id, info.rect.w, info.rect.h, info.origin.x, info.origin.y
        );
        if let Some(support) = support {
            label.push_str(&support);
            label.push(' ');
        }
        let label: String = label.chars().take(info.rect.w as usize).collect();
        let spec = json!({
            "frame": info.id, "anchor": {"at": [-1, 0], "space": "view"},
            "content": {"rows": [label], "style": {"fg": fg, "bg": bg}, "opaque": true},
            "z": 9001, "persist": true,
        });
        if let Ok(spec) = serde_json::from_value::<SpriteSpec>(spec) {
            // The label goes on the raw frame: it must show on candidates
            // too, which are not usable yet.
            let mut spec = spec;
            let x = info.rect.right() as i64 - label_width(&spec) as i64;
            spec.frame = FrameSel::Id(RAW);
            spec.anchor = serde_json::from_value(
                json!({"at": [x.max(info.rect.x as i64), info.rect.y], "space": "view"}),
            )
            .unwrap_or(spec.anchor);
            let _ = engine.scene.add(DEBUG_OWNER, spec, now);
        }
    }
}

fn label_width(spec: &SpriteSpec) -> usize {
    spec.content
        .as_ref()
        .and_then(|c| c.rows.as_ref())
        .and_then(|r| r.first())
        .map_or(0, |r| r.chars().count())
}

fn debug_dump(engine: &ScreenEngine) -> Res {
    let cursor = engine.app.cursor();
    Ok(json!({
        "size": [engine.app.cols(), engine.app.rows()],
        "app": engine.app.grid().text(),
        "host": engine.host.grid().text(),
        "same_state": engine.host.same_state(&engine.app),
        "in_ground": engine.app.in_ground(),
        "cursor": {"x": cursor.col, "y": cursor.row, "visible": cursor.visible,
                   "pending_wrap": cursor.pending_wrap},
        "alt_screen": engine.app.modes().alt_screen,
        "title": engine.app.title(),
        "sprites": engine.scene.list(),
        "frames": engine.frames.list(),
        "injected_bytes": engine.injected,
        "waits": engine.waits.len(),
    }))
}

pub(crate) fn handle(
    engine: &mut ScreenEngine,
    conn: ConnId,
    token: Token,
    method: &str,
    params: Value,
    ctx: &mut Ctx,
) -> Reply {
    let now = ctx.now();
    let result = match method {
        "screen.text" => screen_text(engine, params),
        "screen.cells" => screen_cells(engine, params),
        "screen.find" => screen_find(engine, params),
        "screen.cursor" => screen_cursor(engine, params),
        "screen.wait" => return screen_wait(engine, conn, token, params, ctx),
        "frame.list" => {
            Ok(json!({"frames": engine.frames.list(), "layout": engine.frames.layout()}))
        }
        "frame.text" => frame_text(engine, params),
        "frame.heat" => frame_heat(engine, params),
        "frame.at" => frame_at(engine, params),
        "draw.put" => draw_put(engine, conn, params, now),
        "draw.update" => draw_update(engine, params, now),
        "draw.remove" => match params.get("id").and_then(Value::as_u64) {
            None => Err(RpcError::bad_params("id: required")),
            Some(id) if engine.scene.remove(id) => Ok(json!({})),
            Some(id) => Err(RpcError::new(
                ErrorCode::NoSprite,
                format!("no such sprite: {id}"),
            )),
        },
        "draw.clear" => {
            let all = params.get("all").and_then(Value::as_bool).unwrap_or(false);
            let removed = if all {
                // Everything but the debug overlay.
                let ids: Vec<_> = engine
                    .scene
                    .list()
                    .iter()
                    .filter(|s| s.owner != DEBUG_OWNER)
                    .map(|s| s.id)
                    .collect();
                ids.iter().filter(|id| engine.scene.remove(**id)).count()
            } else {
                engine.scene.clear(Some(conn.0))
            };
            Ok(json!({"removed": removed}))
        }
        "draw.list" => Ok(json!({"sprites": engine.scene.list()})),
        "draw.ext" => draw_ext(engine, conn, params, now),
        "draw.exts" => draw_exts(engine),
        "debug.overlay" => set_overlay(engine, params, now),
        "debug.dump" => debug_dump(engine),
        _ => return Reply::Err(RpcError::unknown_method(method)),
    };
    if method.starts_with("draw.") || method == "debug.overlay" {
        ctx.request_repaint();
    }
    match result {
        Ok(value) => Reply::Ok(value),
        Err(e) => Reply::Err(e),
    }
}
