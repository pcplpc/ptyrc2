// SPDX-License-Identifier: GPL-3.0-or-later
//! Hands on a session: `attach`, `grab`, `click`, `shot`.

use crate::{Common, Fail, connect, print_json};
use clap::Args;
use ptyrc_core::scene::ColorSpec;
use ptyrc_proto::ClientError;
use ptyrc_server::host::Host;
use ptyrc_server::keys;
use ptyrc_vt::{Color, Rgb, palette};
use serde_json::{Value, json};
use std::fmt::Write as _;
use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

type Done = Result<ExitCode, Fail>;

fn frame_value(frame: &str) -> Value {
    frame
        .parse::<u32>()
        .map_or_else(|_| json!(frame), |id| json!(id))
}

/// The bytes of a key that is not text: a detach key, an escape key.
fn special_key(what: &str, key: &str) -> Result<Vec<u8>, Fail> {
    match keys::encode_key(key, false) {
        Ok(bytes) if key.chars().count() > 1 => Ok(bytes),
        _ => Err(Fail::usage(
            format!("{what}: not a key: {key}"),
            "a named key that is not text, as ctrl+t or f12",
        )),
    }
}

/// The private modes of the app that this terminal takes while attached:
/// its keys and its mouse are then written as the app wants them.
const MIRRORED: [u16; 7] = [1, 9, 1000, 1002, 1003, 1006, 2004];

/// The modes of `MIRRORED` that are on, from `session.info`.
fn modes_on(info: &Value) -> Vec<u16> {
    let modes = &info["modes"];
    let mut on = Vec::new();
    if modes["app_cursor"] == true {
        on.push(1);
    }
    let mouse = modes["mouse"].as_array().into_iter().flatten();
    on.extend(mouse.filter_map(|m| m.as_u64()).map(|m| m as u16));
    if modes["mouse_sgr"] == true {
        on.push(1006);
    }
    if modes["bracketed_paste"] == true {
        on.push(2004);
    }
    on
}

#[derive(Args)]
pub struct Attach {
    /// The key that leaves; it is not sent [ctrl+t when not given]
    #[arg(long, value_name = "KEY", default_value = "ctrl+t")]
    detach: String,
    /// Give the session the size of this terminal, and again when this
    /// terminal is resized (headless sessions)
    #[arg(long)]
    resize: bool,
    /// Show the sprites too
    #[arg(long)]
    sprites: bool,
}

/// The remote screen here, and the keys of this terminal there.
///
/// The remote size is left alone unless `--resize`; a larger remote
/// screen is cut at this terminal's size. This terminal is given the
/// cursor-key, mouse and paste modes of the app, looked at with each
/// redraw: arrows, clicks and pastes are typed as the app reads them.
pub fn attach(common: &Common, args: Attach) -> Done {
    let detach = special_key("--detach", &args.detach)?;
    if !std::io::stdin().is_terminal() {
        return Err(Fail::usage(
            "attach needs a terminal on stdin",
            "to watch only: ptyrc show; to type from a script: ptyrc send, ptyrc keys",
        ));
    }
    let mut client = connect(common)?;
    let mut typist = connect(common)?;
    let name = client.hello()["session"].as_str().unwrap_or("").to_string();
    // Raw mode; restored when `host` goes, and on a panic.
    let host = Host::terminal()?;
    let mut local = host.size().unwrap_or((80, 24));
    let mut mirrored: Vec<u16> = Vec::new();
    if args.resize {
        client.call("session.resize", json!({"size": [local.0, local.1]}))?;
    }
    client.subscribe(&["screen", "cursor", "size", "exit"])?;

    let leave = Arc::new(AtomicBool::new(false));
    let flag = leave.clone();
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        let mut buf = [0u8; 4096];
        loop {
            let n = match stdin.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let typed = &buf[..n];
            let at = typed.windows(detach.len()).position(|w| w == detach);
            let typed = &typed[..at.unwrap_or(n)];
            if !typed.is_empty() {
                let bytes = keys::base64_encode(typed);
                if typist.call("input.send", json!({"bytes": bytes})).is_err() {
                    break;
                }
            }
            if at.is_some() {
                break;
            }
        }
        flag.store(true, Ordering::Relaxed);
    });

    let mut out = std::io::stdout().lock();
    let _ = out.write_all(b"\x1b[?1049h");
    let result = (|| -> Result<bool, Fail> {
        loop {
            // This terminal was resized: the cut follows, and the
            // session with --resize.
            let now = host.size().unwrap_or(local);
            if now != local {
                local = now;
                if args.resize {
                    client.call("session.resize", json!({"size": [local.0, local.1]}))?;
                }
            }
            let info = client.call("session.info", json!({}))?;
            let size = info["size"].clone();
            let wanted = modes_on(&info);
            let mut frame = String::new();
            if wanted != mirrored {
                for mode in MIRRORED {
                    let set = if wanted.contains(&mode) { 'h' } else { 'l' };
                    let _ = write!(frame, "\x1b[?{mode}{set}");
                }
                mirrored = wanted;
            }
            let cut = |i: usize, local: u16| size[i].as_u64().unwrap_or(1).min(local as u64);
            let rect = [0, 0, cut(0, local.0), cut(1, local.1)];
            let screen = client.call(
                "screen.text",
                json!({"ansi": true, "rect": rect, "sprites": args.sprites}),
            )?;
            let cursor = client.call("screen.cursor", json!({}))?;
            frame.push_str("\x1b[?25l\x1b[H");
            let lines = screen["lines"].as_array().map_or(&[][..], Vec::as_slice);
            for (i, line) in lines.iter().enumerate() {
                frame.push_str(line.as_str().unwrap_or(""));
                frame.push_str("\x1b[0m\x1b[K");
                if i + 1 < lines.len() {
                    frame.push_str("\r\n");
                }
            }
            frame.push_str("\x1b[J");
            let (x, y) = (cursor["x"].as_u64(), cursor["y"].as_u64());
            if let (Some(x), Some(y), Some(true)) = (x, y, cursor["visible"].as_bool())
                && x < rect[2]
                && y < rect[3]
            {
                let _ = write!(frame, "\x1b[{};{}H\x1b[?25h", y + 1, x + 1);
            }
            out.write_all(frame.as_bytes()).and_then(|_| out.flush())?;
            // Wait for a change, then let a burst of them pass.
            let mut changed = false;
            let mut quiet = 0;
            while !changed {
                if leave.load(Ordering::Relaxed) {
                    return Ok(false);
                }
                match client.next_event(Some(Duration::from_millis(50))) {
                    Ok(Some(event)) if event.event == "exit" => return Ok(true),
                    Ok(Some(_)) => changed = true,
                    // Sprites move without an event; so does the size
                    // of this terminal.
                    Ok(None) => {
                        quiet += 1;
                        changed = args.sprites || host.size().is_some_and(|s| s != local);
                        // A mode set with nothing drawn comes with no
                        // event: looked at four times a second.
                        if !changed && quiet % 5 == 0 {
                            let info = client.call("session.info", json!({}))?;
                            changed = modes_on(&info) != mirrored;
                        }
                    }
                    Err(ClientError::Closed) => return Ok(true),
                    Err(e) => return Err(e.into()),
                }
            }
            while let Ok(Some(event)) = client.next_event(Some(Duration::from_millis(10))) {
                if event.event == "exit" {
                    return Ok(true);
                }
            }
        }
    })();
    for mode in MIRRORED {
        let _ = write!(out, "\x1b[?{mode}l");
    }
    let _ = out.write_all(b"\x1b[?25h\x1b[?1049l");
    let _ = out.flush();
    drop(host);
    let over = result?;
    eprintln!(
        "ptyrc: {} {name}",
        if over {
            "the session ended:"
        } else {
            "detached from"
        }
    );
    Ok(ExitCode::SUCCESS)
}

#[derive(Args)]
pub struct Grab {
    /// Keys to take [default: all]
    keys: Vec<String>,
    /// The key that always releases; it reaches nobody
    #[arg(long, value_name = "KEY", default_value = "ctrl+t")]
    escape: String,
}

/// Take keys from the host keyboard of the session and print them, one
/// JSON object per line, until released.
pub fn grab(common: &Common, args: Grab) -> Done {
    let mut client = connect(common)?;
    let keys = if args.keys.is_empty() {
        Value::Null
    } else {
        json!(args.keys)
    };
    client.call("input.grab", json!({"keys": keys, "escape": args.escape}))?;
    client.subscribe(&["exit"])?;
    loop {
        match client.next_event(None) {
            Ok(Some(event)) if event.event == "input" => {
                print_json(&event.data, false);
                if !event.data["released"].is_null() {
                    return Ok(ExitCode::SUCCESS);
                }
            }
            Ok(Some(event)) if event.event == "exit" => return Ok(ExitCode::SUCCESS),
            Ok(_) => {}
            Err(ClientError::Closed) => return Ok(ExitCode::SUCCESS),
            Err(e) => return Err(e.into()),
        }
    }
}

#[derive(Args)]
pub struct Click {
    /// Screen cell `X,Y`, or with --text a pattern to click on
    target: String,
    /// TARGET is text on the screen: its first cell is clicked
    #[arg(short, long)]
    text: bool,
    /// With --text: TARGET is a regular expression
    #[arg(short = 'E', long, requires = "text")]
    regex: bool,
    /// With --text: the match to click, from 0
    #[arg(long, default_value_t = 0, requires = "text")]
    nth: usize,
    /// left, middle, right, wheel-up, wheel-down, wheel-left, wheel-right
    #[arg(short, long, default_value = "left")]
    button: String,
    /// Drag: release on this screen cell
    #[arg(long, value_name = "X,Y")]
    to: Option<String>,
    /// A modifier held: shift, alt, ctrl (repeatable)
    #[arg(short, long = "mod", value_name = "NAME")]
    mods: Vec<String>,
    /// Clicks in a row: 2 is a double click
    #[arg(long, default_value_t = 1, value_name = "N")]
    count: u8,
}

fn cell_of(text: &str) -> Result<(u16, u16), Fail> {
    let cell = text
        .split_once(',')
        .and_then(|(x, y)| Some((x.trim().parse::<u16>().ok()?, y.trim().parse::<u16>().ok()?)));
    cell.ok_or_else(|| {
        Fail::usage(
            format!("not a cell: {text}"),
            "X,Y from 0, as in 12,3; text on the screen with --text",
        )
    })
}

pub fn click(common: &Common, args: Click) -> Done {
    let mut client = connect(common)?;
    let cell = if args.text {
        let found = client.call(
            "screen.find",
            json!({"pattern": args.target, "regex": args.regex}),
        )?;
        let on_screen: Vec<&Value> = found["matches"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|m| &m["screen"])
            .filter(|s| !s.is_null())
            .collect();
        match on_screen.get(args.nth) {
            Some(cell) => (*cell).clone(),
            None => {
                return Err(Fail {
                    status: 1,
                    message: format!(
                        "{}: {} on screen, no match {}",
                        args.target,
                        on_screen.len(),
                        args.nth
                    ),
                    hint: None,
                });
            }
        }
    } else {
        let (x, y) = cell_of(&args.target)?;
        json!([x, y])
    };
    let mut params = json!({"x": cell[0], "y": cell[1], "button": args.button,
                            "mods": args.mods, "count": args.count});
    if let Some(to) = &args.to {
        let (x, y) = cell_of(to)?;
        params["to"] = json!([x, y]);
    }
    client.call("input.click", params)?;
    if common.json {
        print_json(&json!({"x": cell[0], "y": cell[1]}), false);
    }
    Ok(ExitCode::SUCCESS)
}

#[derive(Args)]
pub struct Shot {
    /// An SVG image [default]
    #[arg(long, group = "format")]
    svg: bool,
    /// Text with escape sequences: `cat` shows it
    #[arg(long, group = "format")]
    ansi: bool,
    /// An HTML fragment: one `<pre>`
    #[arg(long, group = "format")]
    html: bool,
    /// Frame id or name: raw, active, largest, quietest, idle
    #[arg(short, long, default_value = "raw")]
    frame: String,
    /// What the host shows: the sprites over the app
    #[arg(long)]
    sprites: bool,
    /// Leave the cursor out (--svg, --html: a visible cursor is an
    /// inverse cell)
    #[arg(long)]
    no_cursor: bool,
    /// Write there in place of stdout
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,
}

pub fn shot(common: &Common, args: Shot) -> Done {
    let mut client = connect(common)?;
    let view = json!({"frame": frame_value(&args.frame), "sprites": args.sprites});
    let text = if args.ansi {
        let mut view = view;
        view["ansi"] = json!(true);
        let screen = client.call("screen.text", view)?;
        let mut text = String::new();
        for line in screen["lines"].as_array().into_iter().flatten() {
            text.push_str(line.as_str().unwrap_or(""));
            text.push_str("\x1b[0m\n");
        }
        text
    } else {
        let info = client.call("session.info", json!({}))?;
        let host = |key: &str, default: Rgb| {
            info["host_colors"][key]
                .as_str()
                .and_then(|s| palette::parse_color_spec(s.as_bytes()))
                .unwrap_or(default)
        };
        let theme = Theme {
            fg: host("fg", [0xe5, 0xe5, 0xe5]),
            bg: host("bg", [0, 0, 0]),
        };
        let cells = client.call("screen.cells", view)?;
        let mut rows = picture(&cells["rows"], theme);
        if !args.no_cursor {
            let cursor = client.call("screen.cursor", json!({}))?;
            let cell = |key: &str, i: usize| {
                let origin = cells["rect"][i].as_u64()?;
                (cursor[key].as_u64()? as usize).checked_sub(origin as usize)
            };
            if let (Some(true), Some(x), Some(y)) =
                (cursor["visible"].as_bool(), cell("x", 0), cell("y", 1))
            {
                mark_cursor(&mut rows, x, y, theme);
            }
        }
        let cols = cells["size"][0].as_u64().unwrap_or(0) as usize;
        if args.html {
            html(&rows, theme)
        } else {
            svg(&rows, cols, theme)
        }
    };
    match args.output {
        Some(path) => std::fs::write(&path, text).map_err(|e| Fail {
            status: 5,
            message: format!("{}: {e}", path.display()),
            hint: None,
        })?,
        None => std::io::stdout().lock().write_all(text.as_bytes())?,
    }
    Ok(ExitCode::SUCCESS)
}

/// Default colours of the picture: the host's.
#[derive(Clone, Copy)]
struct Theme {
    fg: Rgb,
    bg: Rgb,
}

/// A run of cells drawn alike.
#[derive(Debug, PartialEq)]
struct Piece {
    /// First cell and width in cells.
    x: usize,
    w: usize,
    text: String,
    fg: Rgb,
    /// `None`: the default background.
    bg: Option<Rgb>,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    strike: bool,
}

/// The rows of `screen.cells` as pieces: colours resolved, inverse
/// applied, a wide glyph alone in its piece.
fn picture(rows: &Value, theme: Theme) -> Vec<Vec<Piece>> {
    let color = |value: &Value| -> Option<Rgb> {
        let spec: ColorSpec = serde_json::from_value(value.clone()).ok()?;
        match spec.0 {
            Color::Default => None,
            color => Some(palette::rgb(color, theme.fg)),
        }
    };
    let flag = |run: &Value, key: &str| run[key].as_bool().unwrap_or(false);
    let mut out = Vec::new();
    for row in rows.as_array().into_iter().flatten() {
        let mut pieces: Vec<Piece> = Vec::new();
        let mut x = 0;
        for run in row.as_array().into_iter().flatten() {
            let (mut fg, mut bg) = (color(&run["fg"]), color(&run["bg"]));
            if flag(run, "inverse") {
                (fg, bg) = (Some(bg.unwrap_or(theme.bg)), Some(fg.unwrap_or(theme.fg)));
            }
            let piece = |x: usize, w: usize, text: String| Piece {
                x,
                w,
                text,
                fg: fg.unwrap_or(theme.fg),
                bg,
                bold: flag(run, "bold"),
                dim: flag(run, "dim"),
                italic: flag(run, "italic"),
                underline: flag(run, "underline"),
                strike: flag(run, "strike"),
            };
            let mut narrow = String::new();
            let mut start = x;
            let mut chars = run["t"].as_str().unwrap_or("").chars().peekable();
            while let Some(c) = chars.next() {
                let mut glyph = String::from(c);
                // Combining marks stay with their base.
                while let Some(mark) = chars.peek().filter(|m| ptyrc_vt::char_width(**m) == 0) {
                    glyph.push(*mark);
                    chars.next();
                }
                let w = ptyrc_vt::char_width(c).max(1);
                if w == 1 {
                    narrow.push_str(&glyph);
                } else {
                    if !narrow.is_empty() {
                        pieces.push(piece(start, x - start, std::mem::take(&mut narrow)));
                    }
                    pieces.push(piece(x, w, glyph));
                    start = x + w;
                }
                x += w;
            }
            if !narrow.is_empty() {
                pieces.push(piece(start, x - start, narrow));
            }
        }
        out.push(pieces);
    }
    out
}

/// The cursor: the cell at `(x, y)` of the picture in inverse, alone in
/// its piece. Outside the picture: nothing.
fn mark_cursor(rows: &mut [Vec<Piece>], x: usize, y: usize, theme: Theme) {
    let Some(row) = rows.get_mut(y) else {
        return;
    };
    let Some(index) = row.iter().position(|p| p.x <= x && x < p.x + p.w) else {
        return;
    };
    let piece = row.remove(index);
    // Glyphs of the piece with their cells: a mark stays with its base.
    let mut glyphs: Vec<(usize, String)> = Vec::new();
    for c in piece.text.chars() {
        match (ptyrc_vt::char_width(c), glyphs.last_mut()) {
            (0, Some((_, glyph))) => glyph.push(c),
            (w, _) => glyphs.push((w.max(1), String::from(c))),
        }
    }
    let part = |x: usize, w: usize, text: String, cursor: bool| Piece {
        x,
        w,
        text,
        fg: if cursor {
            piece.bg.unwrap_or(theme.bg)
        } else {
            piece.fg
        },
        bg: if cursor { Some(piece.fg) } else { piece.bg },
        bold: piece.bold,
        dim: piece.dim && !cursor,
        italic: piece.italic,
        underline: piece.underline,
        strike: piece.strike,
    };
    let mut parts = Vec::new();
    let (mut at, mut start, mut text) = (piece.x, piece.x, String::new());
    for (w, glyph) in glyphs {
        if at <= x && x < at + w {
            if !text.is_empty() {
                parts.push(part(start, at - start, std::mem::take(&mut text), false));
            }
            parts.push(part(at, w, glyph, true));
            start = at + w;
        } else {
            text.push_str(&glyph);
        }
        at += w;
    }
    if !text.is_empty() {
        parts.push(part(start, at - start, text, false));
    }
    row.splice(index..index, parts);
}

fn hex([r, g, b]: Rgb) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

const FONT: &str = "DejaVu Sans Mono, Menlo, Consolas, Liberation Mono, monospace";
/// Cell of the SVG, in pixels, for a 14 px font.
const CELL: (f64, f64) = (8.4, 18.0);

fn svg(rows: &[Vec<Piece>], cols: usize, theme: Theme) -> String {
    let (cw, ch) = CELL;
    let (width, height) = (cols as f64 * cw, rows.len() as f64 * ch);
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width:.1}\" height=\"{height:.1}\" \
         viewBox=\"0 0 {width:.1} {height:.1}\" font-family=\"{FONT}\" font-size=\"14\">\n\
         <rect width=\"100%\" height=\"100%\" fill=\"{}\"/>\n",
        hex(theme.bg)
    );
    for (y, row) in rows.iter().enumerate() {
        let top = y as f64 * ch;
        for piece in row {
            if let Some(bg) = piece.bg {
                let _ = writeln!(
                    out,
                    "<rect x=\"{:.1}\" y=\"{top:.1}\" width=\"{:.1}\" height=\"{ch:.1}\" fill=\"{}\"/>",
                    piece.x as f64 * cw,
                    piece.w as f64 * cw,
                    hex(bg)
                );
            }
        }
        for piece in row.iter().filter(|p| !p.text.trim_matches(' ').is_empty()) {
            // Blanks at the ends carry nothing: the text starts at its ink.
            let text = piece.text.trim_matches(' ');
            let lead = piece.text.len() - piece.text.trim_start_matches(' ').len();
            let trail = piece.text.len() - piece.text.trim_end_matches(' ').len();
            let cells = piece.w - lead - trail;
            let mut attrs = String::new();
            if piece.bold {
                attrs.push_str(" font-weight=\"bold\"");
            }
            if piece.italic {
                attrs.push_str(" font-style=\"italic\"");
            }
            if piece.dim {
                attrs.push_str(" opacity=\"0.6\"");
            }
            match (piece.underline, piece.strike) {
                (true, true) => attrs.push_str(" text-decoration=\"underline line-through\""),
                (true, false) => attrs.push_str(" text-decoration=\"underline\""),
                (false, true) => attrs.push_str(" text-decoration=\"line-through\""),
                (false, false) => {}
            }
            let _ = writeln!(
                out,
                "<text x=\"{:.1}\" y=\"{:.1}\" textLength=\"{:.1}\" lengthAdjust=\"spacingAndGlyphs\" \
                 fill=\"{}\"{attrs} xml:space=\"preserve\">{}</text>",
                (piece.x + lead) as f64 * cw,
                top + 13.5,
                cells as f64 * cw,
                hex(piece.fg),
                escape(text)
            );
        }
    }
    out.push_str("</svg>\n");
    out
}

fn html(rows: &[Vec<Piece>], theme: Theme) -> String {
    let mut out = format!(
        "<pre style=\"background:{};color:{};font-family:{FONT};line-height:1.25\">",
        hex(theme.bg),
        hex(theme.fg)
    );
    for row in rows {
        // Trailing blanks without a background are not written.
        let last = row
            .iter()
            .rposition(|p| p.bg.is_some() || !p.text.trim().is_empty());
        let shown = &row[..last.map_or(0, |i| i + 1)];
        for (i, piece) in shown.iter().enumerate() {
            let text = match piece.bg {
                None if i + 1 == shown.len() => piece.text.trim_end_matches(' '),
                _ => &piece.text,
            };
            let mut style = String::new();
            if piece.fg != theme.fg {
                let _ = write!(style, "color:{};", hex(piece.fg));
            }
            if let Some(bg) = piece.bg {
                let _ = write!(style, "background:{};", hex(bg));
            }
            for (on, css) in [
                (piece.bold, "font-weight:bold;"),
                (piece.dim, "opacity:0.6;"),
                (piece.italic, "font-style:italic;"),
                (
                    piece.underline && !piece.strike,
                    "text-decoration:underline;",
                ),
                (
                    !piece.underline && piece.strike,
                    "text-decoration:line-through;",
                ),
                (
                    piece.underline && piece.strike,
                    "text-decoration:underline line-through;",
                ),
            ] {
                if on {
                    style.push_str(css);
                }
            }
            if style.is_empty() {
                out.push_str(&escape(text));
            } else {
                let _ = write!(
                    out,
                    "<span style=\"{style}\">{}</span>",
                    escape(&piece.text)
                );
            }
        }
        out.push('\n');
    }
    out.push_str("</pre>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const THEME: Theme = Theme {
        fg: [0xe5, 0xe5, 0xe5],
        bg: [0, 0, 0],
    };

    fn rows() -> Value {
        json!([
            [{"t": "a<b ", "fg": "red", "bold": true}, {"t": "日x", "bg": 21}, {"t": "  "}],
            [{"t": " inv", "inverse": true}, {"t": "é́", "fg": "#102030", "underline": true}],
        ])
    }

    #[test]
    fn pieces() {
        let rows = picture(&rows(), THEME);
        let shape: Vec<Vec<(usize, usize, &str)>> = rows
            .iter()
            .map(|r| r.iter().map(|p| (p.x, p.w, p.text.as_str())).collect())
            .collect();
        assert_eq!(
            shape,
            [
                vec![(0, 4, "a<b "), (4, 2, "日"), (6, 1, "x"), (7, 2, "  ")],
                vec![(0, 4, " inv"), (4, 1, "é́")],
            ]
        );
        assert_eq!(rows[0][0].fg, palette::xterm(1));
        assert_eq!(rows[0][1].bg, Some(palette::xterm(21)));
        // Inverse: the default colours change places.
        assert_eq!((rows[1][0].fg, rows[1][0].bg), (THEME.bg, Some(THEME.fg)));
        assert_eq!(rows[1][1].fg, [0x10, 0x20, 0x30]);
    }

    #[test]
    fn the_cursor_is_an_inverse_cell() {
        let mut rows = picture(&rows(), THEME);
        mark_cursor(&mut rows, 1, 0, THEME);
        mark_cursor(&mut rows, 5, 0, THEME);
        mark_cursor(&mut rows, 30, 0, THEME);
        mark_cursor(&mut rows, 0, 9, THEME);
        let shape: Vec<(usize, usize, &str)> = rows[0]
            .iter()
            .map(|p| (p.x, p.w, p.text.as_str()))
            .collect();
        assert_eq!(
            shape,
            [
                (0, 1, "a"),
                (1, 1, "<"),
                (2, 2, "b "),
                (4, 2, "日"),
                (6, 1, "x"),
                (7, 2, "  ")
            ]
        );
        let red = palette::xterm(1);
        assert_eq!((rows[0][1].fg, rows[0][1].bg), (THEME.bg, Some(red)));
        assert_eq!((rows[0][0].fg, rows[0][0].bg), (red, None));
        // On a wide glyph: both its cells.
        assert_eq!(rows[0][3].fg, palette::xterm(21));
        assert_eq!(rows[0][3].bg, Some(THEME.fg));
        // A mark stays with its base.
        mark_cursor(&mut rows, 4, 1, THEME);
        assert_eq!(rows[1][1].text, "é́");
        assert!(rows[1][1].bg.is_some());
    }

    #[test]
    fn svg_and_html() {
        let rows = picture(&rows(), THEME);
        let image = svg(&rows, 9, THEME);
        assert!(image.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"75.6\""));
        assert!(image.contains("height=\"36.0\""));
        assert!(image.contains(
            "<text x=\"0.0\" y=\"13.5\" textLength=\"25.2\" lengthAdjust=\"spacingAndGlyphs\" \
             fill=\"#cd0000\" font-weight=\"bold\" xml:space=\"preserve\">a&lt;b</text>"
        ));
        assert!(image.contains("<rect x=\"33.6\" y=\"0.0\" width=\"16.8\" height=\"18.0\""));
        // The inverse run starts at its ink, one cell in.
        assert!(image.contains("<text x=\"8.4\" y=\"31.5\" textLength=\"25.2\""));
        assert_eq!(image.matches("<text").count(), 5);
        let page = html(&rows, THEME);
        assert!(page.contains("<span style=\"color:#cd0000;font-weight:bold;\">a&lt;b </span>"));
        assert!(page.contains("text-decoration:underline;"));
        assert_eq!(page.lines().count(), 3);
        assert!(!page.contains("  \n"));
    }
}
