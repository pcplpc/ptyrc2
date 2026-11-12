// SPDX-License-Identifier: GPL-3.0-or-later
//! The emulator: a byte stream to a `Grid`, a cursor, a pen and modes.

use crate::event::{Cursor, VtEvent};
use crate::grid::{Cell, Grid, Rect, Style};
use crate::sgr::{self, Params};
use smol_str::SmolStr;
use unicode_width::UnicodeWidthChar;

/// Character set of G0 or G1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Charset {
    #[default]
    Ascii,
    /// DEC special graphics: line drawing.
    DecSpecial,
}

/// Terminal modes. Input modes are tracked, not acted on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Modes {
    pub alt_screen: bool,
    /// DECOM: cursor addressing relative to the scroll margins.
    pub origin: bool,
    /// DECAWM.
    pub autowrap: bool,
    /// IRM.
    pub insert: bool,
    /// LNM: line feed also returns the carriage.
    pub linefeed: bool,
    /// Mode 2026 is open.
    pub sync_update: bool,
    pub bracketed_paste: bool,
    /// DECCKM.
    pub app_cursor: bool,
    pub app_keypad: bool,
    /// Any mouse reporting mode is on.
    pub mouse: bool,
    pub focus_events: bool,
    /// DECSCUSR parameter, 0 for the default.
    pub cursor_shape: u8,
    pub g0: Charset,
    pub g1: Charset,
    /// G1 is invoked (SO).
    pub shift_out: bool,
}

impl Default for Modes {
    fn default() -> Modes {
        Modes {
            alt_screen: false,
            origin: false,
            autowrap: true,
            insert: false,
            linefeed: false,
            sync_update: false,
            bracketed_paste: false,
            app_cursor: false,
            app_keypad: false,
            mouse: false,
            focus_events: false,
            cursor_shape: 0,
            g0: Charset::Ascii,
            g1: Charset::Ascii,
            shift_out: false,
        }
    }
}

impl Modes {
    /// The charset glyphs are printed through.
    pub fn charset(&self) -> Charset {
        if self.shift_out { self.g1 } else { self.g0 }
    }
}

/// OSC 8 hyperlink.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub params: String,
    pub uri: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Saved {
    col: u16,
    row: u16,
    pending_wrap: bool,
    pen: Style,
    origin: bool,
    g0: Charset,
    g1: Charset,
    shift_out: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Ground,
    Utf8,
    Esc,
    EscInter,
    Csi,
    Osc,
    OscEsc,
    /// DCS, SOS, PM, APC: skipped up to ST.
    Str,
    StrEsc,
}

const OSC_MAX: usize = 4096;
const LINKS_MAX: usize = 1 << 16;
const EVENTS_MAX: usize = 1024;
const CELL_TEXT_MAX: usize = 32;

#[derive(Clone, Debug, Default)]
struct Parser {
    state: State,
    utf8: [u8; 4],
    utf8_len: u8,
    utf8_need: u8,
    vals: [u16; 32],
    len: usize,
    sub: u32,
    cur: u16,
    has_cur: bool,
    next_sub: bool,
    private: u8,
    inter: [u8; 2],
    ninter: u8,
    ignore: bool,
    osc: Vec<u8>,
}

impl Parser {
    fn start_seq(&mut self) {
        self.len = 0;
        self.sub = 0;
        self.cur = 0;
        self.has_cur = false;
        self.next_sub = false;
        self.private = 0;
        self.ninter = 0;
        self.ignore = false;
    }

    fn push_param(&mut self) {
        if self.len < self.vals.len() {
            self.vals[self.len] = self.cur;
            if self.next_sub {
                self.sub |= 1 << self.len;
            }
            self.len += 1;
        }
        self.cur = 0;
        self.has_cur = false;
    }

    fn collect(&mut self, b: u8) {
        if (self.ninter as usize) < self.inter.len() {
            self.inter[self.ninter as usize] = b;
            self.ninter += 1;
        } else {
            self.ignore = true;
        }
    }
}

/// Terminal emulator.
#[derive(Clone, Debug)]
pub struct Vt {
    cols: u16,
    rows: u16,
    grids: [Grid; 2],
    col: u16,
    row: u16,
    pending_wrap: bool,
    visible: bool,
    pen: Style,
    saved: [Saved; 2],
    /// Scroll margins: rows `top..bottom`.
    top: u16,
    bottom: u16,
    modes: Modes,
    mouse_bits: u8,
    tabs: Vec<bool>,
    title: String,
    links: Vec<Link>,
    events: Vec<VtEvent>,
    last_char: Option<char>,
    /// Cell of the glyph just printed; a control or a sequence forgets it.
    pub(crate) last_pos: Option<(u16, u16)>,
    p: Parser,
}

fn default_tabs(cols: u16) -> Vec<bool> {
    (0..cols).map(|c| c % 8 == 0 && c != 0).collect()
}

fn dec_special(c: char) -> char {
    match c {
        '_' => ' ',
        '`' => '◆',
        'a' => '▒',
        'b' => '␉',
        'c' => '␌',
        'd' => '␍',
        'e' => '␊',
        'f' => '°',
        'g' => '±',
        'h' => '␤',
        'i' => '␋',
        'j' => '┘',
        'k' => '┐',
        'l' => '┌',
        'm' => '└',
        'n' => '┼',
        'o' => '⎺',
        'p' => '⎻',
        'q' => '─',
        'r' => '⎼',
        's' => '⎽',
        't' => '├',
        'u' => '┤',
        'v' => '┴',
        'w' => '┬',
        'x' => '│',
        'y' => '≤',
        'z' => '≥',
        '{' => 'π',
        '|' => '≠',
        '}' => '£',
        '~' => '·',
        _ => c,
    }
}

impl Vt {
    pub fn new(cols: u16, rows: u16) -> Vt {
        let cols = cols.max(1);
        let rows = rows.max(1);
        Vt {
            cols,
            rows,
            grids: [Grid::new(cols, rows), Grid::new(cols, rows)],
            col: 0,
            row: 0,
            pending_wrap: false,
            visible: true,
            pen: Style::default(),
            saved: [Saved::default(); 2],
            top: 0,
            bottom: rows,
            modes: Modes::default(),
            mouse_bits: 0,
            tabs: default_tabs(cols),
            title: String::new(),
            links: Vec::new(),
            events: Vec::new(),
            last_char: None,
            last_pos: None,
            p: Parser::default(),
        }
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn rows(&self) -> u16 {
        self.rows
    }

    /// The active screen.
    pub fn grid(&self) -> &Grid {
        &self.grids[self.modes.alt_screen as usize]
    }

    fn grid_mut(&mut self) -> &mut Grid {
        &mut self.grids[self.modes.alt_screen as usize]
    }

    /// Replace the active screen; resizes when `grid` has another size.
    /// Link ids of `grid` must be of this `Vt`.
    pub fn set_grid(&mut self, grid: &Grid) {
        if grid.cols() != self.cols || grid.rows() != self.rows {
            self.resize(grid.cols(), grid.rows());
        }
        if grid.cols() == self.cols && grid.rows() == self.rows {
            *self.grid_mut() = grid.clone();
        }
    }

    pub fn cursor(&self) -> Cursor {
        Cursor {
            col: self.col,
            row: self.row,
            visible: self.visible,
            pending_wrap: self.pending_wrap,
        }
    }

    pub fn pen(&self) -> Style {
        self.pen
    }

    pub fn modes(&self) -> Modes {
        self.modes
    }

    /// Scroll margins: rows `top..bottom`.
    pub fn margins(&self) -> (u16, u16) {
        (self.top, self.bottom)
    }

    /// Between sequences: not inside an escape sequence, a control string
    /// or a UTF-8 sequence.
    pub fn in_ground(&self) -> bool {
        self.p.state == State::Ground
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn link(&self, id: u32) -> Option<&Link> {
        id.checked_sub(1).and_then(|i| self.links.get(i as usize))
    }

    pub fn take_events(&mut self) -> Vec<VtEvent> {
        std::mem::take(&mut self.events)
    }

    /// Same cursor, pen, modes and margins.
    pub fn same_state(&self, other: &Vt) -> bool {
        self.cursor() == other.cursor()
            && self.pen == other.pen
            && self.modes == other.modes
            && self.margins() == other.margins()
    }

    /// New size. No reflow: cropped or padded at the right and bottom. When
    /// the cursor would fall below the main screen, rows leave by the top.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        if cols == self.cols && rows == self.rows {
            return;
        }
        if !self.modes.alt_screen && self.row >= rows {
            let n = self.row - rows + 1;
            let old_rows = self.rows;
            self.grids[0].scroll_up(0, old_rows, n, &Cell::default());
            self.row -= n;
        }
        for grid in &mut self.grids {
            grid.resize(cols, rows);
        }
        self.cols = cols;
        self.rows = rows;
        self.col = self.col.min(cols - 1);
        self.row = self.row.min(rows - 1);
        self.pending_wrap = false;
        self.top = 0;
        self.bottom = rows;
        self.tabs = default_tabs(cols);
        self.last_pos = None;
        for s in &mut self.saved {
            s.col = s.col.min(cols - 1);
            s.row = s.row.min(rows - 1);
            s.pending_wrap = false;
        }
    }

    fn event(&mut self, ev: VtEvent) {
        if let (
            VtEvent::Scroll {
                region,
                dy,
                scrollback,
            },
            Some(VtEvent::Scroll {
                region: r0,
                dy: d0,
                scrollback: s0,
            }),
        ) = (ev, self.events.last_mut())
            && *r0 == region
            && *s0 == scrollback
            && (*d0 < 0) == (dy < 0)
        {
            *d0 = d0.saturating_add(dy);
            return;
        }
        if self.events.len() >= EVENTS_MAX {
            self.events.drain(..EVENTS_MAX / 2);
        }
        self.events.push(ev);
    }

    /// Blank with the pen background, as erasures leave.
    fn fill(&self) -> Cell {
        Cell::blank(Style {
            bg: self.pen.bg,
            ..Style::default()
        })
    }

    // Byte stream.

    pub fn feed(&mut self, bytes: &[u8]) {
        let mut i = 0;
        let n = bytes.len();
        while i < n {
            let b = bytes[i];
            match self.p.state {
                State::Ground => {
                    if (0x20..0x7f).contains(&b) {
                        let mut j = i + 1;
                        while j < n && (0x20..0x7f).contains(&bytes[j]) {
                            j += 1;
                        }
                        self.print_ascii(&bytes[i..j]);
                        i = j;
                    } else if b < 0x20 || b == 0x7f {
                        i += 1;
                        if b == 0x1b {
                            self.p.state = State::Esc;
                            self.p.start_seq();
                        } else {
                            self.execute(b);
                        }
                    } else {
                        let need = match b {
                            0xc2..=0xdf => 2,
                            0xe0..=0xef => 3,
                            0xf0..=0xf4 => 4,
                            _ => 0,
                        };
                        if need == 0 {
                            self.print('\u{fffd}');
                            i += 1;
                        } else if i + need <= n {
                            match std::str::from_utf8(&bytes[i..i + need]) {
                                Ok(s) => {
                                    self.print(s.chars().next().unwrap_or('\u{fffd}'));
                                    i += need;
                                }
                                Err(e) => {
                                    self.print('\u{fffd}');
                                    i += e.error_len().unwrap_or(1).max(1);
                                }
                            }
                        } else {
                            self.p.utf8[0] = b;
                            self.p.utf8_len = 1;
                            self.p.utf8_need = need as u8;
                            self.p.state = State::Utf8;
                            i += 1;
                        }
                    }
                }
                State::Utf8 => {
                    if b & 0xc0 == 0x80 {
                        self.p.utf8[self.p.utf8_len as usize] = b;
                        self.p.utf8_len += 1;
                        i += 1;
                        if self.p.utf8_len == self.p.utf8_need {
                            self.p.state = State::Ground;
                            let len = self.p.utf8_len as usize;
                            let buf = self.p.utf8;
                            let c = std::str::from_utf8(&buf[..len])
                                .ok()
                                .and_then(|s| s.chars().next())
                                .unwrap_or('\u{fffd}');
                            self.print(c);
                        }
                    } else {
                        // Truncated sequence: the byte is looked at again.
                        self.p.state = State::Ground;
                        self.print('\u{fffd}');
                    }
                }
                State::Esc | State::EscInter => {
                    i += 1;
                    match b {
                        0x18 | 0x1a => self.p.state = State::Ground,
                        0x1b => {
                            self.p.state = State::Esc;
                            self.p.start_seq();
                        }
                        0x00..=0x1f => self.execute(b),
                        0x20..=0x2f => {
                            self.p.collect(b);
                            self.p.state = State::EscInter;
                        }
                        0x30..=0x7e if self.p.state == State::Esc => match b {
                            b'[' => self.p.state = State::Csi,
                            b']' => {
                                self.p.osc.clear();
                                self.p.state = State::Osc;
                            }
                            b'P' | b'X' | b'^' | b'_' => self.p.state = State::Str,
                            _ => {
                                self.p.state = State::Ground;
                                self.esc(b);
                            }
                        },
                        0x30..=0x7e => {
                            self.p.state = State::Ground;
                            if !self.p.ignore {
                                self.esc(b);
                            }
                        }
                        0x7f => {}
                        _ => self.p.state = State::Ground,
                    }
                }
                State::Csi => {
                    i += 1;
                    match b {
                        b'0'..=b'9' => {
                            if self.p.ninter > 0 {
                                self.p.ignore = true;
                            }
                            self.p.cur = self
                                .p
                                .cur
                                .saturating_mul(10)
                                .saturating_add((b - b'0') as u16);
                            self.p.has_cur = true;
                        }
                        b';' | b':' => {
                            self.p.push_param();
                            self.p.next_sub = b == b':';
                            self.p.has_cur = true;
                        }
                        0x3c..=0x3f => {
                            if self.p.len == 0 && !self.p.has_cur && self.p.private == 0 {
                                self.p.private = b;
                            } else {
                                self.p.ignore = true;
                            }
                        }
                        0x20..=0x2f => self.p.collect(b),
                        0x40..=0x7e => {
                            if self.p.has_cur {
                                self.p.push_param();
                            }
                            self.p.state = State::Ground;
                            if !self.p.ignore {
                                let params = Params {
                                    vals: self.p.vals,
                                    len: self.p.len,
                                    sub: self.p.sub,
                                };
                                self.csi(&params, b);
                            }
                        }
                        0x18 | 0x1a => self.p.state = State::Ground,
                        0x1b => {
                            self.p.state = State::Esc;
                            self.p.start_seq();
                        }
                        0x00..=0x1f => self.execute(b),
                        0x7f => {}
                        _ => self.p.state = State::Ground,
                    }
                }
                State::Osc => {
                    // Bulk copy up to the next control byte.
                    let mut j = i;
                    while j < n && !matches!(bytes[j], 0x07 | 0x18 | 0x1a | 0x1b) {
                        j += 1;
                    }
                    let room = OSC_MAX.saturating_sub(self.p.osc.len());
                    self.p.osc.extend_from_slice(&bytes[i..j.min(i + room)]);
                    i = j;
                    if i < n {
                        match bytes[i] {
                            0x07 => {
                                self.p.state = State::Ground;
                                self.osc();
                            }
                            0x1b => self.p.state = State::OscEsc,
                            _ => self.p.state = State::Ground,
                        }
                        i += 1;
                    }
                }
                State::OscEsc => {
                    self.osc();
                    if b == b'\\' {
                        self.p.state = State::Ground;
                        i += 1;
                    } else {
                        // Not ST: an escape sequence starts here.
                        self.p.state = State::Esc;
                        self.p.start_seq();
                    }
                }
                State::Str => {
                    let mut j = i;
                    while j < n && !matches!(bytes[j], 0x18 | 0x1a | 0x1b) {
                        j += 1;
                    }
                    i = j;
                    if i < n {
                        self.p.state = if bytes[i] == 0x1b {
                            State::StrEsc
                        } else {
                            State::Ground
                        };
                        i += 1;
                    }
                }
                State::StrEsc => {
                    if b == b'\\' {
                        self.p.state = State::Ground;
                        i += 1;
                    } else {
                        self.p.state = State::Esc;
                        self.p.start_seq();
                    }
                }
            }
        }
    }

    // Printing.

    fn wrap_if_pending(&mut self) {
        if self.pending_wrap {
            self.pending_wrap = false;
            self.col = 0;
            self.index();
        }
    }

    fn advance(&mut self, w: u16) {
        self.col += w;
        if self.col >= self.cols {
            self.col = self.cols - 1;
            self.pending_wrap = self.modes.autowrap;
        }
    }

    fn print_ascii(&mut self, run: &[u8]) {
        if self.modes.charset() != Charset::Ascii {
            for &b in run {
                self.print(b as char);
            }
            return;
        }
        let pen = self.pen;
        let insert = self.modes.insert;
        for &b in run {
            self.wrap_if_pending();
            let (col, row) = (self.col, self.row);
            let cell = Cell {
                text: SmolStr::new_inline(std::str::from_utf8(&[b]).unwrap_or(" ")),
                width: 1,
                style: pen,
            };
            if insert {
                let fill = self.fill();
                self.grid_mut().insert_cells(col, row, 1, &fill);
            }
            self.grid_mut().put(col, row, cell);
            self.advance(1);
            self.last_pos = Some((col, row));
        }
        self.last_char = run.last().map(|&b| b as char);
    }

    fn print(&mut self, c: char) {
        let c = if self.modes.charset() == Charset::DecSpecial {
            dec_special(c)
        } else {
            c
        };
        let w = if c.is_ascii() {
            1
        } else {
            c.width().unwrap_or(0)
        };
        if w == 0 {
            self.combine(c);
            return;
        }
        let w = w as u16;
        if w > self.cols {
            self.last_char = Some(c);
            return;
        }
        self.wrap_if_pending();
        if w == 2 && self.col + 1 >= self.cols {
            if self.modes.autowrap {
                // The glyph does not fit: the last column is left blank.
                let fill = self.fill();
                let (col, row) = (self.col, self.row);
                self.grid_mut().put(col, row, fill);
                self.col = 0;
                self.index();
            } else {
                self.col = self.cols - 2;
            }
        }
        let (col, row) = (self.col, self.row);
        if self.modes.insert {
            let fill = self.fill();
            self.grid_mut().insert_cells(col, row, w, &fill);
        }
        let mut buf = [0u8; 4];
        let text = SmolStr::new_inline(c.encode_utf8(&mut buf));
        let cell = Cell {
            text,
            width: w as u8,
            style: self.pen,
        };
        self.grid_mut().put(col, row, cell);
        self.advance(w);
        self.last_char = Some(c);
        self.last_pos = Some((col, row));
    }

    /// A zero-width character joins the glyph just printed.
    fn combine(&mut self, c: char) {
        let Some((col, row)) = self.last_pos else {
            return;
        };
        if col >= self.cols || row >= self.rows {
            return;
        }
        let cell = self.grid_mut().cell_mut(col, row);
        if cell.width != 0 && cell.text.len() + c.len_utf8() <= CELL_TEXT_MAX {
            let mut s = String::with_capacity(cell.text.len() + 4);
            s.push_str(&cell.text);
            s.push(c);
            cell.text = SmolStr::new(s);
        }
    }

    // Vertical motion.

    fn scroll_up(&mut self, n: u16) {
        let n = n.min(self.bottom - self.top);
        if n == 0 {
            return;
        }
        let fill = self.fill();
        let (top, bottom) = (self.top, self.bottom);
        self.grid_mut().scroll_up(top, bottom, n, &fill);
        self.event(VtEvent::Scroll {
            region: Rect::new(0, top, self.cols, bottom - top),
            dy: -(n as i16),
            scrollback: !self.modes.alt_screen && top == 0,
        });
    }

    fn scroll_down(&mut self, n: u16) {
        let n = n.min(self.bottom - self.top);
        if n == 0 {
            return;
        }
        let fill = self.fill();
        let (top, bottom) = (self.top, self.bottom);
        self.grid_mut().scroll_down(top, bottom, n, &fill);
        self.event(VtEvent::Scroll {
            region: Rect::new(0, top, self.cols, bottom - top),
            dy: n as i16,
            scrollback: false,
        });
    }

    /// IND: down one row, scrolling at the bottom margin.
    fn index(&mut self) {
        if self.row + 1 == self.bottom {
            self.scroll_up(1);
        } else if self.row + 1 < self.rows {
            self.row += 1;
        }
    }

    /// RI: up one row, scrolling at the top margin.
    fn reverse_index(&mut self) {
        if self.row == self.top {
            self.scroll_down(1);
        } else if self.row > 0 {
            self.row -= 1;
        }
    }

    fn insert_lines(&mut self, n: u16) {
        if self.row < self.top || self.row >= self.bottom {
            return;
        }
        let n = n.min(self.bottom - self.row);
        let fill = self.fill();
        let (row, bottom) = (self.row, self.bottom);
        self.grid_mut().scroll_down(row, bottom, n, &fill);
        self.event(VtEvent::Scroll {
            region: Rect::new(0, row, self.cols, bottom - row),
            dy: n as i16,
            scrollback: false,
        });
        self.col = 0;
        self.pending_wrap = false;
    }

    fn delete_lines(&mut self, n: u16) {
        if self.row < self.top || self.row >= self.bottom {
            return;
        }
        let n = n.min(self.bottom - self.row);
        let fill = self.fill();
        let (row, bottom) = (self.row, self.bottom);
        self.grid_mut().scroll_up(row, bottom, n, &fill);
        self.event(VtEvent::Scroll {
            region: Rect::new(0, row, self.cols, bottom - row),
            dy: -(n as i16),
            scrollback: false,
        });
        self.col = 0;
        self.pending_wrap = false;
    }

    // Cursor.

    fn goto(&mut self, col: u16, row: u16) {
        self.col = col.min(self.cols - 1);
        self.row = if self.modes.origin {
            (self.top.saturating_add(row)).min(self.bottom - 1)
        } else {
            row.min(self.rows - 1)
        };
        self.pending_wrap = false;
        self.last_pos = None;
    }

    fn up(&mut self, n: u16) {
        let min = if self.row >= self.top { self.top } else { 0 };
        self.row = self.row.saturating_sub(n).max(min);
        self.pending_wrap = false;
    }

    fn down(&mut self, n: u16) {
        let max = if self.row < self.bottom {
            self.bottom - 1
        } else {
            self.rows - 1
        };
        self.row = self.row.saturating_add(n).min(max);
        self.pending_wrap = false;
    }

    fn tab(&mut self, n: u16) {
        for _ in 0..n {
            let mut c = self.col + 1;
            while c < self.cols && !self.tabs[c as usize] {
                c += 1;
            }
            self.col = c.min(self.cols - 1);
        }
    }

    fn back_tab(&mut self, n: u16) {
        for _ in 0..n {
            let mut c = self.col.saturating_sub(1);
            while c > 0 && !self.tabs[c as usize] {
                c -= 1;
            }
            self.col = c;
        }
        self.pending_wrap = false;
    }

    fn save_cursor(&mut self) {
        self.saved[self.modes.alt_screen as usize] = Saved {
            col: self.col,
            row: self.row,
            pending_wrap: self.pending_wrap,
            pen: self.pen,
            origin: self.modes.origin,
            g0: self.modes.g0,
            g1: self.modes.g1,
            shift_out: self.modes.shift_out,
        };
    }

    fn restore_cursor(&mut self) {
        let s = self.saved[self.modes.alt_screen as usize];
        self.last_pos = None;
        self.col = s.col.min(self.cols - 1);
        self.row = s.row.min(self.rows - 1);
        self.pen = s.pen;
        self.modes.origin = s.origin;
        self.modes.g0 = s.g0;
        self.modes.g1 = s.g1;
        self.modes.shift_out = s.shift_out;
        if s.origin {
            self.row = self.row.clamp(self.top, self.bottom - 1);
        }
        self.pending_wrap = s.pending_wrap && self.modes.autowrap && self.col == self.cols - 1;
    }

    // Controls.

    fn execute(&mut self, b: u8) {
        if !matches!(b, 0x07 | 0x0e | 0x0f) {
            self.last_pos = None;
        }
        match b {
            0x07 => self.event(VtEvent::Bell),
            0x08 => {
                self.col = self.col.saturating_sub(1);
                self.pending_wrap = false;
            }
            0x09 => self.tab(1),
            0x0a..=0x0c => {
                self.index();
                if self.modes.linefeed {
                    self.col = 0;
                }
                self.pending_wrap = false;
            }
            0x0d => {
                self.col = 0;
                self.pending_wrap = false;
            }
            0x0e => self.modes.shift_out = true,
            0x0f => self.modes.shift_out = false,
            _ => {}
        }
    }

    fn esc(&mut self, b: u8) {
        if self.p.ninter == 0 && !matches!(b, b'=' | b'>') {
            self.last_pos = None;
        }
        let inter = self.p.inter;
        match (&inter[..self.p.ninter as usize], b) {
            ([], b'7') => self.save_cursor(),
            ([], b'8') => self.restore_cursor(),
            ([], b'D') => {
                self.index();
                self.pending_wrap = false;
            }
            ([], b'E') => {
                self.index();
                self.col = 0;
                self.pending_wrap = false;
            }
            ([], b'M') => {
                self.reverse_index();
                self.pending_wrap = false;
            }
            ([], b'H') => self.tabs[self.col as usize] = true,
            ([], b'c') => self.reset(),
            ([], b'=') => self.modes.app_keypad = true,
            ([], b'>') => self.modes.app_keypad = false,
            ([b'('], f) => self.modes.g0 = charset(f),
            ([b')'], f) => self.modes.g1 = charset(f),
            ([b'#'], b'8') => {
                let cell = Cell::narrow("E", Style::default());
                for row in 0..self.rows {
                    self.grid_mut().clear_row(row, &cell);
                }
                self.top = 0;
                self.bottom = self.rows;
                self.col = 0;
                self.row = 0;
                self.pending_wrap = false;
            }
            _ => {}
        }
    }

    /// RIS.
    fn reset(&mut self) {
        let was_alt = self.modes.alt_screen;
        let events = std::mem::take(&mut self.events);
        *self = Vt::new(self.cols, self.rows);
        self.events = events;
        if was_alt {
            self.event(VtEvent::AltScreen(false));
        }
        self.event(VtEvent::Clear);
    }

    fn set_alt(&mut self, on: bool, clear: bool) {
        if self.modes.alt_screen == on {
            return;
        }
        self.modes.alt_screen = on;
        if on && clear {
            let fill = self.fill();
            for row in 0..self.rows {
                self.grids[1].clear_row(row, &fill);
            }
        }
        self.pending_wrap = false;
        self.last_pos = None;
        self.event(VtEvent::AltScreen(on));
    }

    fn set_mode(&mut self, p: &Params, on: bool) {
        for i in 0..p.len {
            let code = p.vals[i];
            if self.p.private == b'?' {
                match code {
                    1 => self.modes.app_cursor = on,
                    6 => {
                        self.modes.origin = on;
                        self.goto(0, 0);
                    }
                    7 => {
                        self.modes.autowrap = on;
                        if !on {
                            self.pending_wrap = false;
                        }
                        self.last_pos = None;
                    }
                    25 => self.visible = on,
                    9 | 1000 | 1002 | 1003 => {
                        let bit = match code {
                            9 => 1,
                            1000 => 2,
                            1002 => 4,
                            _ => 8,
                        };
                        if on {
                            self.mouse_bits |= bit;
                        } else {
                            self.mouse_bits &= !bit;
                        }
                        self.modes.mouse = self.mouse_bits != 0;
                    }
                    1004 => self.modes.focus_events = on,
                    47 | 1047 => self.set_alt(on, false),
                    1048 => {
                        if on {
                            self.save_cursor();
                        } else {
                            self.restore_cursor();
                        }
                    }
                    1049 => {
                        if on {
                            self.save_cursor();
                            self.set_alt(true, true);
                        } else {
                            self.set_alt(false, false);
                            self.restore_cursor();
                        }
                    }
                    2004 => self.modes.bracketed_paste = on,
                    2026 => self.modes.sync_update = on,
                    _ => {}
                }
            } else if self.p.private == 0 {
                match code {
                    4 => self.modes.insert = on,
                    20 => self.modes.linefeed = on,
                    _ => {}
                }
            }
        }
    }

    fn erase_display(&mut self, mode: u16) {
        let fill = self.fill();
        let (col, row, cols, rows) = (self.col, self.row, self.cols, self.rows);
        let grid = self.grid_mut();
        match mode {
            0 => {
                grid.erase(row, col, cols, &fill);
                for r in row + 1..rows {
                    grid.clear_row(r, &fill);
                }
            }
            1 => {
                for r in 0..row {
                    grid.clear_row(r, &fill);
                }
                grid.erase(row, 0, col + 1, &fill);
            }
            2 => {
                for r in 0..rows {
                    grid.clear_row(r, &fill);
                }
                self.event(VtEvent::Clear);
            }
            _ => {}
        }
    }

    fn erase_line(&mut self, mode: u16) {
        let fill = self.fill();
        let (col, row, cols) = (self.col, self.row, self.cols);
        let grid = self.grid_mut();
        match mode {
            0 => grid.erase(row, col, cols, &fill),
            1 => grid.erase(row, 0, col + 1, &fill),
            2 => grid.clear_row(row, &fill),
            _ => {}
        }
    }

    fn csi(&mut self, p: &Params, fin: u8) {
        let private = self.p.private;
        let inter = self.p.inter;
        let inter = &inter[..self.p.ninter as usize];
        let n = p.get(0, 1);
        if !matches!(fin, b'm' | b'h' | b'l' | b'q') {
            self.last_pos = None;
        }
        match (private, inter, fin) {
            (0, [], b'A') => self.up(n),
            (0, [], b'B' | b'e') => self.down(n),
            (0, [], b'C' | b'a') => {
                self.col = self.col.saturating_add(n).min(self.cols - 1);
                self.pending_wrap = false;
            }
            (0, [], b'D') => {
                self.col = self.col.saturating_sub(n);
                self.pending_wrap = false;
            }
            (0, [], b'E') => {
                self.down(n);
                self.col = 0;
            }
            (0, [], b'F') => {
                self.up(n);
                self.col = 0;
            }
            (0, [], b'G' | b'`') => {
                self.col = (n - 1).min(self.cols - 1);
                self.pending_wrap = false;
            }
            (0, [], b'H' | b'f') => self.goto(p.get(1, 1) - 1, n - 1),
            (0, [], b'd') => self.goto(self.col, n - 1),
            (0, [], b'I') => self.tab(n),
            (0, [], b'Z') => self.back_tab(n),
            (0 | b'?', [], b'J') => self.erase_display(p.get(0, 0)),
            (0 | b'?', [], b'K') => self.erase_line(p.get(0, 0)),
            (0, [], b'L') => self.insert_lines(n),
            (0, [], b'M') => self.delete_lines(n),
            (0, [], b'@') => {
                let fill = self.fill();
                let (col, row) = (self.col, self.row);
                self.grid_mut().insert_cells(col, row, n, &fill);
            }
            (0, [], b'P') => {
                let fill = self.fill();
                let (col, row) = (self.col, self.row);
                self.grid_mut().delete_cells(col, row, n, &fill);
            }
            (0, [], b'X') => {
                let fill = self.fill();
                let (col, row) = (self.col, self.row);
                self.grid_mut()
                    .erase(row, col, col.saturating_add(n), &fill);
            }
            (0, [], b'S') => self.scroll_up(n),
            (0, [], b'T') if p.len <= 1 => self.scroll_down(n),
            (0, [], b'b') => {
                if let Some(c) = self.last_char {
                    for _ in 0..n.min(self.cols.saturating_mul(self.rows)) {
                        self.print(c);
                    }
                }
            }
            (0, [], b'g') => match p.get(0, 0) {
                0 => self.tabs[self.col as usize] = false,
                3 => self.tabs.fill(false),
                _ => {}
            },
            (_, [], b'h') => self.set_mode(p, true),
            (_, [], b'l') => self.set_mode(p, false),
            (0, [], b'm') => sgr::apply(&mut self.pen, p),
            (0, [], b'r') => {
                let top = n - 1;
                let bottom = p.get(1, self.rows).min(self.rows);
                if top + 1 < bottom {
                    self.top = top;
                    self.bottom = bottom;
                    self.goto(0, 0);
                }
            }
            (0, [], b's') if p.len == 0 => self.save_cursor(),
            (0, [], b'u') if p.len == 0 => self.restore_cursor(),
            (0, [b' '], b'q') => self.modes.cursor_shape = p.get(0, 0).min(255) as u8,
            (0, [b'!'], b'p') => {
                // DECSTR.
                self.modes.insert = false;
                self.modes.origin = false;
                self.modes.autowrap = true;
                self.modes.g0 = Charset::Ascii;
                self.modes.g1 = Charset::Ascii;
                self.modes.shift_out = false;
                self.modes.app_cursor = false;
                self.modes.app_keypad = false;
                self.visible = true;
                self.top = 0;
                self.bottom = self.rows;
                self.pen = Style {
                    link: self.pen.link,
                    ..Style::default()
                };
                self.saved[self.modes.alt_screen as usize] = Saved::default();
                self.pending_wrap = false;
            }
            _ => {}
        }
    }

    fn osc(&mut self) {
        let buf = std::mem::take(&mut self.p.osc);
        let mut parts = buf.splitn(2, |&b| b == b';');
        let code = parts.next().unwrap_or(&[]);
        let rest = parts.next().unwrap_or(&[]);
        match code {
            b"0" | b"2" => {
                let mut title = String::from_utf8_lossy(rest).into_owned();
                if title.len() > 1024 {
                    let mut cut = 1024;
                    while !title.is_char_boundary(cut) {
                        cut -= 1;
                    }
                    title.truncate(cut);
                }
                self.title = title;
                self.event(VtEvent::Title);
            }
            b"8" => {
                let mut parts = rest.splitn(2, |&b| b == b';');
                let params = String::from_utf8_lossy(parts.next().unwrap_or(&[])).into_owned();
                let uri = String::from_utf8_lossy(parts.next().unwrap_or(&[])).into_owned();
                self.pen.link = if uri.is_empty() {
                    0
                } else {
                    self.intern(params, uri)
                };
            }
            _ => {}
        }
        self.p.osc = buf;
        self.p.osc.clear();
    }

    /// Id of a link, 0 when the table is full.
    fn intern(&mut self, params: String, uri: String) -> u32 {
        // Recent links first: a link is usually reopened soon.
        if let Some(i) = self
            .links
            .iter()
            .rposition(|l| l.uri == uri && l.params == params)
        {
            return i as u32 + 1;
        }
        if self.links.len() >= LINKS_MAX {
            return 0;
        }
        self.links.push(Link { params, uri });
        self.links.len() as u32
    }
}

fn charset(f: u8) -> Charset {
    if f == b'0' {
        Charset::DecSpecial
    } else {
        Charset::Ascii
    }
}
