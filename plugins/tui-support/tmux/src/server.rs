// SPDX-License-Identifier: GPL-3.0-or-later
//! A tmux server, asked through the `tmux` program: its sessions, their
//! clients and options, and the swap of a client for another program.

use std::process::{Command, Stdio};

/// Session option naming the ptyrc session that wraps a client of it.
pub const OPTION: &str = "@ptyrc";
/// Session option holding why a wrap did not start.
pub const OPTION_ERROR: &str = "@ptyrc-error";

/// A tmux server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Server {
    /// What tells `tmux` which one: `-S path`, `-L name`, or nothing.
    select: Vec<String>,
}

/// A session of a server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    /// `$3`: stays through a rename.
    pub id: String,
    pub name: String,
}

/// A client attached to a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Client {
    pub tty: String,
    pub pid: u32,
    /// Time of its last activity, in seconds.
    pub activity: u64,
}

impl Server {
    /// The server of the pane this process runs in: the socket of `$TMUX`.
    pub fn enclosing() -> Option<Server> {
        let tmux = std::env::var("TMUX").ok()?;
        let socket = tmux.split(',').next().filter(|s| !s.is_empty())?;
        Some(Server::at(socket))
    }

    /// The server of a socket path: `tmux -S`.
    pub fn at(socket: impl Into<String>) -> Server {
        Server {
            select: vec!["-S".into(), socket.into()],
        }
    }

    /// The server of a socket name: `tmux -L`.
    pub fn named(name: impl Into<String>) -> Server {
        Server {
            select: vec!["-L".into(), name.into()],
        }
    }

    /// The server `tmux` talks to when told nothing.
    pub fn default_server() -> Server {
        Server { select: Vec::new() }
    }

    /// The path of its socket.
    pub fn socket(&self) -> Result<String, String> {
        self.run(&["display-message", "-p", "#{socket_path}"])
    }

    /// `tmux [-S socket] WORDS...`.
    pub fn argv(&self, words: &[&str]) -> Vec<String> {
        let mut argv = vec!["tmux".to_string()];
        argv.extend(self.select.iter().cloned());
        argv.extend(words.iter().map(|w| w.to_string()));
        argv
    }

    /// The command of a client attached to `session`.
    pub fn attach_argv(&self, session: &Session) -> Vec<String> {
        self.argv(&["attach-session", "-t", &session.id])
    }

    /// Run a tmux command: its output without the last newline, or what
    /// it said on failure.
    pub fn run(&self, words: &[&str]) -> Result<String, String> {
        let argv = self.argv(words);
        let output = Command::new(&argv[0])
            .args(&argv[1..])
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("tmux: {e}"))?;
        let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).trim_end().to_string();
        if output.status.success() {
            Ok(text(&output.stdout))
        } else {
            Err(text(&output.stderr))
        }
    }

    /// The session of `target` (a session, a window or a pane); `None`:
    /// the one tmux takes as current.
    pub fn session(&self, target: Option<&str>) -> Result<Session, String> {
        let mut words = vec!["display-message", "-p"];
        if let Some(target) = target {
            words.extend(["-t", target]);
        }
        words.push("#{session_id}\t#{session_name}");
        let line = self.run(&words)?;
        match line.split_once('\t') {
            Some((id, name)) if id.starts_with('$') => Ok(Session {
                id: id.to_string(),
                name: name.to_string(),
            }),
            _ => Err("no session".into()),
        }
    }

    /// The clients attached to `session` with a terminal, the last
    /// active first; `None`: those of every session.
    pub fn clients(&self, session: Option<&Session>) -> Result<Vec<Client>, String> {
        let format = "#{client_control_mode}\t#{client_activity}\t#{client_pid}\t#{client_tty}";
        let mut words = vec!["list-clients", "-F", format];
        if let Some(session) = session {
            words.extend(["-t", &session.id]);
        }
        let lines = self.run(&words)?;
        let mut clients: Vec<Client> = lines.lines().filter_map(parse_client).collect();
        clients.sort_by_key(|c| std::cmp::Reverse(c.activity));
        Ok(clients)
    }

    /// A user option of `session`; `None` when unset or empty.
    pub fn option(&self, session: &Session, name: &str) -> Option<String> {
        let value = self
            .run(&["show-options", "-qv", "-t", &session.id, name])
            .ok()?;
        (!value.is_empty()).then_some(value)
    }

    pub fn set_option(&self, session: &Session, name: &str, value: &str) -> Result<(), String> {
        self.run(&["set-option", "-t", &session.id, name, value])
            .map(drop)
    }

    pub fn unset_option(&self, session: &Session, name: &str) {
        let _ = self.run(&["set-option", "-u", "-t", &session.id, name]);
    }

    /// Replace the client on `tty` by `argv`, run in its place on its
    /// terminal, with its environment. The client says nothing as it
    /// leaves.
    pub fn swap(&self, tty: &str, argv: &[String]) -> Result<(), String> {
        let words: Vec<String> = argv.iter().map(|w| quote(w)).collect();
        let command = format!("exec {}", words.join(" "));
        self.run(&["detach-client", "-t", tty, "-E", &command])
            .map(drop)
    }
}

/// A line of `Server::clients`; `None` for a control client.
fn parse_client(line: &str) -> Option<Client> {
    let mut fields = line.splitn(4, '\t');
    let control = fields.next()?;
    let activity = fields.next()?.parse().ok()?;
    let pid = fields.next()?.parse().ok()?;
    let tty = fields.next().filter(|t| !t.is_empty())?;
    (control != "1").then(|| Client {
        tty: tty.to_string(),
        pid,
        activity,
    })
}

/// A word for `sh`.
pub fn quote(word: &str) -> String {
    let plain = |b: u8| b.is_ascii_alphanumeric() || b"_-./=:,@%+".contains(&b);
    if !word.is_empty() && word.bytes().all(plain) {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

/// The name of the ptyrc session over the tmux session `name`:
/// `tmux-<name>`, what a session name cannot hold replaced by `_`.
pub fn session_name(name: &str) -> String {
    let kept: String = name
        .chars()
        .take(48)
        .map(|c| match c {
            c if c.is_ascii_alphanumeric() => c,
            '.' | '_' | '-' => c,
            _ => '_',
        })
        .collect();
    format!("tmux-{kept}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words() {
        assert_eq!(quote("/usr/bin/ptyrc"), "/usr/bin/ptyrc");
        assert_eq!(quote("$3"), "'$3'");
        assert_eq!(quote("fishes -a"), "'fishes -a'");
        assert_eq!(quote("it's"), r"'it'\''s'");
        assert_eq!(quote(""), "''");
    }

    #[test]
    fn names() {
        assert_eq!(session_name("main"), "tmux-main");
        assert_eq!(session_name("my work/2"), "tmux-my_work_2");
        assert_eq!(session_name("é"), "tmux-_");
        assert_eq!(session_name(&"x".repeat(80)).len(), 53);
    }

    #[test]
    fn clients() {
        let client = parse_client("0\t1794571200\t4242\t/dev/pts/7").unwrap();
        assert_eq!(client.tty, "/dev/pts/7");
        assert_eq!((client.pid, client.activity), (4242, 1794571200));
        assert_eq!(parse_client("1\t1794571200\t4242\t/dev/pts/7"), None);
        assert_eq!(parse_client("0\t1794571200\t4242\t"), None);
        assert_eq!(parse_client(""), None);
    }

    #[test]
    fn commands() {
        let session = Session {
            id: "$3".into(),
            name: "main".into(),
        };
        assert_eq!(
            Server::at("/tmp/s").attach_argv(&session),
            ["tmux", "-S", "/tmp/s", "attach-session", "-t", "$3"]
        );
        assert_eq!(
            Server::default_server().argv(&["kill-server"]),
            ["tmux", "kill-server"]
        );
        assert_eq!(Server::named("x").argv(&["ls"]), ["tmux", "-L", "x", "ls"]);
    }
}
