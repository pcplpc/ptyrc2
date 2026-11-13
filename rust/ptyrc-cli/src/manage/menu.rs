// SPDX-License-Identifier: GPL-3.0-or-later
//! The menu of `ptyrc manage`: keys in, rows of styled runs out.
//!
//! No clock, no I/O. A front gives it the keys (named as `input.grab`
//! names them), the apps, the catalog; it applies the actions returned
//! and draws `render(w, h)`.

use super::catalog::{App, Catalog, Kind};
use serde_json::Value;

/// How a run is drawn. `fg`: a colour name of the wire.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub fg: Option<&'static str>,
    pub bold: bool,
    pub dim: bool,
    pub inverse: bool,
}

const PLAIN: Style = Style {
    fg: None,
    bold: false,
    dim: false,
    inverse: false,
};
const BOLD: Style = Style {
    bold: true,
    ..PLAIN
};
const DIM: Style = Style { dim: true, ..PLAIN };
const SELECTED: Style = Style {
    inverse: true,
    bold: true,
    ..PLAIN
};
const CURSOR: Style = Style {
    inverse: true,
    ..PLAIN
};

fn fg(name: &'static str) -> Style {
    Style {
        fg: Some(name),
        ..PLAIN
    }
}

/// Cells drawn alike.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub style: Style,
}

pub type Row = Vec<Run>;

fn run(text: impl Into<String>, style: Style) -> Run {
    Run {
        text: text.into(),
        style,
    }
}

fn len(text: &str) -> usize {
    text.chars().count()
}

fn row_len(row: &[Run]) -> usize {
    row.iter().map(|r| len(&r.text)).sum()
}

/// `text` in at most `width` cells, `…` at the cut.
fn cut(text: &str, width: usize) -> String {
    if len(text) <= width {
        return text.to_string();
    }
    let mut out: String = text.chars().take(width.saturating_sub(1)).collect();
    if width > 0 {
        out.push('…');
    }
    out
}

/// `row` in exactly `width` cells: cut with `…`, padded with `pad`.
fn fit(row: Row, width: usize, pad: Style) -> Row {
    let mut out = Row::new();
    let mut used = 0;
    let total = row_len(&row);
    for piece in row {
        let room = width - used;
        if room == 0 {
            break;
        }
        let n = len(&piece.text);
        if n <= room && (used + n < width || total <= width) {
            used += n;
            out.push(piece);
        } else {
            out.push(run(cut(&piece.text, room), piece.style));
            used = width;
        }
    }
    if used < width {
        out.push(run(" ".repeat(width - used), pad));
    }
    out
}

/// A row drawn as selected: one inverse bar.
fn selected(row: Row) -> Row {
    row.into_iter().map(|r| run(r.text, SELECTED)).collect()
}

/// The text of rows, a line each: what tests and logs read.
#[cfg(test)]
pub fn text(rows: &[Row]) -> String {
    let line = |row: &Row| row.iter().map(|r| r.text.as_str()).collect::<String>();
    rows.iter().map(line).collect::<Vec<_>>().join("\n")
}

/// State of an app of the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Running,
    Paused,
    Exited,
    Gone,
}

/// An app of the driver's table (`app.list`, topic `apps`).
#[derive(Clone, Debug, PartialEq)]
pub struct AppInfo {
    pub id: u64,
    /// Started by the driver; else a client connected from elsewhere.
    pub child: bool,
    pub name: String,
    pub argv: Vec<String>,
    pub pid: u64,
    pub state: State,
    pub status: Option<i64>,
    /// Seconds in this state.
    pub since: f64,
    pub me: bool,
    pub log: Option<String>,
    /// The frames its sprites and widgets are on.
    pub frames: Vec<u64>,
    /// It changes frame while it runs (topic `move`).
    pub moves: bool,
    /// As `manage::label`, `manage::options` (how many), `manage::is_manager`.
    label: String,
    options: usize,
    manager: bool,
}

impl AppInfo {
    pub fn from_json(value: &Value) -> Option<AppInfo> {
        let state = match value["state"].as_str()? {
            "running" => State::Running,
            "paused" => State::Paused,
            "exited" => State::Exited,
            "gone" => State::Gone,
            _ => return None,
        };
        let argv = value["argv"].as_array().map_or(&[][..], Vec::as_slice);
        Some(AppInfo {
            id: value["id"].as_u64()?,
            child: value["kind"].as_str() != Some("client"),
            name: value["name"].as_str().unwrap_or("").to_string(),
            argv: (argv.iter())
                .map(|a| a.as_str().unwrap_or("").to_string())
                .collect(),
            pid: value["pid"].as_u64().unwrap_or(0),
            state,
            status: value["status"].as_i64(),
            since: value["since"].as_f64().unwrap_or(0.0),
            me: value["me"].as_bool().unwrap_or(false),
            log: value["log"].as_str().map(str::to_string),
            frames: (value["frames"].as_array().into_iter().flatten())
                .filter_map(Value::as_u64)
                .collect(),
            moves: value["moves"].as_bool().unwrap_or(false),
            label: super::label(value),
            options: super::options(value).len(),
            manager: super::is_manager(value),
        })
    }

    /// How many words of `argv` come before the options: the program,
    /// and the name of an app run by `ptyrc-app`.
    fn head(&self) -> usize {
        self.argv.len() - self.options.min(self.argv.len())
    }

    /// The name shown (`manage::label`).
    pub fn label(&self) -> String {
        self.label.clone()
    }

    /// The arguments after the name (`manage::options`).
    pub fn args(&self) -> &[String] {
        &self.argv[self.head()..]
    }

    /// It changes frame as it runs: only while it runs.
    fn moves_live(&self) -> bool {
        self.moves && self.state == State::Running
    }

    /// The manager itself, this one or another: not shown.
    pub fn is_manager(&self) -> bool {
        self.me || self.manager
    }
}

/// A frame of the session (`frame.list`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameInfo {
    pub id: u64,
    /// `raw`, `pane`.
    pub kind: String,
    /// On the screen: `[x, y, w, h]`.
    pub rect: [usize; 4],
    /// Its key, else the preset that knows it.
    pub what: String,
    pub idle: bool,
}

impl FrameInfo {
    /// A frame one draws on: on screen, not chrome.
    pub fn from_json(value: &Value) -> Option<FrameInfo> {
        let kind = value["kind"].as_str()?;
        if kind == "chrome" || value["state"].as_str() == Some("inactive") {
            return None;
        }
        let cell = |i: usize| value["rect"][i].as_u64().map(|v| v as usize);
        let what = value["key"].as_str().filter(|k| !k.is_empty());
        Some(FrameInfo {
            id: value["id"].as_u64()?,
            kind: kind.to_string(),
            rect: [cell(0)?, cell(1)?, cell(2)?, cell(3)?],
            what: what.or(value["support"].as_str()).unwrap_or("").to_string(),
            idle: value["is_idle"].as_bool().unwrap_or(false),
        })
    }
}

/// Names of frames an app may be given in place of an id.
pub const FRAME_NAMES: [&str; 5] = ["raw", "active", "largest", "quietest", "idle"];

/// What a front does for the menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// An app of the catalog: `ptyrc-app NAME ARGS...`.
    Start {
        name: String,
        args: Vec<String>,
    },
    Stop(u64),
    Pause(u64),
    Resume(u64),
    /// `argv`: the whole new command; `None`: as before.
    Restart {
        id: u64,
        argv: Option<Vec<String>>,
    },
    Forget(u64),
    /// `app.move`: it changes frame as it runs.
    Move {
        id: u64,
        frame: String,
    },
    /// Show this frame on the screen itself; `None`: no more.
    Highlight(Option<u64>),
    /// Read its log, then `Menu::log`.
    Log(u64),
    /// The session chosen.
    Session(String),
    Quit,
}

/// Split a typed line into words: blanks, `'...'`, `"..."`, `\x`.
pub fn words(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word: Option<String> = None;
    let mut quote = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => word.get_or_insert_default().push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                word.get_or_insert_default();
            }
            (None, '\\') => word
                .get_or_insert_default()
                .extend(chars.next().or(Some('\\'))),
            (None, c) if c.is_whitespace() => out.extend(word.take()),
            (None, c) => word.get_or_insert_default().push(c),
        }
    }
    out.extend(word);
    out
}

/// Words as a line `words` reads back.
pub fn line_of(words: &[String]) -> String {
    let quoted = |w: &String| {
        if w.is_empty() || w.chars().any(|c| c.is_whitespace() || "'\"\\".contains(c)) {
            format!("'{}'", w.replace('\'', "'\\''"))
        } else {
            w.clone()
        }
    };
    words.iter().map(quoted).collect::<Vec<_>>().join(" ")
}

/// A typed line and its cursor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Line {
    text: Vec<char>,
    at: usize,
}

impl Line {
    fn new(text: &str) -> Line {
        let text: Vec<char> = text.chars().collect();
        Line {
            at: text.len(),
            text,
        }
    }

    fn string(&self) -> String {
        self.text.iter().collect()
    }

    /// A key of the editor. False: not one of its keys.
    fn key(&mut self, key: &str) -> bool {
        match key {
            "left" => self.at = self.at.saturating_sub(1),
            "right" => self.at = (self.at + 1).min(self.text.len()),
            "home" | "ctrl+a" => self.at = 0,
            "end" | "ctrl+e" => self.at = self.text.len(),
            "backspace" | "ctrl+h" if self.at > 0 => {
                self.at -= 1;
                self.text.remove(self.at);
            }
            "backspace" | "ctrl+h" => {}
            "delete" | "ctrl+d" if self.at < self.text.len() => {
                self.text.remove(self.at);
            }
            "delete" | "ctrl+d" => {}
            "ctrl+u" => {
                self.text.drain(..self.at);
                self.at = 0;
            }
            "ctrl+k" => self.text.truncate(self.at),
            "space" => self.insert(' '),
            _ => {
                let mut chars = key.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) if !c.is_control() => self.insert(c),
                    _ => return false,
                }
            }
        }
        true
    }

    fn insert(&mut self, c: char) {
        self.text.insert(self.at, c);
        self.at += 1;
    }

    /// The line in `width` cells, the cursor shown when `editing`.
    fn runs(&self, width: usize, editing: bool, style: Style) -> Row {
        if !editing {
            return vec![run(cut(&self.string(), width), style)];
        }
        // A window that holds the cursor, one cell kept for it at the end.
        let width = width.max(1);
        let start = (self.at + 1).saturating_sub(width);
        let shown = |range: std::ops::Range<usize>| -> String {
            let end = range.end.min(self.text.len());
            let from = range.start.min(end);
            self.text[from..end].iter().collect()
        };
        let under = self.text.get(self.at).copied().unwrap_or(' ');
        vec![
            run(shown(start..self.at), style),
            run(under.to_string(), CURSOR),
            run(shown(self.at + 1..start + width), style),
        ]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum FieldKind {
    /// The row that starts.
    Submit,
    Flag {
        on: bool,
    },
    Count {
        n: usize,
    },
    Choice {
        choices: Vec<String>,
        at: Option<usize>,
    },
    Value {
        line: Line,
    },
    /// Positional words, typed.
    Words {
        line: Line,
    },
    /// Whatever else, typed.
    More {
        line: Line,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Field {
    /// The flags, as `-r, --rules`; the name of a positional.
    label: String,
    /// The flag written.
    flag: String,
    flags: Vec<String>,
    default: Option<String>,
    help: String,
    kind: FieldKind,
}

impl Field {
    fn line(&mut self) -> Option<&mut Line> {
        match &mut self.kind {
            FieldKind::Value { line } | FieldKind::Words { line } | FieldKind::More { line } => {
                Some(line)
            }
            _ => None,
        }
    }
}

/// What a form is for.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Target {
    Start,
    /// The words of the old command kept: the program, the name.
    Restart {
        id: u64,
        head: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Form {
    target: Target,
    name: String,
    fields: Vec<Field>,
    sel: usize,
    editing: bool,
}

impl Form {
    fn new(target: Target, name: &str, app: Option<&App>) -> Form {
        let plain = |label: &str, help: &str, kind| Field {
            label: label.into(),
            flag: String::new(),
            flags: Vec::new(),
            default: None,
            help: help.into(),
            kind,
        };
        let verb = match target {
            Target::Start => "start",
            Target::Restart { .. } => "restart",
        };
        let mut fields = vec![plain(
            &format!("{verb} {name}"),
            "enter: with the options below",
            FieldKind::Submit,
        )];
        let (options, positionals) = match app {
            Some(app) => (&app.options[..], &app.positionals[..]),
            None => (&[][..], &[][..]),
        };
        // The frame is every app's: after its own options.
        let last = |o: &&super::catalog::Opt| o.flags.iter().any(|f| f == "--frame");
        for option in (options.iter().filter(|o| !last(o))).chain(options.iter().filter(last)) {
            let kind = match option.kind {
                Kind::Flag => FieldKind::Flag { on: false },
                Kind::Count => FieldKind::Count { n: 0 },
                Kind::Choice => FieldKind::Choice {
                    choices: option.choices.clone(),
                    at: None,
                },
                Kind::Value => FieldKind::Value {
                    line: Line::default(),
                },
            };
            fields.push(Field {
                label: option.flags.join(", "),
                flag: option.flags.last().cloned().unwrap_or_default(),
                flags: option.flags.clone(),
                default: option.default.clone(),
                help: option.help.clone(),
                kind,
            });
        }
        for positional in positionals {
            let line = Line::default();
            fields.push(plain(
                &positional.name,
                &positional.help,
                FieldKind::Words { line },
            ));
        }
        fields.push(plain(
            "more arguments",
            "added as typed, after the options",
            FieldKind::More {
                line: Line::default(),
            },
        ));
        Form {
            target,
            name: name.into(),
            fields,
            sel: 0,
            editing: false,
        }
    }

    fn field_of(&self, flag: &str) -> Option<usize> {
        self.fields
            .iter()
            .position(|f| f.flags.iter().any(|x| x == flag))
    }

    /// Fill from the arguments of a running app. What is not known goes
    /// to `more arguments`.
    fn fill(&mut self, args: &[String]) {
        let mut more: Vec<String> = Vec::new();
        let mut positional: Vec<String> = Vec::new();
        let has_positional =
            (self.fields.iter()).any(|f| matches!(f.kind, FieldKind::Words { .. }));
        let mut i = 0;
        while i < args.len() {
            let word = &args[i];
            i += 1;
            let (flag, glued) = match word.split_once('=') {
                Some((flag, value)) if flag.starts_with("--") => (flag, Some(value)),
                _ => (word.as_str(), None),
            };
            if let Some(at) = self.field_of(flag) {
                let next = glued.map(str::to_string).or_else(|| args.get(i).cloned());
                let field = &mut self.fields[at];
                match &mut field.kind {
                    FieldKind::Flag { on } if glued.is_none() => *on = true,
                    FieldKind::Count { n } if glued.is_none() => *n += 1,
                    FieldKind::Value { line } if next.is_some() => {
                        *line = Line::new(&next.unwrap_or_default());
                        i += usize::from(glued.is_none());
                    }
                    FieldKind::Choice { choices, at }
                        if next.as_ref().is_some_and(|v| choices.contains(v)) =>
                    {
                        *at = choices.iter().position(|c| Some(c) == next.as_ref());
                        i += usize::from(glued.is_none());
                    }
                    _ => more.push(word.clone()),
                }
                continue;
            }
            // `-ab`: short flags in one word.
            let cluster: Option<Vec<usize>> = (word.strip_prefix('-'))
                .filter(|rest| rest.len() > 1 && !rest.starts_with('-'))
                .and_then(|rest| {
                    rest.chars()
                        .map(|c| {
                            self.field_of(&format!("-{c}")).filter(|at| {
                                matches!(
                                    self.fields[*at].kind,
                                    FieldKind::Flag { .. } | FieldKind::Count { .. }
                                )
                            })
                        })
                        .collect()
                });
            match cluster {
                Some(fields) => {
                    for at in fields {
                        match &mut self.fields[at].kind {
                            FieldKind::Flag { on } => *on = true,
                            FieldKind::Count { n } => *n += 1,
                            _ => {}
                        }
                    }
                }
                None if has_positional && !word.starts_with('-') => positional.push(word.clone()),
                None => more.push(word.clone()),
            }
        }
        for field in &mut self.fields {
            match &mut field.kind {
                FieldKind::Words { line } => {
                    *line = Line::new(&line_of(&std::mem::take(&mut positional)))
                }
                FieldKind::More { line } => *line = Line::new(&line_of(&more)),
                _ => {}
            }
        }
    }

    /// The arguments the form stands for.
    fn args(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut tail = Vec::new();
        for field in &self.fields {
            match &field.kind {
                FieldKind::Submit => {}
                FieldKind::Flag { on } => out.extend(on.then(|| field.flag.clone())),
                FieldKind::Count { n } => out.extend(vec![field.flag.clone(); *n]),
                FieldKind::Choice { choices, at } => {
                    if let Some(choice) = at.and_then(|at| choices.get(at)) {
                        out.extend([field.flag.clone(), choice.clone()]);
                    }
                }
                FieldKind::Value { line } => {
                    let value = line.string();
                    if !value.trim().is_empty() {
                        out.extend([field.flag.clone(), value.trim().to_string()]);
                    }
                }
                FieldKind::Words { line } | FieldKind::More { line } => {
                    tail.extend(words(&line.string()))
                }
            }
        }
        out.extend(tail);
        out
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Act {
    Pause,
    Resume,
    Restart,
    Options,
    Move,
    Stop,
    Forget,
    Log,
}

impl Act {
    fn label(self, exited: bool) -> &'static str {
        match self {
            Act::Pause => "pause",
            Act::Resume => "resume",
            Act::Restart if exited => "start again",
            Act::Restart => "restart",
            Act::Options if exited => "start again with other options…",
            Act::Options => "restart with other options…",
            Act::Move => "move to a frame…",
            Act::Stop => "stop",
            Act::Forget => "forget",
            Act::Log => "log",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Act::Pause | Act::Resume => "p",
            Act::Restart => "r",
            Act::Options => "e",
            Act::Move => "m",
            Act::Stop | Act::Forget => "x",
            Act::Log => "l",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Screen {
    Sessions {
        names: Vec<String>,
        sel: usize,
    },
    Main {
        sel: usize,
        id: Option<u64>,
    },
    Actions {
        id: u64,
        sel: usize,
    },
    Start {
        sel: usize,
    },
    Form(Box<Form>),
    /// The frames; `pick`: chosen for that.
    Frames {
        sel: usize,
        pick: Option<Pick>,
    },
    /// The apps on a frame.
    FrameApps {
        frame: u64,
        sel: usize,
    },
    Log {
        title: String,
        lines: Vec<String>,
        top: usize,
    },
}

/// What a frame is picked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pick {
    /// The app moves there.
    Move(u64),
    /// The frame of the form below.
    Field,
}

/// A row of the frames screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    Frame(u64),
    Name(&'static str),
}

/// A row of the main list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    App(u64),
    Start,
    Random,
    Frames,
}

/// The menu.
#[derive(Clone, Debug)]
pub struct Menu {
    session: String,
    apps: Vec<AppInfo>,
    catalog: Option<Catalog>,
    stack: Vec<Screen>,
    note: Option<(String, bool)>,
    seed: u64,
    drawn: Option<String>,
    frames: Vec<FrameInfo>,
    /// The frame the front was last told to show.
    lit: Option<u64>,
}

fn since(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    match s {
        0..60 => format!("{s}s"),
        60..3600 => format!("{}m", s / 60),
        3600..172_800 => format!("{}h", s / 3600),
        _ => format!("{}d", s / 86400),
    }
}

/// Move a selection among `n` rows.
fn step(sel: &mut usize, n: usize, key: &str, page: usize) -> bool {
    let last = n.saturating_sub(1);
    *sel = match key {
        "up" | "k" | "shift+tab" => sel.saturating_sub(1),
        "down" | "j" | "tab" => (*sel + 1).min(last),
        "home" => 0,
        "end" => last,
        "pageup" => sel.saturating_sub(page),
        "pagedown" => (*sel + page).min(last),
        _ => return false,
    };
    true
}

/// The keys of a screen, for its foot: text, and how soon it is dropped.
type Keys<'a> = Vec<(&'a str, u8)>;

/// Rows of a page of a list.
const PAGE: usize = 5;

impl Menu {
    /// `session`: its name, for the title. `seed`: of `random app`.
    pub fn new(session: &str, seed: u64) -> Menu {
        Menu {
            session: session.into(),
            apps: Vec::new(),
            catalog: None,
            stack: vec![Screen::Main { sel: 0, id: None }],
            note: None,
            seed: seed | 1,
            drawn: None,
            frames: Vec::new(),
            lit: None,
        }
    }

    /// A menu that first asks which session.
    pub fn sessions(names: Vec<String>) -> Menu {
        let mut menu = Menu::new("", 1);
        menu.stack = vec![Screen::Sessions { names, sel: 0 }];
        menu
    }

    /// The table of the driver, whole.
    pub fn apps(&mut self, mut apps: Vec<AppInfo>) {
        apps.retain(|a| a.state != State::Gone);
        apps.sort_by_key(|a| a.id);
        self.apps = apps;
        self.follow();
    }

    /// One app changed: the data of an `apps` event.
    pub fn app(&mut self, info: AppInfo) {
        // `me` is told by `app.list` only.
        let me = self.apps.iter().any(|a| a.id == info.id && a.me);
        self.apps.retain(|a| a.id != info.id);
        if info.state != State::Gone {
            self.apps.push(AppInfo { me, ..info });
            self.apps.sort_by_key(|a| a.id);
        }
        self.follow();
    }

    /// Time passed: the apps have been that much longer in their state.
    pub fn elapsed(&mut self, seconds: f64) {
        for app in &mut self.apps {
            app.since += seconds;
        }
    }

    /// The apps that can be started. `None`: no `ptyrc-app`.
    pub fn catalog(&mut self, catalog: Option<Catalog>) {
        self.catalog = catalog;
        self.follow();
    }

    /// A line for the user, until the next key. `bad`: an error.
    pub fn note(&mut self, text: &str, bad: bool) {
        let line = text.lines().next().unwrap_or("").trim();
        self.note = (!line.is_empty()).then(|| (line.to_string(), bad));
    }

    /// The log asked by `Action::Log`.
    pub fn log(&mut self, id: u64, lines: Vec<String>) {
        let name = self.info(id).map_or_else(|| id.to_string(), AppInfo::label);
        let top = lines.len();
        self.stack.push(Screen::Log {
            title: format!("log of {name}"),
            lines,
            top,
        });
    }

    pub fn info(&self, id: u64) -> Option<&AppInfo> {
        self.apps.iter().find(|a| a.id == id)
    }

    fn shown(&self) -> Vec<&AppInfo> {
        self.apps.iter().filter(|a| !a.is_manager()).collect()
    }

    fn entries(&self) -> Vec<Entry> {
        let mut entries: Vec<Entry> = self.shown().iter().map(|a| Entry::App(a.id)).collect();
        if let Some(catalog) = &self.catalog {
            entries.push(Entry::Start);
            if catalog.apps.iter().any(|a| !a.random.is_empty()) {
                entries.push(Entry::Random);
            }
        }
        entries.push(Entry::Frames);
        entries
    }

    /// After the table changed: the selection stays on its app; screens
    /// of an app that left are closed.
    fn follow(&mut self) {
        let entries = self.entries();
        let known = |id: &u64| self.apps.iter().any(|a| a.id == *id);
        let closed = (self.stack.iter()).position(|s| match s {
            Screen::Actions { id, .. } => !known(id),
            Screen::Form(form) => match &form.target {
                Target::Restart { id, .. } => !known(id),
                Target::Start => false,
            },
            _ => false,
        });
        if let Some(at) = closed {
            self.stack.truncate(at.max(1));
        }
        for screen in &mut self.stack {
            if let Screen::Main { sel, id } = screen {
                let at = id.and_then(|id| entries.iter().position(|e| *e == Entry::App(id)));
                *sel = at.unwrap_or(*sel).min(entries.len().saturating_sub(1));
                *id = match entries.get(*sel) {
                    Some(Entry::App(id)) => Some(*id),
                    _ => None,
                };
            }
        }
    }

    fn acts(&self, id: u64) -> Vec<Act> {
        let Some(app) = self.info(id) else {
            return Vec::new();
        };
        let log = app.log.is_some().then_some(Act::Log);
        let mut acts = match (app.state, app.child) {
            (State::Running, true) => {
                vec![Act::Pause, Act::Restart, Act::Options, Act::Move, Act::Stop]
            }
            (State::Paused, true) => vec![
                Act::Resume,
                Act::Restart,
                Act::Options,
                Act::Move,
                Act::Stop,
            ],
            (State::Running, false) if app.moves => vec![Act::Pause, Act::Move, Act::Stop],
            (State::Running, false) => vec![Act::Pause, Act::Stop],
            (State::Paused, false) => vec![Act::Resume, Act::Stop],
            (_, true) => vec![Act::Restart, Act::Options, Act::Move, Act::Forget],
            (_, false) => Vec::new(),
        };
        acts.extend(log);
        acts
    }

    fn rand(&mut self) -> u64 {
        // xorshift64.
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        self.seed
    }

    /// An app of the catalog and one of its argument lists, never the
    /// one drawn last.
    fn random(&mut self) -> Option<Action> {
        let catalog = self.catalog.clone()?;
        let mut pool: Vec<&App> = catalog
            .apps
            .iter()
            .filter(|a| !a.random.is_empty())
            .collect();
        if pool.len() > 1 {
            pool.retain(|a| Some(&a.name) != self.drawn.as_ref());
        }
        let app = *pool.get(self.rand() as usize % pool.len().max(1))?;
        let args = app.random[self.rand() as usize % app.random.len()].clone();
        self.drawn = Some(app.name.clone());
        Some(Action::Start {
            name: app.name.clone(),
            args,
        })
    }

    /// Do `act` on an app. The screens above the main list are closed,
    /// unless a form opens.
    fn act(&mut self, id: u64, act: Act) -> Vec<Action> {
        let Some(app) = self.info(id).cloned() else {
            return Vec::new();
        };
        if !self.acts(id).contains(&act) {
            let why = match (act, app.child) {
                (Act::Restart | Act::Options, false) => {
                    "started outside the driver: stop it, then start it here"
                }
                (Act::Move, _) => "it does not move, and was not started here",
                (Act::Log, _) => "it has no log",
                (Act::Pause | Act::Resume, _) => "it has ended",
                _ => "not for this one",
            };
            self.note(&format!("{}: {why}", app.label()), true);
            return Vec::new();
        }
        self.stack.truncate(1);
        match act {
            Act::Pause => vec![Action::Pause(id)],
            Act::Resume => vec![Action::Resume(id)],
            Act::Restart => vec![Action::Restart { id, argv: None }],
            Act::Stop => vec![Action::Stop(id)],
            Act::Forget => vec![Action::Forget(id)],
            Act::Log => vec![Action::Log(id)],
            Act::Move => {
                // The picker opens on the frame the app is on.
                let rows = self.choices(true);
                let on = app.frames.first().map(|f| Choice::Frame(*f));
                let sel = rows.iter().position(|c| Some(*c) == on).unwrap_or(0);
                let pick = Some(Pick::Move(id));
                self.stack.push(Screen::Frames { sel, pick });
                Vec::new()
            }
            Act::Options => {
                let name = app.label();
                let known = self.catalog.as_ref().and_then(|c| c.app(&name));
                // Only an app run by `ptyrc-app` has the options of the catalog.
                let known = known.filter(|_| app.head() >= 2);
                let target = Target::Restart {
                    id,
                    head: app.argv[..app.head()].to_vec(),
                };
                let mut form = Form::new(target, &name, known);
                form.fill(app.args());
                self.stack.push(Screen::Form(Box::new(form)));
                Vec::new()
            }
        }
    }

    /// A key, named as `input.grab` names it: `up`, `enter`, `esc`,
    /// `space`, `a`... What the front is to do.
    pub fn key(&mut self, key: &str) -> Vec<Action> {
        // esc and a key in one read come as alt and the key: both are meant.
        let mut actions = match key.strip_prefix("alt+") {
            Some(then) if !then.is_empty() => {
                let mut actions = self.key_("esc");
                actions.extend(self.key_(then));
                actions
            }
            _ => self.key_(key),
        };
        actions.extend(self.light());
        actions
    }

    /// The frame to show on the screen changed: the action that says so.
    fn light(&mut self) -> Option<Action> {
        let lit = match self.stack.last() {
            Some(Screen::Frames { sel, pick }) => match self.choices(pick.is_some()).get(*sel) {
                Some(Choice::Frame(id)) => Some(*id),
                _ => None,
            },
            Some(Screen::FrameApps { frame, .. }) => Some(*frame),
            _ => None,
        };
        (lit != self.lit).then(|| {
            self.lit = lit;
            Action::Highlight(lit)
        })
    }

    /// The rows of the frames screen: the frames, then, to pick one,
    /// the names.
    fn choices(&self, pick: bool) -> Vec<Choice> {
        let mut rows: Vec<Choice> = self.frames.iter().map(|f| Choice::Frame(f.id)).collect();
        if pick {
            rows.extend(FRAME_NAMES.map(Choice::Name));
        }
        rows
    }

    fn apps_on(&self, frame: u64) -> Vec<&AppInfo> {
        let mut apps = self.shown();
        apps.retain(|a| a.frames.contains(&frame));
        apps
    }

    /// The frames of the session, on screen.
    pub fn frames(&mut self, mut frames: Vec<FrameInfo>) -> Option<Action> {
        frames.sort_by_key(|f| f.id);
        self.frames = frames;
        let known = |id: &u64| self.frames.iter().any(|f| f.id == *id);
        let closed = (self.stack.iter())
            .position(|s| matches!(s, Screen::FrameApps { frame, .. } if !known(frame)));
        if let Some(at) = closed {
            self.stack.truncate(at.max(1));
        }
        let rows = |pick: bool| self.frames.len() + if pick { FRAME_NAMES.len() } else { 0 };
        let last: Vec<usize> = (self.stack.iter())
            .map(|s| match s {
                Screen::Frames { pick, .. } => rows(pick.is_some()).saturating_sub(1),
                _ => 0,
            })
            .collect();
        for (screen, last) in self.stack.iter_mut().zip(last) {
            if let Screen::Frames { sel, .. } = screen {
                *sel = (*sel).min(last);
            }
        }
        self.light()
    }

    fn key_(&mut self, key: &str) -> Vec<Action> {
        self.note = None;
        let entries = self.entries();
        let back = matches!(key, "esc" | "q" | "left" | "h");
        let Some(screen) = self.stack.last_mut() else {
            return vec![Action::Quit];
        };
        match screen {
            Screen::Sessions { names, sel } => {
                if step(sel, names.len(), key, PAGE) {
                } else if key == "enter" {
                    let name = names.get(*sel).cloned();
                    return name.map(Action::Session).into_iter().collect();
                } else if matches!(key, "esc" | "q") {
                    return vec![Action::Quit];
                }
            }
            Screen::Main { sel, id } => {
                if step(sel, entries.len(), key, PAGE) {
                    *id = match entries.get(*sel) {
                        Some(Entry::App(id)) => Some(*id),
                        _ => None,
                    };
                    return Vec::new();
                }
                let entry = entries.get(*sel).copied();
                let on_app = |act: Act| match entry {
                    Some(Entry::App(id)) => Some((id, act)),
                    _ => None,
                };
                let wanted = match key {
                    "esc" | "q" => return vec![Action::Quit],
                    "s" if self.catalog.is_some() => {
                        self.stack.push(Screen::Start { sel: 0 });
                        return Vec::new();
                    }
                    "s" => {
                        self.note("ptyrc-app was not found: nothing to start", true);
                        return Vec::new();
                    }
                    "enter" | "right" => match entry {
                        Some(Entry::App(id)) => {
                            self.stack.push(Screen::Actions { id, sel: 0 });
                            return Vec::new();
                        }
                        Some(Entry::Start) => {
                            self.stack.push(Screen::Start { sel: 0 });
                            return Vec::new();
                        }
                        Some(Entry::Random) => return self.random().into_iter().collect(),
                        Some(Entry::Frames) => {
                            self.stack.push(Screen::Frames { sel: 0, pick: None });
                            return Vec::new();
                        }
                        None => return Vec::new(),
                    },
                    "p" | "space" => on_app(Act::Pause),
                    "r" => on_app(Act::Restart),
                    "e" => on_app(Act::Options),
                    "x" | "delete" => on_app(Act::Stop),
                    "l" => on_app(Act::Log),
                    "m" => on_app(Act::Move),
                    "f" => {
                        self.stack.push(Screen::Frames { sel: 0, pick: None });
                        return Vec::new();
                    }
                    _ => None,
                };
                if let Some((id, act)) = wanted {
                    // One key for both ways.
                    let act = match (act, self.info(id).map(|a| a.state)) {
                        (Act::Pause, Some(State::Paused)) => Act::Resume,
                        (Act::Stop, Some(State::Exited)) => Act::Forget,
                        (act, _) => act,
                    };
                    return self.act(id, act);
                }
            }
            Screen::Actions { id, sel } => {
                let id = *id;
                let at = *sel;
                let acts = self.acts(id);
                if let Some(Screen::Actions { sel, .. }) = self.stack.last_mut()
                    && step(sel, acts.len(), key, PAGE)
                {
                    return Vec::new();
                }
                if back {
                    self.stack.pop();
                } else if matches!(key, "enter" | "right" | "space") {
                    return acts.get(at).map_or_else(Vec::new, |act| self.act(id, *act));
                } else if let Some(act) = acts.iter().find(|a| a.key() == key) {
                    return self.act(id, *act);
                }
            }
            Screen::Start { sel } => {
                let apps = self.catalog.as_ref().map_or(&[][..], |c| &c.apps[..]);
                if step(sel, apps.len(), key, PAGE) {
                } else if back {
                    self.stack.pop();
                } else if matches!(key, "enter" | "right")
                    && let Some(app) = apps.get(*sel)
                {
                    let form = Form::new(Target::Start, &app.name, Some(app));
                    self.stack.push(Screen::Form(Box::new(form)));
                }
            }
            Screen::Form(_) => return self.form_key(key),
            Screen::Frames { sel, pick } => {
                let (at, pick) = (*sel, *pick);
                let rows = self.choices(pick.is_some());
                if let Some(Screen::Frames { sel, .. }) = self.stack.last_mut()
                    && step(sel, rows.len(), key, PAGE)
                {
                    return Vec::new();
                }
                if back {
                    self.stack.pop();
                    return Vec::new();
                }
                let (true, Some(choice)) = (matches!(key, "enter" | "right"), rows.get(at)) else {
                    return Vec::new();
                };
                let frame = match choice {
                    Choice::Frame(id) => id.to_string(),
                    Choice::Name(name) => name.to_string(),
                };
                match (pick, choice) {
                    (None, Choice::Frame(id)) => {
                        let frame = *id;
                        self.stack.push(Screen::FrameApps { frame, sel: 0 });
                    }
                    (None, Choice::Name(_)) => {}
                    (Some(Pick::Move(id)), _) => {
                        self.stack.truncate(1);
                        let Some(app) = self.info(id) else {
                            return Vec::new();
                        };
                        return vec![match app.moves_live() {
                            true => Action::Move { id, frame },
                            false => Action::Restart {
                                id,
                                argv: Some(super::with_frame(&app.argv, &frame)),
                            },
                        }];
                    }
                    (Some(Pick::Field), _) => {
                        self.stack.pop();
                        if let Some(Screen::Form(form)) = self.stack.last_mut()
                            && let Some(line) = form.fields[form.sel].line()
                        {
                            *line = Line::new(&frame);
                        }
                    }
                }
            }
            Screen::FrameApps { frame, sel } => {
                let (frame, at) = (*frame, *sel);
                let apps: Vec<u64> = self.apps_on(frame).iter().map(|a| a.id).collect();
                if let Some(Screen::FrameApps { sel, .. }) = self.stack.last_mut()
                    && step(sel, apps.len(), key, PAGE)
                {
                } else if back {
                    self.stack.pop();
                } else if matches!(key, "enter" | "right")
                    && let Some(id) = apps.get(at)
                {
                    self.stack.push(Screen::Actions { id: *id, sel: 0 });
                }
            }
            Screen::Log { lines, top, .. } => {
                // `top` is clamped when drawn: the height is not known here.
                let last = lines.len();
                match key {
                    "up" | "k" => *top = top.saturating_sub(1),
                    "down" | "j" => *top = (*top + 1).min(last),
                    "pageup" => *top = top.saturating_sub(PAGE),
                    "pagedown" | "space" => *top = (*top + PAGE).min(last),
                    "home" => *top = 0,
                    "end" => *top = last,
                    "esc" | "q" | "left" | "h" | "enter" => {
                        self.stack.pop();
                    }
                    _ => {}
                }
            }
        }
        Vec::new()
    }

    fn form_key(&mut self, key: &str) -> Vec<Action> {
        let Some(Screen::Form(form)) = self.stack.last_mut() else {
            return Vec::new();
        };
        let n = form.fields.len();
        let at = form.sel;
        if form.editing {
            let Some(line) = form.fields[at].line() else {
                form.editing = false;
                return Vec::new();
            };
            match key {
                "enter" | "esc" => form.editing = false,
                "up" | "down" | "tab" | "shift+tab" => {
                    form.editing = false;
                    step(&mut form.sel, n, key, PAGE);
                }
                _ => {
                    line.key(key);
                }
            }
            return Vec::new();
        }
        if matches!(key, "esc" | "q") {
            self.stack.pop();
            return Vec::new();
        }
        let field = &mut form.fields[at];
        // `j`, `k`, `h` move here too: nothing is typed unless editing.
        match (&mut field.kind, key) {
            (FieldKind::Submit, "enter" | "space") => {
                let args = form.args();
                let action = match &form.target {
                    Target::Start => Action::Start {
                        name: form.name.clone(),
                        args,
                    },
                    Target::Restart { id, head } => Action::Restart {
                        id: *id,
                        argv: Some(head.iter().cloned().chain(args).collect()),
                    },
                };
                self.stack.truncate(1);
                return vec![action];
            }
            (FieldKind::Flag { on }, "enter" | "space" | "left" | "right") => *on = !*on,
            (FieldKind::Count { n }, "enter" | "space" | "right") => *n = (*n + 1) % 10,
            (FieldKind::Count { n }, "left") => *n = n.saturating_sub(1),
            (FieldKind::Choice { choices, at }, "enter" | "space" | "right") => {
                *at = match *at {
                    None if !choices.is_empty() => Some(0),
                    Some(i) if i + 1 < choices.len() => Some(i + 1),
                    _ => None,
                }
            }
            (FieldKind::Choice { choices, at }, "left") => {
                *at = match *at {
                    None => choices.len().checked_sub(1),
                    Some(0) => None,
                    Some(i) => Some(i - 1),
                }
            }
            (FieldKind::Value { line }, "enter" | "space" | "right")
                if field.flags.iter().any(|f| f == "--frame") =>
            {
                // The frame is picked, not typed.
                let on = line.string();
                let rows = self.choices(true);
                let sel = rows.iter().position(|c| match c {
                    Choice::Frame(id) => id.to_string() == on,
                    Choice::Name(name) => *name == on,
                });
                let pick = Some(Pick::Field);
                self.stack.push(Screen::Frames {
                    sel: sel.unwrap_or(0),
                    pick,
                });
            }
            (
                FieldKind::Value { .. } | FieldKind::Words { .. } | FieldKind::More { .. },
                "enter" | "space" | "right",
            ) => form.editing = true,
            (
                FieldKind::Value { line } | FieldKind::Words { line } | FieldKind::More { line },
                "backspace" | "delete",
            ) => *line = Line::default(),
            (_, "left" | "h") if at == 0 => {
                self.stack.pop();
            }
            _ => {
                step(&mut form.sel, n, key, PAGE);
            }
        }
        Vec::new()
    }

    /// The menu in `w` by `h` cells: `h` rows of exactly `w` cells.
    pub fn render(&self, w: usize, h: usize) -> Vec<Row> {
        let (w, h) = (w.max(8), h.max(3));
        let inner = w - 2;
        let Some(screen) = self.stack.last() else {
            return vec![fit(Row::new(), w, PLAIN); h];
        };
        // What the screen is: title, rows, selection, keys, a hint.
        let mut hint: Option<String> = None;
        let (title, rows, sel, keys): (String, Vec<Row>, Option<usize>, Keys) = match screen {
            Screen::Sessions { names, sel } => (
                "which session?".into(),
                names.iter().map(|n| vec![run(n.clone(), PLAIN)]).collect(),
                Some(*sel),
                vec![("↑↓ move", 1), ("enter choose", 1), ("esc leave", 0)],
            ),
            Screen::Main { sel, .. } => {
                let rows = self.main_rows(inner);
                let title = match self.session.as_str() {
                    "" => "apps".to_string(),
                    name => format!("apps of {name}"),
                };
                let on_app = matches!(self.entries().get(*sel), Some(Entry::App(_)));
                let mut keys = vec![("↑↓ move", 1)];
                if on_app {
                    keys.extend([("enter open", 1), ("s start", 4), ("p pause", 2)]);
                    keys.extend([("r restart", 5), ("e options", 6), ("x stop", 3)]);
                    keys.extend([("m move", 6), ("l log", 7), ("f frames", 7)]);
                } else {
                    keys.extend([("enter open", 1), ("s start", 4), ("f frames", 5)]);
                }
                keys.push(("esc leave", 0));
                (title, rows, Some(*sel), keys)
            }
            Screen::Actions { id, sel } => {
                let app = self.info(*id);
                let exited = app.is_some_and(|a| a.state == State::Exited);
                let rows = (self.acts(*id).iter())
                    .map(|act| {
                        vec![
                            run(format!("{:<34}", act.label(exited)), PLAIN),
                            run(act.key(), DIM),
                        ]
                    })
                    .collect();
                let title = app.map_or_else(String::new, |a| {
                    format!("{} {}", a.label(), line_of(a.args()))
                });
                hint = app.map(|a| self.state_line(a));
                let keys = vec![("↑↓ move", 1), ("enter do", 1), ("esc back", 0)];
                (title.trim().to_string(), rows, Some(*sel), keys)
            }
            Screen::Start { sel } => {
                let apps = self.catalog.as_ref().map_or(&[][..], |c| &c.apps[..]);
                let width = apps.iter().map(|a| len(&a.name)).max().unwrap_or(0);
                let rows = (apps.iter())
                    .map(|a| {
                        vec![
                            run(format!("{:<width$}", a.name), BOLD),
                            run(format!("  {}", a.help), PLAIN),
                        ]
                    })
                    .collect();
                hint = apps.get(*sel).map(|a| a.help.clone());
                let keys = vec![("↑↓ move", 1), ("enter options", 1), ("esc back", 0)];
                ("start an app".into(), rows, Some(*sel), keys)
            }
            Screen::Form(form) => {
                let rows = form_rows(form, inner);
                let field = &form.fields[form.sel];
                hint = Some(field.help.clone()).filter(|h| !h.is_empty());
                let keys = if form.editing {
                    vec![("type the value", 1), ("enter done", 0)]
                } else {
                    let change = match field.kind {
                        FieldKind::Submit => "enter go",
                        FieldKind::Flag { .. } => "space toggle",
                        FieldKind::Count { .. } | FieldKind::Choice { .. } => "←→ change",
                        _ => "enter edit",
                    };
                    vec![("↑↓ move", 2), (change, 1), ("esc back", 0)]
                };
                let verb = match form.target {
                    Target::Start => "start",
                    Target::Restart { .. } => "restart",
                };
                let title = format!("{verb} {}: options", form.name);
                // An edited row is not a bar: its cursor is.
                let sel = (!form.editing).then_some(form.sel);
                let mut all = self.body(title, rows, sel, keys, hint, w, h, Some(form.sel));
                all.truncate(h);
                return all;
            }
            Screen::Frames { sel, pick } => {
                let choices = self.choices(pick.is_some());
                let rows = choices.iter().map(|c| self.choice_row(*c)).collect();
                let lit = match choices.get(*sel) {
                    Some(Choice::Frame(id)) => Some(*id),
                    _ => None,
                };
                let app = match pick {
                    Some(Pick::Move(id)) => self.info(*id),
                    _ => None,
                };
                let title = match (pick, app) {
                    (Some(Pick::Move(_)), Some(app)) => format!("move {} to", app.label()),
                    (Some(_), _) => "which frame?".to_string(),
                    (None, _) => "frames".to_string(),
                };
                // Said before enter: what the move costs.
                hint = match (pick, app, lit) {
                    (Some(Pick::Move(_)), Some(app), _) if app.moves_live() => {
                        Some("moves live: state kept".into())
                    }
                    (Some(Pick::Move(_)), _, _) => Some("restarts there: state lost".into()),
                    (Some(Pick::Field), _, _) => Some("the frame it starts on".into()),
                    (None, _, Some(id)) => {
                        let frame = self.frames.iter().find(|f| f.id == id);
                        frame.map(|f| f.what.clone()).filter(|w| !w.is_empty())
                    }
                    (None, _, None) => None,
                };
                let enter = match pick {
                    Some(Pick::Move(_)) => "enter move",
                    Some(Pick::Field) => "enter choose",
                    None => "enter apps",
                };
                let keys = vec![("↑↓ move", 1), (enter, 1), ("esc back", 0)];
                let inner = w - 2;
                let ruled = usize::from(h >= 12);
                let side = match inner {
                    74.. => screen_map(&self.frames, lit, 34, h.saturating_sub(3 + ruled)),
                    60.. => screen_map(&self.frames, lit, 26, h.saturating_sub(3 + ruled)),
                    44.. => screen_map(&self.frames, lit, 16, h.saturating_sub(3 + ruled)),
                    _ => Vec::new(),
                };
                return self.body_beside(
                    title,
                    rows,
                    Some(*sel),
                    keys,
                    hint,
                    w,
                    h,
                    Some(*sel),
                    side,
                );
            }
            Screen::FrameApps { frame, sel } => {
                let on = self.apps_on(*frame);
                let mut rows: Vec<Row> = on.iter().map(|a| self.app_row(a, inner)).collect();
                let sel = match rows.is_empty() {
                    true => {
                        rows.push(vec![run("no app draws on this frame", DIM)]);
                        None
                    }
                    false => Some((*sel).min(rows.len() - 1)),
                };
                let keys = vec![("↑↓ move", 1), ("enter actions", 1), ("esc back", 0)];
                (format!("apps on frame {frame}"), rows, sel, keys)
            }
            Screen::Log { title, lines, top } => {
                let rows: Vec<Row> = match lines.is_empty() {
                    true => vec![vec![run("nothing was written", DIM)]],
                    false => lines.iter().map(|l| vec![run(l.clone(), PLAIN)]).collect(),
                };
                let keys = vec![("↑↓ scroll", 1), ("esc back", 0)];
                return self.body(title.clone(), rows, None, keys, None, w, h, Some(*top));
            }
        };
        self.body(title, rows, sel, keys, hint, w, h, sel)
    }

    /// Title, a window on `rows` that shows row `at`, a line of note or
    /// hint, the keys.
    #[allow(clippy::too_many_arguments)]
    fn body(
        &self,
        title: String,
        rows: Vec<Row>,
        sel: Option<usize>,
        keys: Vec<(&str, u8)>,
        hint: Option<String>,
        w: usize,
        h: usize,
        at: Option<usize>,
    ) -> Vec<Row> {
        self.body_beside(title, rows, sel, keys, hint, w, h, at, Vec::new())
    }

    /// `body`, the rows of `side` at the right of the list, from its
    /// first row down: they do not scroll.
    #[allow(clippy::too_many_arguments)]
    fn body_beside(
        &self,
        title: String,
        rows: Vec<Row>,
        sel: Option<usize>,
        keys: Vec<(&str, u8)>,
        hint: Option<String>,
        w: usize,
        h: usize,
        at: Option<usize>,
        side: Vec<Row>,
    ) -> Vec<Row> {
        let inner = w - 2;
        let side_w = side.iter().map(|r| row_len(r)).max().unwrap_or(0);
        // The list, less the side and a cell between them.
        let list = match side_w {
            0 => inner,
            side_w => inner.saturating_sub(side_w + 1),
        };
        let margin = |row: Row, pad: Style| {
            let mut out = vec![run(" ", PLAIN)];
            out.extend(fit(row, inner, pad));
            out.push(run(" ", PLAIN));
            out
        };
        // A rule under the title when there is height for it.
        let ruled = h >= 12;
        let message = match (&self.note, hint) {
            (Some((text, true)), _) => Some(vec![run(
                text.clone(),
                Style {
                    bold: true,
                    ..fg("red")
                },
            )]),
            (Some((text, false)), _) => Some(vec![run(text.clone(), BOLD)]),
            (None, Some(hint)) => Some(vec![run(hint, DIM)]),
            (None, None) => None,
        };
        // The line of note is kept when there is height: nothing jumps.
        let noted = message.is_some() || h >= 12;
        let room = h
            .saturating_sub(2 + usize::from(ruled) + usize::from(noted))
            .max(1);
        let n = rows.len();
        let top = match at {
            Some(at) if n > room => at.saturating_sub(room / 2).min(n - room),
            _ => 0,
        };
        let mut out = Vec::with_capacity(h);
        let place = match (sel, n > room) {
            (Some(sel), true) => format!("{}/{n}", sel + 1),
            (None, true) => format!("{}-{}/{n}", top + 1, top + room),
            _ => String::new(),
        };
        let name = cut(&title, inner.saturating_sub(len(&place) + 1));
        let gap = inner - len(&name) - len(&place);
        out.push(margin(
            vec![
                run(name, BOLD),
                run(" ".repeat(gap), PLAIN),
                run(place, DIM),
            ],
            PLAIN,
        ));
        if ruled {
            out.push(margin(vec![run("─".repeat(inner), DIM)], PLAIN));
        }
        let mut listed: Vec<Row> = Vec::new();
        for (i, row) in rows.into_iter().enumerate().skip(top).take(room) {
            // More above, more below: told at the right end.
            let more = match i {
                i if i == top && top > 0 => Some("↑"),
                i if i == top + room - 1 && top + room < n => Some("↓"),
                _ => None,
            };
            let mut row = match sel == Some(i) {
                true => selected(fit(row, list, PLAIN)),
                false => fit(row, list, PLAIN),
            };
            if let (Some(more), Some(last)) = (more, row.last_mut())
                && last.text.ends_with(' ')
            {
                last.text.pop();
                row.push(run(more, DIM));
            }
            listed.push(row);
        }
        let first = out.len();
        let rows_out = h - 1 - usize::from(noted) - first;
        let mut side = side.into_iter();
        for k in 0..rows_out {
            let mut row = match listed.get(k) {
                Some(row) => row.clone(),
                None => fit(Row::new(), list, PLAIN),
            };
            if side_w > 0 {
                row.push(run(" ", PLAIN));
                row.extend(side.next().unwrap_or_default());
            }
            out.push(margin(row, PLAIN));
        }
        if noted {
            out.push(margin(message.unwrap_or_default(), PLAIN));
        }
        out.push(margin(vec![run(foot(&keys, inner), DIM)], PLAIN));
        out.truncate(h);
        out
    }

    fn state_line(&self, app: &AppInfo) -> String {
        let state = match (app.state, app.status) {
            (State::Running, _) => "running".to_string(),
            (State::Paused, _) => "paused".to_string(),
            (_, Some(0) | None) => "ended".to_string(),
            (_, Some(status)) => format!("ended with status {status}"),
        };
        let kind = if app.child { "" } else { ", started elsewhere" };
        format!("{state} for {}, pid {}{kind}", since(app.since), app.pid)
    }

    /// An app as a row: mark, name and arguments, state, since.
    fn app_row(&self, app: &AppInfo, inner: usize) -> Row {
        let (mark, state) = match (app.state, app.status) {
            (State::Running, _) => (run("●", fg("green")), "running".to_string()),
            (State::Paused, _) => (run("‖", fg("yellow")), "paused".to_string()),
            (_, Some(0) | None) => (run("○", DIM), "ended".to_string()),
            (_, Some(status)) => (run("○", fg("red")), format!("exit {status}")),
        };
        let state_style = mark.style;
        let tail = format!(" {state:<8} {:>4}", since(app.since));
        let room = inner.saturating_sub(2 + len(&tail));
        let mut text = vec![run(app.label(), BOLD)];
        let args = line_of(app.args());
        if !args.is_empty() {
            text.push(run(format!(" {args}"), PLAIN));
        }
        if !app.child {
            text.push(run(format!(" (pid {})", app.pid), DIM));
        }
        let mut row = vec![mark, run(" ", PLAIN)];
        row.extend(fit(text, room, PLAIN));
        row.push(run(format!(" {state:<8}"), state_style));
        row.push(run(format!(" {:>4}", since(app.since)), DIM));
        row
    }

    /// A row of the frames screen.
    fn choice_row(&self, choice: Choice) -> Row {
        let id = match choice {
            Choice::Frame(id) => id,
            Choice::Name(name) => {
                let what = match name {
                    "raw" => "the whole screen",
                    "active" => "the pane of the cursor",
                    "largest" => "the largest pane",
                    "quietest" => "the pane that changes least",
                    _ => "a pane that is idle",
                };
                return vec![run(format!("{name:<9}"), BOLD), run(what, DIM)];
            }
        };
        let Some(frame) = self.frames.iter().find(|f| f.id == id) else {
            return vec![run(format!("{id:>2}"), BOLD)];
        };
        let size = format!("{}x{}", frame.rect[2], frame.rect[3]);
        let busy = match frame.idle {
            true => run("idle ", fg("green")),
            false => run("busy ", fg("yellow")),
        };
        let names: Vec<String> = self.apps_on(id).iter().map(|a| a.label()).collect();
        let apps = match names.is_empty() {
            true => run("·", DIM),
            false => run(names.join(", "), BOLD),
        };
        vec![
            run(format!("{id:>2} "), BOLD),
            run(format!("{:<5}{size:<8}", frame.kind), PLAIN),
            busy,
            apps,
        ]
    }

    fn main_rows(&self, inner: usize) -> Vec<Row> {
        let mut rows: Vec<Row> = (self.shown().iter())
            .map(|app| self.app_row(app, inner))
            .collect();
        if rows.is_empty() {
            rows.push(vec![run("no app on this session", DIM)]);
            // Not a row to select: the entries start below it.
            rows.clear();
        }
        if let Some(catalog) = &self.catalog {
            rows.push(vec![run("+ start an app…", PLAIN)]);
            if catalog.apps.iter().any(|a| !a.random.is_empty()) {
                rows.push(vec![run("? random app", PLAIN)]);
            }
        }
        rows.push(vec![run("# frames…", PLAIN)]);
        rows
    }
}

/// The screen to scale, in `width` cells and at most `height` rows,
/// border included: a tile per pane with its id, the frame `lit` filled.
fn screen_map(frames: &[FrameInfo], lit: Option<u64>, width: usize, height: usize) -> Vec<Row> {
    let raw = frames.iter().find(|f| f.kind == "raw");
    let cols = raw.map_or(0, |f| f.rect[2]).max(1);
    let lines = raw.map_or(0, |f| f.rect[3]).max(1);
    if width < 6 || height < 3 {
        return Vec::new();
    }
    // Cells of the map are cells of the screen: one scale for both axes.
    let mut iw = width - 2;
    let mut ih = ((iw * lines + cols / 2) / cols).max(1);
    if ih > height - 2 {
        ih = height - 2;
        iw = (ih * cols / lines).clamp(1, width - 2);
    }
    let yellow = Style {
        bold: true,
        ..fg("yellow")
    };
    let raw_lit = raw.is_some_and(|f| Some(f.id) == lit);
    let edge = if raw_lit { yellow } else { DIM };
    let mut cells = vec![vec![(' ', PLAIN); iw]; ih];
    if raw_lit {
        cells = vec![vec![('█', fg("yellow")); iw]; ih];
    }
    for frame in frames.iter().filter(|f| f.kind != "raw") {
        let [x, y, w, h] = frame.rect;
        let scale = |v: usize, to: usize, of: usize| ((v * to + of / 2) / of).min(to);
        let (x0, y0) = (
            scale(x, iw, cols).min(iw - 1),
            scale(y, ih, lines).min(ih - 1),
        );
        let (mut x1, mut y1) = (scale(x + w, iw, cols), scale(y + h, ih, lines));
        // A tile is a cell at least, and a cell apart from the next.
        if x1 < iw && x1 > x0 + 1 {
            x1 -= 1;
        }
        if y1 < ih && y1 > y0 + 1 && ih > 4 {
            y1 -= 1;
        }
        let (x1, y1) = (x1.max(x0 + 1), y1.max(y0 + 1));
        let on = Some(frame.id) == lit;
        let fill = if on {
            ('█', fg("yellow"))
        } else {
            ('░', DIM)
        };
        for row in &mut cells[y0..y1] {
            row[x0..x1].fill(fill);
        }
        // Its id, in the middle, when it fits.
        let id = frame.id.to_string();
        if id.len() <= x1 - x0 {
            let style = Style {
                inverse: on,
                ..if on { yellow } else { BOLD }
            };
            let at = x0 + (x1 - x0 - id.len()) / 2;
            for (i, c) in id.chars().enumerate() {
                cells[(y0 + y1 - 1) / 2][at + i] = (c, style);
            }
        }
    }
    let mut rows = vec![vec![run(format!("┌{}┐", "─".repeat(iw)), edge)]];
    for line in cells {
        let mut row = vec![run("│", edge)];
        for (c, style) in line {
            match row.last_mut() {
                Some(last) if last.style == style && row_len(std::slice::from_ref(last)) > 0 => {
                    last.text.push(c)
                }
                _ => row.push(run(c.to_string(), style)),
            }
        }
        // The border is its own run: never joined to a tile.
        row.push(run("│", edge));
        rows.push(row);
    }
    rows.push(vec![run(format!("└{}┘", "─".repeat(iw)), edge)]);
    rows
}

/// The keys that fit `width`, the least important dropped first; the
/// way back (priority 0) always.
fn foot(keys: &[(&str, u8)], width: usize) -> String {
    let mut kept: Vec<(&str, u8)> = keys.to_vec();
    loop {
        let line = kept.iter().map(|k| k.0).collect::<Vec<_>>().join("  ");
        let worst = kept.iter().map(|k| k.1).max().unwrap_or(0);
        if len(&line) <= width || worst == 0 {
            return cut(&line, width);
        }
        let at = kept.iter().rposition(|k| k.1 == worst).unwrap_or(0);
        kept.remove(at);
    }
}

/// Cells of the value column of a form wide enough to show the help
/// of each row beside it.
const VALUE: usize = 12;

fn form_rows(form: &Form, inner: usize) -> Vec<Row> {
    let labels = form.fields.iter().skip(1).map(|f| len(&f.label));
    let label = labels.max().unwrap_or(0).min(inner / 2);
    let room = inner.saturating_sub(4 + label + 1);
    // Wide: the help of every row, in a column of its own.
    let helped = inner >= 60 && room > VALUE + 12;
    let mut rows = Vec::new();
    for (i, field) in form.fields.iter().enumerate() {
        let editing = form.editing && form.sel == i;
        // A typed line of words has the width: its help is the hint.
        let wordy = matches!(field.kind, FieldKind::Words { .. } | FieldKind::More { .. });
        let helped = helped && !wordy;
        let name = run(format!("{:<label$} ", cut(&field.label, label)), PLAIN);
        let room = if helped && !editing { VALUE } else { room };
        let unset = |default: &Option<String>| match default {
            Some(default) => run(cut(&format!("({default})"), room), DIM),
            None => run("·", DIM),
        };
        let mut row = match &field.kind {
            FieldKind::Submit => {
                rows.push(vec![run(format!("▶ {}", field.label), BOLD)]);
                continue;
            }
            FieldKind::Flag { on } => {
                vec![run(if *on { "[x] " } else { "[ ] " }, PLAIN), name]
            }
            FieldKind::Count { n } => vec![run(format!("[{n}] "), PLAIN), name],
            FieldKind::Choice { choices, at } => {
                let value = match at.and_then(|at| choices.get(at)) {
                    Some(choice) => run(cut(&format!("‹ {choice} ›"), room), PLAIN),
                    None => unset(&field.default),
                };
                vec![run("    ", PLAIN), name, value]
            }
            FieldKind::Value { line } | FieldKind::Words { line } | FieldKind::More { line } => {
                let mut row = vec![run("    ", PLAIN), name];
                if line.text.is_empty() && !editing {
                    row.push(unset(&field.default));
                } else {
                    row.extend(line.runs(room, editing, PLAIN));
                }
                row
            }
        };
        if helped && !editing {
            let mut row_ = fit(row, 4 + label + 1 + VALUE + 1, PLAIN);
            row_.push(run(field.help.clone(), DIM));
            row = row_;
        }
        rows.push(row);
    }
    rows
}

#[cfg(test)]
pub(crate) mod tests;
