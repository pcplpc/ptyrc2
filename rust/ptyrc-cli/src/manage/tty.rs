// SPDX-License-Identifier: GPL-3.0-or-later
//! The menu on this terminal: alternate screen, raw keys.

use super::menu::{Menu, Row, Style};
use super::ui::{Flow, Runner, Session};
use crate::Fail;
use ptyrc_proto::{Client, ClientError, Event};
use ptyrc_server::host::Host;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::process::ExitCode;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

/// The menu takes no more of a large terminal than this.
const MAX: (usize, usize) = (100, 30);
/// The times shown are counted again this often.
const BEAT: Duration = Duration::from_secs(1);

enum Msg {
    Keys(Vec<u8>),
    Event(Event),
    /// The keyboard or the session is gone.
    Closed,
}

/// This terminal in raw mode, on its alternate screen; as it was when
/// dropped.
pub struct Term {
    host: Host,
    sender: Sender<Msg>,
    inbox: Receiver<Msg>,
}

/// SGR of a run.
fn sgr(style: Style) -> String {
    let mut out = String::from("\x1b[0");
    let colour = match style.fg {
        Some("red") => ";31",
        Some("green") => ";32",
        Some("yellow") => ";33",
        _ => "",
    };
    out.push_str(colour);
    for (on, code) in [(style.bold, ";1"), (style.dim, ";2"), (style.inverse, ";7")] {
        if on {
            out.push_str(code);
        }
    }
    out.push('m');
    out
}

/// The names of the keys in `bytes`, as `input.grab` names them.
pub fn keys(bytes: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        let (key, tail) = rest.split_at(ptyrc_server::key_len(rest).max(1));
        rest = tail;
        names.extend(ptyrc_server::key_name(key));
    }
    names
}

impl Term {
    pub fn open() -> Result<Term, Fail> {
        let host = Host::terminal()?;
        let (sender, inbox) = channel();
        let typed = sender.clone();
        std::thread::spawn(move || {
            let mut stdin = std::io::stdin().lock();
            let mut buf = [0u8; 1024];
            loop {
                let msg = match stdin.read(&mut buf) {
                    Ok(0) | Err(_) => Msg::Closed,
                    Ok(n) => Msg::Keys(buf[..n].to_vec()),
                };
                let closed = matches!(msg, Msg::Closed);
                if typed.send(msg).is_err() || closed {
                    break;
                }
            }
        });
        let mut term = Term {
            host,
            sender,
            inbox,
        };
        term.host.write(b"\x1b[?1049h\x1b[?25l")?;
        Ok(term)
    }

    fn size(&self) -> (usize, usize) {
        let (cols, rows) = self.host.size().unwrap_or((80, 24));
        ((cols as usize).min(MAX.0), (rows as usize).min(MAX.1))
    }

    fn draw(&mut self, menu: &Menu, (w, h): (usize, usize)) -> Result<(), Fail> {
        let mut out = String::from("\x1b[H");
        let rows: Vec<Row> = menu.render(w, h);
        for (i, row) in rows.iter().enumerate() {
            for piece in row {
                out.push_str(&sgr(piece.style));
                out.push_str(&piece.text);
            }
            out.push_str("\x1b[0m\x1b[K");
            if i + 1 < rows.len() {
                out.push_str("\r\n");
            }
        }
        let _ = write!(out, "\x1b[J");
        Ok(self.host.write(out.as_bytes())?)
    }

    /// Ask which session. `None`: none.
    pub fn pick(&mut self, names: Vec<String>) -> Option<String> {
        let mut menu = Menu::sessions(names);
        loop {
            self.draw(&menu, self.size()).ok()?;
            match self.inbox.recv_timeout(BEAT) {
                Ok(Msg::Keys(bytes)) => {
                    for key in keys(&bytes) {
                        for action in menu.key(&key) {
                            match action {
                                super::menu::Action::Session(name) => return Some(name),
                                super::menu::Action::Quit => return None,
                                _ => {}
                            }
                        }
                    }
                }
                Ok(_) | Err(RecvTimeoutError::Disconnected) => return None,
                Err(RecvTimeoutError::Timeout) => {}
            }
        }
    }

    /// The menu on a session, until the user leaves or the session ends.
    pub fn run(mut self, client: Client) -> Result<ExitCode, Fail> {
        // The events come on a connection of their own: a call never
        // waits behind them.
        let mut events = Client::connect_session(client.hello()["session"].as_str())?;
        events.subscribe(&["apps", "frames", "exit"])?;
        let told = self.sender.clone();
        std::thread::spawn(move || {
            loop {
                let msg = match events.next_event(None) {
                    Ok(Some(event)) if event.event == "exit" => Msg::Closed,
                    Ok(Some(event)) => Msg::Event(event),
                    Ok(None) => continue,
                    Err(ClientError::Closed) | Err(_) => Msg::Closed,
                };
                let closed = matches!(msg, Msg::Closed);
                if told.send(msg).is_err() || closed {
                    break;
                }
            }
        });
        let (session, catalog) = Session::new(client);
        let name = session.name();
        let seed = std::process::id() as u64 ^ (Instant::now().elapsed().as_nanos() as u64 + 1);
        let mut menu = Menu::new(&name, seed);
        menu.catalog(catalog);
        let mut runner = Runner::new(menu, session);
        let mut beat = Instant::now();
        let ended = loop {
            self.draw(&runner.menu, self.size())?;
            let now = Instant::now();
            let until = runner
                .deadline()
                .map_or(beat + BEAT, |d| d.min(beat + BEAT));
            match self
                .inbox
                .recv_timeout(until.saturating_duration_since(now))
            {
                Ok(Msg::Keys(bytes)) => {
                    let mut flow = Flow::Stay;
                    for key in keys(&bytes) {
                        flow = runner.key(&key, Instant::now());
                        if flow != Flow::Stay {
                            break;
                        }
                    }
                    if flow != Flow::Stay {
                        break false;
                    }
                }
                Ok(Msg::Event(event)) => runner.event(&event.event, &event.data),
                Ok(Msg::Closed) | Err(RecvTimeoutError::Disconnected) => break true,
                Err(RecvTimeoutError::Timeout) => {}
            }
            let now = Instant::now();
            runner.tick(now);
            if now >= beat + BEAT {
                runner.beat();
                beat = now;
            }
        };
        drop(self);
        if ended {
            eprintln!("ptyrc: the session ended: {name}");
        }
        Ok(ExitCode::SUCCESS)
    }
}

impl Drop for Term {
    fn drop(&mut self) {
        let _ = self.host.write(b"\x1b[0m\x1b[?25h\x1b[?1049l");
        let _ = std::io::stdout().flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_named_as_grabbed() {
        let typed = b"a\x1b[A\x1b[B\r \x7f\x1bOC\x14\xc3\xa9\x1b[<0;1;1M";
        let wanted = [
            "a",
            "up",
            "down",
            "enter",
            "space",
            "backspace",
            "right",
            "ctrl+t",
            "é",
        ];
        assert_eq!(keys(typed), wanted);
        // esc is esc when alone in its read.
        assert_eq!(keys(b"\x1b"), ["esc"]);
    }

    #[test]
    fn styles() {
        let style = Style {
            fg: Some("green"),
            bold: true,
            ..Style::default()
        };
        assert_eq!(sgr(style), "\x1b[0;32;1m");
        assert_eq!(sgr(Style::default()), "\x1b[0m");
    }
}
