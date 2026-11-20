// SPDX-License-Identifier: GPL-3.0-or-later
//! The pop-up: the driver's own screen that asks for the permissions an
//! app declares and has no answer for. See `plan/design.md`,
//! "Permissions". Pure: what it shows, the keys, the answers.

use crate::grants::Permission;
use crate::permissions::{Declared, Entry};
use ptyrc_store::manifest::{self, Colour};
use ptyrc_vt::{Attrs, Cell, Color, Grid, Style, char_width, str_width};

/// The least screen the box is drawn on; a smaller one is refused at
/// the request.
pub(crate) const MIN_SIZE: (u16, u16) = (24, 8);

/// How a permission is shown (`plan/store-model.md`, Colours): plain,
/// ticked when shown; amber and red, ticked by hand.
fn ticked(colour: Colour) -> bool {
    colour == Colour::Plain
}

fn fg(colour: Colour) -> Color {
    match colour {
        Colour::Plain => Color::Default,
        Colour::Amber => Color::Indexed(11),
        Colour::Red => Color::Indexed(9),
    }
}

/// `limit-*` in plain words: the figures of `[limits]` it grants.
fn limits(permission: Permission, declared: &Declared) -> String {
    let figures: Vec<String> = (declared.limits.iter())
        .filter(|(k, _)| manifest::limit_permission(k) == permission.name())
        .map(|(key, n)| match key.as_str() {
            "memory-mib" => format!("up to {n} MiB of memory"),
            "canvas-screens" => format!("canvases of up to {n} screens"),
            "log-mib" => format!("a log of up to {n} MiB"),
            "tick-ms" => format!("up to {n} ms a tick"),
            "init-ms" => format!("up to {n} ms to start"),
            "calls-per-second" => format!("up to {n} requests a second"),
            _ => format!("{key} = {n}"),
        })
        .collect();
    match (figures.is_empty(), permission) {
        (false, _) => format!("use {}", figures.join(", ")),
        (true, Permission::LimitMemory) => "use more memory than the default".into(),
        (true, _) => "use more time than the default".into(),
    }
}

/// A permission in plain words, and its colour: the table of
/// `ptyrc_store::manifest::COLOURS`, held to it by
/// `ptyrc-cli/tests/store.rs`. The declared words of `words`, the
/// figures of `limit-*`.
pub(crate) fn told(permission: Permission, declared: &Declared) -> (String, Colour) {
    use Colour::*;
    use Permission::*;
    let words = &declared.words;
    let (text, colour) = match permission {
        Layout => ("see how the screen is split, not what it shows", Plain),
        Shape => ("see where the text is, not what it says", Plain),
        Words if words.is_empty() => ("see the words it names", Plain),
        Words => return (format!("see these words: {}", words.join(", ")), Plain),
        Text => ("read all the text on the screen", Red),
        FrontWidget => ("show a box in front of the text, bearing its name", Plain),
        Front => ("draw over the text, without a border", Amber),
        ScreenWide => ("draw anywhere on the screen, not on its frame alone", Amber),
        KeysArrows => ("take the arrow and function keys", Plain),
        KeysGrab => ("take other keys, or all of them while it shows a box", Red),
        KeysRead => ("read every key you type", Red),
        Pointer => ("follow the mouse pointer anywhere", Plain),
        Clicks => ("see your clicks anywhere", Red),
        ClicksGrab => ("take clicks from the program", Red),
        Clock => ("read the wall clock and the time zone", Plain),
        Storage => ("keep files in a directory of its own", Plain),
        Files => ("read a file you name", Plain),
        FilesWrite => ("write a file you name", Red),
        ProbeGit => ("run git in the directory of your program", Plain),
        ProbeSystem => ("see the processes, the logins, the listening ports", Plain),
        LimitMemory | LimitTime => return (limits(permission, declared), Plain),
    };
    (text.to_string(), colour)
}

/// What an answered screen leaves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// `enter`: the answers, merged into those of the file before.
    Answered(Entry),
    /// `esc`: nothing written, nothing started.
    Cancelled,
}

/// One screen: an app, the permissions asked, a box ticked or not each.
#[derive(Clone, Debug)]
pub(crate) struct Ask {
    /// The app's name on the table.
    pub name: String,
    /// The key of the file: `native/NAME`, or an origin.
    pub key: String,
    /// Tier and commit; `native` for a native app.
    pub tier: String,
    pub declared: Declared,
    /// The answers of the file before.
    pub entry: Entry,
    /// From `ptyrc manage`: every declared permission, to change.
    pub review: bool,
    rows: Vec<(Permission, bool)>,
    sel: usize,
    /// The first line of the list shown.
    top: usize,
}

impl Ask {
    /// Ask for `shown`, in the order of the list. A start: plain ones
    /// ticked, the others not. A review: the answers of `entry`; one
    /// without an answer as a start.
    pub fn new(
        name: &str,
        key: &str,
        tier: &str,
        declared: &Declared,
        entry: Entry,
        shown: &[Permission],
        review: bool,
    ) -> Ask {
        let mut shown = shown.to_vec();
        shown.sort();
        shown.dedup();
        let rows = shown
            .iter()
            .map(|&p| {
                let plain = ticked(told(p, declared).1);
                let ticked = match (entry.allow.contains(&p), entry.deny.contains(&p)) {
                    (true, _) if review => true,
                    (_, true) if review => false,
                    _ => plain,
                };
                (p, ticked)
            })
            .collect();
        let mut ask = Ask {
            name: name.to_string(),
            key: key.to_string(),
            tier: tier.to_string(),
            declared: declared.clone(),
            entry,
            review,
            rows,
            sel: 0,
            top: 0,
        };
        // A degree comes unticked when one below it is.
        if let Some(low) = (ask.rows.iter())
            .filter(|(p, t)| !t && p.degree().is_some())
            .filter_map(|(p, _)| p.degree())
            .min()
        {
            for (p, t) in &mut ask.rows {
                *t &= p.degree().is_none_or(|d| d < low);
            }
        }
        ask
    }

    /// The permissions shown and whether each is ticked.
    #[cfg(test)]
    pub fn rows(&self) -> &[(Permission, bool)] {
        &self.rows
    }

    /// A key of the host keyboard, by its name (`grab::name`). `None`:
    /// the screen stays.
    pub fn key(&mut self, name: &str) -> Option<Outcome> {
        let last = self.rows.len().saturating_sub(1);
        match name {
            "up" | "k" => self.sel = self.sel.saturating_sub(1),
            "down" | "j" => self.sel = (self.sel + 1).min(last),
            "home" => self.sel = 0,
            "end" => self.sel = last,
            " " | "space" => self.toggle(self.sel),
            "enter" => return Some(Outcome::Answered(self.answers())),
            "esc" => return Some(Outcome::Cancelled),
            _ => {}
        }
        None
    }

    /// Tick or untick a row. Unticking a degree unticks those above it;
    /// ticking one ticks those below.
    fn toggle(&mut self, at: usize) {
        let Some(&(permission, ticked)) = self.rows.get(at) else {
            return;
        };
        self.rows[at].1 = !ticked;
        let Some(degree) = permission.degree() else {
            return;
        };
        for (p, t) in &mut self.rows {
            match p.degree() {
                Some(d) if ticked && d > degree => *t = false,
                Some(d) if !ticked && d < degree => *t = true,
                _ => {}
            }
        }
    }

    /// The entry of the file after: each row shown a yes or a no, the
    /// rest as before.
    pub fn answers(&self) -> Entry {
        let mut entry = self.entry.clone();
        for &(p, ticked) in &self.rows {
            let (into, from) = match ticked {
                true => (&mut entry.allow, &mut entry.deny),
                false => (&mut entry.deny, &mut entry.allow),
            };
            into.insert(p);
            from.remove(&p);
        }
        entry
    }

    /// The lines of the list: per permission, its line and its reason;
    /// the row each line is of.
    fn list(&self, inner: usize) -> Vec<(usize, Vec<(String, Style)>)> {
        let mut lines = Vec::new();
        for (i, &(p, ticked)) in self.rows.iter().enumerate() {
            let (text, colour) = told(p, &self.declared);
            let mut style = Style {
                fg: fg(colour),
                ..Style::default()
            };
            if i == self.sel {
                style.attrs.insert(Attrs::INVERSE);
            }
            let check = if ticked { "[x] " } else { "[ ] " };
            lines.push((i, vec![(format!("{check}{text}"), style)]));
            let why = self.declared.permissions.get(&p).map_or("", |w| w.trim());
            if !why.is_empty() {
                let dim = Style {
                    attrs: Attrs(Attrs::DIM),
                    ..Style::default()
                };
                lines.push((i, vec![(format!("    {}", one_line(why, inner - 4)), dim)]));
            }
        }
        lines
    }

    /// The box for a screen of `cols` x `rows`, and where it goes; it is
    /// cut to the screen when that is smaller than `MIN_SIZE`. The list
    /// scrolls to keep the selection in view.
    pub fn render(&mut self, cols: u16, rows: u16) -> (Grid, (u16, u16)) {
        let border = Style {
            fg: Color::Indexed(15),
            bg: Color::Indexed(4),
            attrs: Attrs(Attrs::BOLD),
            ..Style::default()
        };
        let plain = Style::default();
        let dim = Style {
            attrs: Attrs(Attrs::DIM),
            ..Style::default()
        };
        let head = match self.review {
            true => "This app may:",
            false => "This app asks to:",
        };
        let keys = match self.review {
            true => "↑↓ move  space tick  enter save  esc cancel",
            false => "↑↓ move  space tick  enter start  esc cancel",
        };
        let title = format!(" {} ─ {} ", self.name, self.tier);
        let mark = " ptyrc ";
        let fixed = [head, keys, &self.key];
        let list = self.list(usize::from(cols.saturating_sub(4)).max(8));
        let widest = (fixed.iter().map(|l| str_width(l)))
            .chain(list.iter().map(|(_, l)| str_width(&l[0].0)))
            .chain([str_width(&title) + str_width(mark) + 3])
            .max()
            .unwrap_or(0);
        let inner = widest.min(usize::from(cols.max(MIN_SIZE.0)) - 4);
        let w = inner + 4;
        // Top, key, head, blank, list, blank, keys, bottom.
        let room = usize::from(rows.max(MIN_SIZE.1)).saturating_sub(7).max(1);
        let shown = list.len().min(room);
        let h = shown + 7;
        // The selection in view.
        let first = list.iter().position(|(r, _)| *r == self.sel).unwrap_or(0);
        let last = list.iter().rposition(|(r, _)| *r == self.sel).unwrap_or(0);
        if first < self.top {
            self.top = first;
        }
        if last >= self.top + shown {
            self.top = last + 1 - shown;
        }
        self.top = self.top.min(list.len() - shown);

        let mut grid = Grid::new(w as u16, h as u16);
        let put = |grid: &mut Grid, y: usize, x: usize, text: &str, style: Style, end: usize| {
            let mut x = x;
            for c in text.chars() {
                let cw = char_width(c);
                if cw == 0 {
                    continue;
                }
                if x + cw > end {
                    break;
                }
                grid.put(x as u16, y as u16, Cell::glyph(c, cw as u8, style));
                x += cw;
            }
        };
        for y in 0..h {
            for x in 0..w {
                let edge = x == 0 || x == w - 1 || y == 0 || y == h - 1;
                let c = match (x, y) {
                    (0, 0) => '╔',
                    (x, 0) if x == w - 1 => '╗',
                    (0, y) if y == h - 1 => '╚',
                    (x, y) if x == w - 1 && y == h - 1 => '╝',
                    (_, 0) => '═',
                    (_, y) if y == h - 1 => '═',
                    _ if edge => '║',
                    _ => ' ',
                };
                let style = if edge { border } else { plain };
                grid.put(x as u16, y as u16, Cell::glyph(c, 1, style));
            }
        }
        put(&mut grid, 0, 2, &title, border, w - 2);
        let at = w.saturating_sub(str_width(mark) + 2);
        put(&mut grid, h - 1, at, mark, border, w - 1);
        put(&mut grid, 1, 2, &self.key, dim, w - 2);
        put(&mut grid, 2, 2, head, plain, w - 2);
        for (i, (_, runs)) in list.iter().skip(self.top).take(shown).enumerate() {
            let (text, style) = &runs[0];
            let text = one_line(text, inner);
            put(&mut grid, 4 + i, 2, &text, *style, w - 2);
        }
        if self.top > 0 {
            put(&mut grid, 3, w - 8, "↑ more", dim, w - 2);
        }
        if self.top + shown < list.len() {
            put(&mut grid, h - 3, w - 8, "↓ more", dim, w - 2);
        }
        put(&mut grid, h - 2, 2, keys, dim, w - 2);
        let x = cols.saturating_sub(w as u16) / 2;
        let y = rows.saturating_sub(h as u16) / 2;
        (grid, (x, y))
    }
}

/// `text` on one line of at most `width` cells; cut with `…`.
fn one_line(text: &str, width: usize) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if str_width(&flat) <= width {
        return flat;
    }
    let mut out = String::new();
    let mut used = 0;
    for c in flat.chars() {
        let cw = char_width(c);
        if used + cw + 1 > width {
            break;
        }
        out.push(c);
        used += cw;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use Permission::*;

    fn declared(permissions: &[Permission]) -> Declared {
        Declared {
            permissions: permissions
                .iter()
                .map(|p| (*p, format!("why {}", p.name())))
                .collect(),
            words: vec!["food".into(), "fish".into(), "toy".into()],
            limits: Vec::new(),
            hand: Default::default(),
        }
    }

    fn entry(allow: &[Permission], deny: &[Permission]) -> Entry {
        Entry {
            allow: allow.iter().copied().collect(),
            deny: deny.iter().copied().collect(),
        }
    }

    fn ask(shown: &[Permission], before: Entry, review: bool) -> Ask {
        let all = declared(&[Shape, Words, Text, KeysArrows, Front]);
        Ask::new("cat", "native/cat", "native", &all, before, shown, review)
    }

    fn text(grid: &Grid) -> Vec<String> {
        grid.text()
    }

    #[test]
    fn plain_ticked_red_and_amber_not() {
        let a = ask(&[Text, Shape, Front, KeysArrows], Entry::default(), false);
        assert_eq!(
            a.rows(),
            [
                (Shape, true),
                (Text, false),
                (Front, false),
                (KeysArrows, true)
            ]
        );
        let none = Declared::default();
        assert_eq!(told(Text, &none).1, Colour::Red);
        assert_eq!(told(Front, &none).1, Colour::Amber);
        assert_eq!(told(ScreenWide, &none).1, Colour::Amber);
        // Decision 17: asked, plain.
        assert_eq!(told(Clock, &none).1, Colour::Plain);
        assert_eq!(told(Storage, &none).1, Colour::Plain);
    }

    #[test]
    fn the_permissions_and_colours_are_those_of_the_manifest() {
        let mut ours: Vec<&str> = Permission::ALL.iter().map(|p| p.name()).collect();
        ours.sort();
        assert_eq!(ours, manifest::PERMISSIONS);
        for &p in Permission::ALL {
            let told = told(p, &Declared::default()).1;
            assert_eq!(told, manifest::colour(p.name()), "{}", p.name());
        }
    }

    #[test]
    fn a_limit_with_its_figure() {
        let mut d = declared(&[Shape, LimitMemory, LimitTime]);
        d.limits = vec![
            ("memory-mib".into(), 128),
            ("log-mib".into(), 2),
            ("tick-ms".into(), 100),
        ];
        let memory = told(LimitMemory, &d);
        assert_eq!(
            memory.0,
            "use up to 128 MiB of memory, a log of up to 2 MiB"
        );
        assert_eq!(memory.1, Colour::Plain);
        assert_eq!(told(LimitTime, &d).0, "use up to 100 ms a tick");
        let a = Ask::new(
            "cat",
            "x/y/z",
            "untested",
            &d,
            Entry::default(),
            &[LimitMemory],
            false,
        );
        assert_eq!(a.rows(), [(LimitMemory, true)]);
    }

    #[test]
    fn keys_tick_and_answer() {
        let mut a = ask(&[Shape, Text], entry(&[KeysArrows], &[]), false);
        assert_eq!(a.key("down"), None);
        assert_eq!(a.key(" "), None);
        assert_eq!(a.rows(), [(Shape, true), (Text, true)]);
        assert_eq!(a.key("k"), None);
        assert_eq!(a.key("space"), None);
        // Shape unticked: text above it too.
        assert_eq!(a.rows(), [(Shape, false), (Text, false)]);
        let Some(Outcome::Answered(e)) = a.key("enter") else {
            panic!("answered");
        };
        // A yes and a no both kept; the answers before stay.
        assert_eq!(e, entry(&[KeysArrows], &[Shape, Text]));
        assert_eq!(a.key("esc"), Some(Outcome::Cancelled));
    }

    #[test]
    fn the_ladder() {
        let mut a = ask(&[Shape, Words, Text], Entry::default(), false);
        assert_eq!(a.rows(), [(Shape, true), (Words, true), (Text, false)]);
        // Text ticked: those below stay ticked.
        a.key("down");
        a.key("down");
        a.key(" ");
        assert_eq!(a.rows(), [(Shape, true), (Words, true), (Text, true)]);
        // Words unticked: text too, shape stays.
        a.key("up");
        a.key(" ");
        assert_eq!(a.rows(), [(Shape, true), (Words, false), (Text, false)]);
        // Text ticked again: words below it too.
        a.key("j");
        a.key(" ");
        assert_eq!(a.rows(), [(Shape, true), (Words, true), (Text, true)]);
        // Shape unticked: every degree above it.
        a.key("home");
        a.key(" ");
        assert_eq!(a.rows(), [(Shape, false), (Words, false), (Text, false)]);
    }

    #[test]
    fn a_review_is_filled_from_the_file() {
        let a = ask(
            &[Shape, Words, Text, KeysArrows, Front],
            entry(&[Text, Front], &[Words]),
            true,
        );
        // Words denied: text above it comes unticked though allowed.
        assert_eq!(
            a.rows(),
            [
                (Shape, true),
                (Words, false),
                (Text, false),
                (Front, true),
                (KeysArrows, true)
            ]
        );
    }

    #[test]
    fn the_box() {
        let mut a = ask(&[Shape, Words], Entry::default(), false);
        let (grid, (x, y)) = a.render(80, 24);
        let lines = text(&grid);
        assert!(lines[0].starts_with("╔═ cat ─ native ═"), "{lines:?}");
        assert!(lines[0].ends_with("╗"));
        assert!(lines.last().unwrap().contains(" ptyrc ═╝"));
        assert!(lines[1].starts_with("║ native/cat  "));
        assert!(lines[2].starts_with("║ This app asks to:  "));
        assert!(lines[4].contains("[x] see where the text is, not what it says"));
        assert!(lines[5].contains("    why shape"));
        assert!(lines[6].contains("[x] see these words: food, fish, toy"));
        assert!(lines[grid.rows() as usize - 2].contains("enter start  esc cancel"));
        assert_eq!((x, y), ((80 - grid.cols()) / 2, (24 - grid.rows()) / 2));
        // The selection is shown inverse.
        assert!(grid.cell(2, 4).style.attrs.contains(Attrs::INVERSE));
        assert!(!grid.cell(2, 6).style.attrs.contains(Attrs::INVERSE));
    }

    #[test]
    fn a_short_screen_scrolls_a_narrow_one_cuts() {
        let all = [Shape, Words, Text, KeysArrows, Front];
        let mut a = ask(&all, Entry::default(), false);
        let (grid, _) = a.render(40, 9);
        let lines = text(&grid);
        assert_eq!(grid.rows(), 9);
        assert!(grid.cols() <= 40);
        assert!(lines[4].contains("[x] see where the text"), "{lines:?}");
        assert!(lines[4].contains('…'), "{lines:?}");
        assert!(lines[6].contains("↓ more"));
        for _ in 0..3 {
            a.key("down");
        }
        let (grid, _) = a.render(40, 9);
        let lines = text(&grid);
        assert!(lines[3].contains("↑ more"), "{lines:?}");
        assert!(
            lines[4..6]
                .iter()
                .any(|l| l.contains("[ ] draw over the text"))
        );
        // Smaller than the least: the box of the least, cut by the
        // screen where it is laid.
        let (grid, (x, y)) = a.render(10, 3);
        assert_eq!((grid.cols(), grid.rows()), (MIN_SIZE.0, MIN_SIZE.1));
        assert_eq!((x, y), (0, 0));
    }
}
