// SPDX-License-Identifier: GPL-3.0-or-later
//! Client verbs on the screen, the frames and the scene.

use crate::{Common, call, connect, print_json};
use clap::{Args, Subcommand};
use ptyrc_proto::ClientError;
use serde_json::{Map, Value, json};
use std::io::Write;
use std::process::ExitCode;
use std::time::Duration;

type Done = Result<ExitCode, String>;

fn frame_value(frame: &str) -> Value {
    frame
        .parse::<u32>()
        .map_or_else(|_| json!(frame), |id| json!(id))
}

fn strings(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

#[derive(Args)]
pub struct Text {
    /// Frame id or name: raw, active, largest, quietest, idle
    #[arg(short, long, default_value = "raw")]
    frame: String,
    /// Keep colours and attributes
    #[arg(long)]
    ansi: bool,
}

pub fn text(common: &Common, args: Text) -> Done {
    let result = call(
        common,
        "screen.text",
        json!({"frame": frame_value(&args.frame), "ansi": args.ansi}),
    )?;
    if common.json {
        print_json(&result, false);
    } else {
        for line in strings(&result["lines"]) {
            println!("{line}{}", if args.ansi { "\x1b[0m" } else { "" });
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[derive(Args)]
pub struct Find {
    /// PATTERN is a regular expression
    #[arg(short = 'E', long)]
    regex: bool,
    #[arg(short, long)]
    ignore_case: bool,
    /// Also search what the frame remembers outside its view
    #[arg(short, long)]
    known: bool,
    #[arg(short, long, default_value = "raw")]
    frame: String,
    pattern: String,
}

/// Exit status 1 without a match, as grep.
pub fn find(common: &Common, args: Find) -> Done {
    let result = call(
        common,
        "screen.find",
        json!({
            "pattern": args.pattern, "regex": args.regex, "ignore_case": args.ignore_case,
            "known": args.known, "frame": frame_value(&args.frame),
        }),
    )?;
    let matches = result["matches"].as_array().cloned().unwrap_or_default();
    if common.json {
        print_json(&result, false);
    } else {
        for m in &matches {
            let screen = match m["screen"].as_array() {
                Some(s) => format!("screen {},{}", s[0], s[1]),
                None => "off screen".to_string(),
            };
            println!(
                "frame {} at {},{}  {}  {}",
                m["frame"],
                m["x"],
                m["y"],
                screen,
                m["text"].as_str().unwrap_or("")
            );
        }
    }
    Ok(if matches.is_empty() {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

pub fn cursor(common: &Common) -> Done {
    let result = call(common, "screen.cursor", json!({}))?;
    if common.json {
        print_json(&result, false);
    } else {
        println!(
            "{},{}{}  frame {}",
            result["x"],
            result["y"],
            if result["visible"] == false {
                " hidden"
            } else {
                ""
            },
            result["frame"]
        );
    }
    Ok(ExitCode::SUCCESS)
}

#[derive(Args)]
pub struct Wait {
    /// Text to wait for
    pattern: Option<String>,
    #[arg(short = 'E', long)]
    regex: bool,
    /// Wait until PATTERN is no longer shown
    #[arg(long, requires = "pattern")]
    gone: bool,
    /// Wait until the screen has not changed for MS milliseconds
    #[arg(long, value_name = "MS")]
    stable: Option<u64>,
    /// Give up after MS milliseconds
    #[arg(long, value_name = "MS", default_value_t = 5000)]
    timeout: u64,
    #[arg(short, long, default_value = "raw")]
    frame: String,
}

/// Exit status 1 on timeout.
pub fn wait(common: &Common, args: Wait) -> Done {
    if args.pattern.is_none() && args.stable.is_none() {
        return Err("wait: a PATTERN or --stable is required".into());
    }
    let mut params = Map::new();
    params.insert("frame".into(), frame_value(&args.frame));
    params.insert("timeout_ms".into(), json!(args.timeout));
    if let Some(pattern) = args.pattern {
        params.insert("pattern".into(), json!(pattern));
        params.insert("regex".into(), json!(args.regex));
        params.insert("gone".into(), json!(args.gone));
    }
    if let Some(ms) = args.stable {
        params.insert("stable_ms".into(), json!(ms));
    }
    let result = call(common, "screen.wait", Value::Object(params))?;
    if common.json {
        print_json(&result, false);
    }
    Ok(if result["matched"] == true {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn print_frames(result: &Value) {
    println!(
        "{:>4}  {:<6}  {:<9}  {:<14}  {:<12}  {:<13}  {:<5}  KEY",
        "ID", "KIND", "STATE", "RECT", "ORIGIN", "EXTENT", "IDLE"
    );
    for f in result["frames"].as_array().into_iter().flatten() {
        let r = &f["rect"];
        let state = format!(
            "{}{}",
            f["state"].as_str().unwrap_or("?"),
            if f["usable"] == true { "" } else { "-" }
        );
        let support = f["support"]
            .as_str()
            .map(|s| format!(" [{s}]"))
            .unwrap_or_default();
        let idle = format!(
            "{:.0}%{}",
            f["idle"].as_f64().unwrap_or(0.0) * 100.0,
            if f["is_idle"] == true { "*" } else { "" }
        );
        println!(
            "{:>4}  {:<6}  {:<9}  {:<14}  {:<12}  {:<13}  {:<5}  {}{}",
            f["id"].to_string(),
            f["kind"].as_str().unwrap_or("?"),
            state,
            format!("{}x{}+{}+{}", r[2], r[3], r[0], r[1]),
            format!("{},{}", f["origin"][0], f["origin"][1]),
            format!("{}..{}", f["extent"][0], f["extent"][1]),
            idle,
            f["key"].as_str().unwrap_or("-"),
            support,
        );
    }
}

#[derive(Args)]
pub struct Frames {
    /// Print again each time the frames change
    #[arg(short, long)]
    watch: bool,
    /// Include frames remembered but not on screen
    #[arg(short, long)]
    all: bool,
}

pub fn frames(common: &Common, args: Frames) -> Done {
    let mut client = connect(common)?;
    let show = |result: &mut Value| {
        if !args.all
            && let Some(list) = result["frames"].as_array_mut()
        {
            list.retain(|f| f["state"] != "inactive");
        }
        if common.json {
            print_json(result, false)
        } else {
            print_frames(result)
        }
    };
    let mut result = client
        .call("frame.list", json!({}))
        .map_err(|e| e.to_string())?;
    show(&mut result);
    if !args.watch {
        return Ok(ExitCode::SUCCESS);
    }
    client
        .subscribe(&["frames", "exit"])
        .map_err(|e| e.to_string())?;
    loop {
        match client.next_event(None) {
            Ok(Some(event)) if event.event == "frames" => {
                println!();
                let mut data = event.data;
                show(&mut data);
            }
            Ok(Some(event)) if event.event == "exit" => return Ok(ExitCode::SUCCESS),
            Ok(_) => {}
            Err(ClientError::Closed) => return Ok(ExitCode::SUCCESS),
            Err(e) => return Err(e.to_string()),
        }
    }
}

pub fn heat(common: &Common, frame: &str, classes: bool) -> Done {
    let result = call(common, "frame.heat", json!({"frame": frame_value(frame)}))?;
    if common.json {
        print_json(&result, false);
    } else {
        let rows = if classes { "classes" } else { "rows" };
        for line in strings(&result[rows]) {
            println!("{line}");
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Exit status 0 when the frame is idle, 1 otherwise.
pub fn idle(common: &Common, frame: &str) -> Done {
    let result = call(common, "frame.idle", json!({"frame": frame_value(frame)}))?;
    let is_idle = result["idle"] == true;
    if common.json {
        print_json(&result, false);
    } else {
        println!(
            "{:.2} {} (free {:.2})",
            result["score"].as_f64().unwrap_or(0.0),
            if is_idle { "idle" } else { "busy" },
            result["free"].as_f64().unwrap_or(0.0),
        );
    }
    Ok(if is_idle {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

/// Where and how a sprite goes.
#[derive(Args)]
pub struct Place {
    #[arg(short, long, default_value = "raw")]
    frame: String,
    /// Position X,Y; negative counts from the right and bottom (view)
    #[arg(long, value_name = "X,Y", allow_hyphen_values = true)]
    at: Option<String>,
    /// Space of --at: view (fixed on screen) or frame (moves with scrolling)
    #[arg(long, default_value = "view")]
    space: String,
    /// Anchor next to this text
    #[arg(long, value_name = "TEXT")]
    near: Option<String>,
    /// Side of --near: right, left, above, below, over
    #[arg(long, default_value = "right")]
    side: String,
    /// Let the driver choose a quiet spot: top-right, bottom, any...
    #[arg(long, value_name = "PREFER")]
    auto: Option<String>,
    /// Follow the cursor at offset DX,DY
    #[arg(long, value_name = "DX,DY", allow_hyphen_values = true)]
    cursor: Option<String>,
    /// over, behind (text stays on top) or tint
    #[arg(long, default_value = "over")]
    blend: String,
    #[arg(short, default_value_t = 0, allow_hyphen_values = true)]
    z: i32,
    /// Remove after MS milliseconds
    #[arg(long, value_name = "MS")]
    ttl: Option<u64>,
}

fn pair(text: &str) -> Result<(i64, i64), String> {
    text.split_once(',')
        .and_then(|(a, b)| Some((a.trim().parse().ok()?, b.trim().parse().ok()?)))
        .ok_or_else(|| format!("expected X,Y: {text}"))
}

impl Place {
    /// Sprite fields. A sprite made by the CLI outlives its connection.
    fn fields(&self) -> Result<Map<String, Value>, String> {
        let anchor = if let Some(text) = &self.near {
            json!({"text": text, "side": self.side})
        } else if let Some(prefer) = &self.auto {
            json!({"auto": {"prefer": prefer}})
        } else if let Some(offset) = &self.cursor {
            let (dx, dy) = pair(offset)?;
            json!({"cursor": {"dx": dx, "dy": dy}})
        } else {
            let (x, y) = pair(self.at.as_deref().unwrap_or("0,0"))?;
            json!({"at": [x, y], "space": self.space})
        };
        let mut fields = Map::new();
        fields.insert("frame".into(), frame_value(&self.frame));
        fields.insert("anchor".into(), anchor);
        fields.insert("blend".into(), json!(self.blend));
        fields.insert("z".into(), json!(self.z));
        fields.insert("persist".into(), json!(true));
        if let Some(ttl) = self.ttl {
            fields.insert("ttl_ms".into(), json!(ttl));
        }
        Ok(fields)
    }
}

#[derive(Args)]
pub struct Paint {
    #[arg(long)]
    fg: Option<String>,
    #[arg(long)]
    bg: Option<String>,
    #[arg(long)]
    bold: bool,
}

fn color(text: &str) -> Value {
    text.parse::<u8>()
        .map_or_else(|_| json!(text), |n| json!(n))
}

impl Paint {
    fn style(&self) -> Map<String, Value> {
        let mut style = Map::new();
        if let Some(fg) = &self.fg {
            style.insert("fg".into(), color(fg));
        }
        if let Some(bg) = &self.bg {
            style.insert("bg".into(), color(bg));
        }
        if self.bold {
            style.insert("bold".into(), json!(true));
        }
        style
    }
}

#[derive(Subcommand)]
pub enum Draw {
    /// Draw text; each argument is a row
    Text {
        #[command(flatten)]
        place: Place,
        #[command(flatten)]
        paint: Paint,
        /// Spaces hide what is below
        #[arg(long)]
        opaque: bool,
        #[arg(required = true)]
        rows: Vec<String>,
    },
    /// Draw with an extension: `ptyrc draw ext shapes circle '{"r":4}'`
    Ext {
        #[command(flatten)]
        place: Place,
        name: String,
        function: String,
        /// Parameters of the function, a JSON object
        params: Option<String>,
    },
    /// Start a native effect: `ptyrc draw effect confetti`
    Effect {
        #[command(flatten)]
        place: Place,
        name: String,
        /// Parameters of the effect, a JSON object
        params: Option<String>,
    },
    /// List draw extensions and effects
    Exts,
    /// Remove sprites by id
    Rm {
        #[arg(required = true)]
        ids: Vec<u64>,
    },
    /// List sprites
    Ls,
    /// Remove every sprite
    Clear,
}

fn json_arg(text: Option<String>) -> Result<Value, String> {
    match text {
        Some(text) => serde_json::from_str(&text).map_err(|e| format!("params: {e}")),
        None => Ok(json!({})),
    }
}

fn report_id(common: &Common, result: Value) -> Done {
    if common.json {
        print_json(&result, false)
    } else {
        println!("{}", result["id"])
    }
    Ok(ExitCode::SUCCESS)
}

pub fn draw(common: &Common, verb: Draw) -> Done {
    match verb {
        Draw::Text {
            place,
            paint,
            opaque,
            rows,
        } => {
            let mut spec = place.fields()?;
            spec.insert(
                "content".into(),
                json!({"rows": rows, "style": paint.style(), "opaque": opaque}),
            );
            report_id(common, call(common, "draw.put", Value::Object(spec))?)
        }
        Draw::Ext {
            place,
            name,
            function,
            params,
        } => {
            let mut spec = place.fields()?;
            spec.insert("name".into(), json!(name));
            spec.insert("function".into(), json!(function));
            spec.insert("params".into(), json_arg(params)?);
            report_id(common, call(common, "draw.ext", Value::Object(spec))?)
        }
        Draw::Effect {
            place,
            name,
            params,
        } => {
            let mut spec = place.fields()?;
            spec.insert(
                "effect".into(),
                json!({"name": name, "params": json_arg(params)?}),
            );
            report_id(common, call(common, "draw.put", Value::Object(spec))?)
        }
        Draw::Exts => {
            let result = call(common, "draw.exts", json!({}))?;
            if common.json {
                print_json(&result, false);
                return Ok(ExitCode::SUCCESS);
            }
            for ext in result["exts"].as_array().into_iter().flatten() {
                for f in ext["functions"].as_array().into_iter().flatten() {
                    println!(
                        "{} {}  ({})  {}",
                        ext["name"].as_str().unwrap_or("?"),
                        f["name"].as_str().unwrap_or("?"),
                        f["params"].as_str().unwrap_or(""),
                        f["doc"].as_str().unwrap_or("")
                    );
                }
            }
            println!("effects: {}", strings(&result["effects"]).join(", "));
            Ok(ExitCode::SUCCESS)
        }
        Draw::Rm { ids } => {
            let mut client = connect(common)?;
            for id in ids {
                client
                    .call("draw.remove", json!({"id": id}))
                    .map_err(|e| e.to_string())?;
            }
            Ok(ExitCode::SUCCESS)
        }
        Draw::Ls => {
            let result = call(common, "draw.list", json!({}))?;
            if common.json {
                print_json(&result, false);
                return Ok(ExitCode::SUCCESS);
            }
            for s in result["sprites"].as_array().into_iter().flatten() {
                let screen = match s["screen"].as_array() {
                    Some(p) => format!("{},{}", p[0], p[1]),
                    None => "hidden".into(),
                };
                println!(
                    "{:>5}  frame {:<4}  {}x{}  {:<8}  z{}  {}",
                    s["id"].to_string(),
                    s["frame"].to_string(),
                    s["size"][0],
                    s["size"][1],
                    screen,
                    s["z"],
                    s["kind"].as_str().unwrap_or("")
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        Draw::Clear => {
            let result = call(common, "draw.clear", json!({"all": true}))?;
            if common.json {
                print_json(&result, false);
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// Live mirror of the remote screen, until the session ends or ^C.
pub fn show(common: &Common) -> Done {
    let mut client = connect(common)?;
    client
        .subscribe(&["screen", "cursor", "size", "exit"])
        .map_err(|e| e.to_string())?;
    let mut out = std::io::stdout().lock();
    // Alternate screen, cursor hidden; restored on the way out.
    let _ = out.write_all(b"\x1b[?1049h\x1b[?25l");
    let result = (|| -> Result<(), String> {
        loop {
            let screen = client
                .call("screen.text", json!({"ansi": true}))
                .map_err(|e| e.to_string())?;
            let mut frame = String::from("\x1b[H");
            for line in strings(&screen["lines"]) {
                frame.push_str(line);
                frame.push_str("\x1b[0m\x1b[K\r\n");
            }
            frame.push_str("\x1b[J");
            out.write_all(frame.as_bytes())
                .and_then(|_| out.flush())
                .map_err(|e| e.to_string())?;
            // Wait for a change, then let a burst of them pass.
            loop {
                match client.next_event(None) {
                    Ok(Some(event)) if event.event == "exit" => return Ok(()),
                    Ok(Some(_)) => break,
                    Ok(None) => {}
                    Err(ClientError::Closed) => return Ok(()),
                    Err(e) => return Err(e.to_string()),
                }
            }
            while let Ok(Some(event)) = client.next_event(Some(Duration::from_millis(15))) {
                if event.event == "exit" {
                    return Ok(());
                }
            }
        }
    })();
    let _ = out.write_all(b"\x1b[?25h\x1b[?1049l");
    let _ = out.flush();
    result.map(|_| ExitCode::SUCCESS)
}

#[derive(Subcommand)]
pub enum Debug {
    /// Show on the host what the driver sees: frames, heat; off to hide
    Overlay {
        #[arg(default_value = "frames")]
        what: String,
    },
    /// Internal state as JSON
    Dump,
}

pub fn debug(common: &Common, verb: Debug) -> Done {
    match verb {
        Debug::Overlay { what } => {
            call(common, "debug.overlay", json!({"what": what}))?;
        }
        Debug::Dump => print_json(&call(common, "debug.dump", json!({}))?, !common.json),
    }
    Ok(ExitCode::SUCCESS)
}
