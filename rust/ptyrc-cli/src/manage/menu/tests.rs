// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::manage::catalog;
use serde_json::json;

pub(crate) fn info(id: u64, words: &str, state: &str) -> AppInfo {
    let mut argv = vec!["/usr/bin/ptyrc-app".to_string()];
    argv.extend(super::words(words));
    AppInfo::from_json(&json!({
        "id": id, "kind": "child", "name": argv[1], "argv": argv, "pid": 4000 + id,
        "state": state, "status": if state == "exited" { json!(1) } else { json!(null) },
        "runs": 1, "since": 61.0 * id as f64, "conns": [], "sprites": 0, "widgets": 0,
        "log": format!("/run/x.app-{id}.log"), "me": false,
        "frames": if id == 9 { json!([]) } else { json!([id + 1]) }, "moves": id == 9,
    }))
    .unwrap()
}

fn client(id: u64, argv: &[&str]) -> AppInfo {
    AppInfo::from_json(&json!({
        "id": id, "kind": "client", "name": "python3", "argv": argv, "pid": 777,
        "state": "running", "status": null, "since": 7200.0, "log": null, "me": false,
    }))
    .unwrap()
}

/// A tmux of three panes over a status line, 80x24.
pub(crate) fn frames() -> Vec<FrameInfo> {
    let frame = |id: u64, kind: &str, rect: [u64; 4], key: &str, idle: bool| {
        FrameInfo::from_json(&json!({
            "id": id, "kind": kind, "state": "stable", "usable": true, "rect": rect,
            "key": key, "support": null, "is_idle": idle,
        }))
    };
    [
        frame(0, "raw", [0, 0, 80, 24], "", false),
        frame(2, "pane", [0, 0, 40, 23], "tmux:main:0/vim", false),
        frame(3, "pane", [41, 0, 39, 11], "tmux:main:1", true),
        frame(
            4,
            "pane",
            [41, 12, 39, 11],
            "tmux:main:2/htop:processes",
            false,
        ),
        frame(5, "chrome", [0, 23, 80, 1], "", true),
    ]
    .into_iter()
    .flatten()
    .collect()
}

#[test]
#[ignore = "prints the screens of the frames"]
fn screens_of_frames() {
    let mut m = menu();
    let sizes = [(34, 9), (48, 13), (64, 18), (78, 22)];
    keys(&mut m, "f down down");
    for (w, h) in sizes {
        println!("frames {w}x{h}\n{}\n", shown(&m, w, h));
    }
    keys(&mut m, "esc m");
    for (w, h) in sizes {
        println!("move {w}x{h}\n{}\n", shown(&m, w, h));
    }
}

pub(crate) fn menu() -> Menu {
    let mut menu = Menu::new("tmux-main", 7);
    menu.catalog(Some(catalog::sample()));
    menu.apps(vec![
        info(1, "bricks -a", "running"),
        info(2, "fishes --decorate --bubbles --count 12", "paused"),
        info(3, "clock --tz Mars/Olympus", "exited"),
        client(4, &["/usr/bin/python3", "/home/u/draw.py"]),
        client(5, &["/usr/bin/ptyrc", "manage"]),
    ]);
    menu.frames(frames());
    menu
}

fn keys(menu: &mut Menu, keys: &str) -> Vec<Action> {
    let mut out = Vec::new();
    for key in keys.split_whitespace() {
        out = menu.key(key);
    }
    out
}

fn shown(menu: &Menu, w: usize, h: usize) -> String {
    let rows = menu.render(w, h);
    assert_eq!(rows.len(), h);
    for row in &rows {
        assert_eq!(row_len(row), w, "{:?}", text(std::slice::from_ref(row)));
    }
    text(&rows)
}

/// The row drawn as selected.
fn bar(menu: &Menu) -> String {
    let rows = menu.render(64, 18);
    let row = rows
        .iter()
        .find(|r| r.iter().any(|x| x.style.inverse && x.style.bold));
    row.map_or_else(String::new, |r| {
        text(std::slice::from_ref(r)).trim().to_string()
    })
}

#[test]
#[ignore = "prints the screens: cargo test -p ptyrc-cli screens -- --ignored --nocapture"]
fn screens() {
    let mut m = menu();
    let sizes = [(34, 9), (48, 13), (64, 18), (78, 22)];
    for (w, h) in sizes {
        println!("main {w}x{h}\n{}\n", shown(&m, w, h));
    }
    keys(&mut m, "enter");
    println!("actions\n{}\n", shown(&m, 48, 13));
    keys(&mut m, "esc s");
    for (w, h) in sizes {
        println!("start {w}x{h}\n{}\n", shown(&m, w, h));
    }
    keys(
        &mut m,
        "down down down down down down down down enter down down down down space down",
    );
    for (w, h) in sizes {
        println!("form {w}x{h}\n{}\n", shown(&m, w, h));
    }
    keys(
        &mut m,
        "esc esc down e down down down down down down down enter 4",
    );
    println!("restart\n{}\n", shown(&m, 64, 18));
}

/// What a form of the catalog asks: its options kept, then `action`.
fn kept(name: &str, args: &[&str], action: Action) -> Vec<Action> {
    let (name, args) = (name.into(), s(args));
    vec![Action::Remember { name, args }, action]
}

fn s(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| w.to_string()).collect()
}

#[test]
fn main_list() {
    let mut m = menu();
    let screen = shown(&m, 64, 18);
    assert!(screen.starts_with(" apps of tmux-main"), "{screen}");
    assert!(screen.contains("● bricks -a "), "{screen}");
    assert!(screen.contains("‖ fishes --decorate --bubbles --count 12         paused     2m"));
    assert!(screen.contains("○ clock --tz Mars/Olympus                        exit 1     3m"));
    assert!(
        screen.contains("● python3 /home/u/draw.py (pid 777)"),
        "{screen}"
    );
    // The manager is not an app of its own list.
    assert!(!screen.contains("manage"), "{screen}");
    assert!(screen.contains("+ start an app…") && screen.contains("? random app"));
    assert!(screen.contains("  esc leave "), "{screen}");
    // The selection is one bar; it moves and stops at the ends.
    assert!(bar(&m).starts_with("● bricks -a"), "{}", bar(&m));
    keys(&mut m, "up down");
    assert!(bar(&m).starts_with("‖ fishes"));
    keys(&mut m, "j j j j j j j");
    assert_eq!(bar(&m), "# frames…");
    keys(&mut m, "home k");
    assert!(bar(&m).starts_with("● bricks"));
    assert_eq!(keys(&mut m, "q"), [Action::Quit]);
    assert_eq!(keys(&mut m, "esc"), [Action::Quit]);
}

#[test]
fn every_size_holds_the_way_back() {
    let mut m = menu();
    for path in [
        "",
        "enter",
        "esc s",
        "enter",
        "down down enter",
        "esc esc esc down e",
    ] {
        keys(&mut m, path);
        for (w, h) in [
            (34, 9),
            (48, 13),
            (64, 18),
            (78, 22),
            (20, 5),
            (8, 3),
            (200, 60),
        ] {
            let screen = shown(&m, w, h);
            if w >= 34 {
                let foot = screen.lines().last().unwrap();
                assert!(
                    foot.contains("esc back") || foot.contains("esc leave"),
                    "{screen}"
                );
            }
        }
    }
}

#[test]
fn shortcuts() {
    let mut m = menu();
    assert_eq!(keys(&mut m, "p"), [Action::Pause(1)]);
    assert_eq!(keys(&mut m, "r"), [Action::Restart { id: 1, argv: None }]);
    assert_eq!(keys(&mut m, "x"), [Action::Stop(1)]);
    assert_eq!(keys(&mut m, "l"), [Action::Log(1)]);
    // Paused: the same key resumes. Exited: `x` forgets.
    assert_eq!(keys(&mut m, "down p"), [Action::Resume(2)]);
    assert_eq!(keys(&mut m, "down x"), [Action::Forget(3)]);
    assert_eq!(keys(&mut m, "delete"), [Action::Forget(3)]);
    assert_eq!(keys(&mut m, "r"), [Action::Restart { id: 3, argv: None }]);
    assert_eq!(keys(&mut m, "p"), []);
    assert!(shown(&m, 64, 18).contains("clock: it has ended"));
    // A client: stopped and paused, not restarted; no log.
    assert_eq!(keys(&mut m, "down p"), [Action::Pause(4)]);
    assert_eq!(keys(&mut m, "x"), [Action::Stop(4)]);
    assert_eq!(keys(&mut m, "r"), []);
    let screen = shown(&m, 78, 22);
    assert!(screen.contains("python3: started outside the driver: stop it, then start it here"));
    assert_eq!(keys(&mut m, "l"), []);
    // The note goes with the next key.
    keys(&mut m, "up");
    assert!(!shown(&m, 78, 22).contains("outside"));
    // On a row that is no app, the keys of an app do nothing.
    assert_eq!(keys(&mut m, "end p x r e l"), []);
}

#[test]
fn actions_of_a_row() {
    let mut m = menu();
    keys(&mut m, "enter");
    let screen = shown(&m, 48, 13);
    assert!(screen.starts_with(" bricks -a "), "{screen}");
    for line in [
        "pause",
        "restart ",
        "restart with other options…",
        "stop",
        "log",
    ] {
        assert!(screen.contains(&format!("\n {line}")), "{line}: {screen}");
    }
    assert!(screen.contains("running for 1m, pid 4001"), "{screen}");
    assert!(screen.contains("esc back"));
    assert_eq!(keys(&mut m, "down down down down enter"), [Action::Stop(1)]);
    // Done: back on the list.
    assert!(shown(&m, 48, 13).starts_with(" apps of"));
    keys(&mut m, "enter");
    assert_eq!(keys(&mut m, "esc"), []);
    assert!(shown(&m, 48, 13).starts_with(" apps of"));
    // The shortcuts work there too; `q` is back, not out.
    assert_eq!(keys(&mut m, "enter q"), []);
    assert_eq!(keys(&mut m, "enter p"), [Action::Pause(1)]);
    // An exited one starts again, or is forgotten.
    keys(&mut m, "down down enter");
    let screen = shown(&m, 48, 13);
    assert!(
        screen.contains("start again ") && screen.contains("forget"),
        "{screen}"
    );
    assert!(screen.contains("ended with status 1 for 3m"), "{screen}");
    assert_eq!(
        keys(&mut m, "enter"),
        [Action::Restart { id: 3, argv: None }]
    );
    // A client.
    keys(&mut m, "down enter");
    let screen = shown(&m, 64, 18);
    assert!(
        !screen.contains("restart") && !screen.contains("log"),
        "{screen}"
    );
    assert!(
        screen.contains("running for 2h, pid 777, started elsewhere"),
        "{screen}"
    );
    assert_eq!(keys(&mut m, "down enter"), [Action::Stop(4)]);
}

#[test]
fn start_with_defaults_is_enter_twice() {
    let mut m = menu();
    keys(&mut m, "s");
    let screen = shown(&m, 64, 18);
    assert!(screen.starts_with(" start an app"), "{screen}");
    assert!(
        screen.contains("\n confetti  adds confetti to your terminal"),
        "{screen}"
    );
    assert_eq!(
        keys(&mut m, "down enter enter"),
        kept(
            "clock",
            &["--front"],
            Action::Start {
                name: "clock".into(),
                args: s(&["--front"])
            }
        )
    );
    assert!(shown(&m, 64, 18).starts_with(" apps of"));
    // The same by the entry of the list.
    keys(&mut m, "end up up enter");
    assert!(shown(&m, 64, 18).starts_with(" start an app"));
    // A short list scrolls and says where it is.
    let screen = shown(&m, 34, 9);
    assert!(screen.contains("1/12") && screen.contains('↓'), "{screen}");
    keys(&mut m, "end");
    let screen = shown(&m, 34, 9);
    assert!(screen.contains("12/12") && screen.contains('↑') && screen.contains("monitor"));
}

#[test]
fn start_filters_as_typed() {
    let mut m = menu();
    keys(&mut m, "s s n o");
    let screen = shown(&m, 64, 18);
    assert!(screen.starts_with(" start an app: sno"), "{screen}");
    assert!(
        screen.contains("\n snow ") && !screen.contains("confetti"),
        "{screen}"
    );
    assert_eq!(
        keys(&mut m, "enter enter"),
        kept(
            "snow",
            &[],
            Action::Start {
                name: "snow".into(),
                args: vec![]
            }
        )
    );
    // j, k, q, h are letters there; nothing matches: a dim row, enter
    // does nothing.
    keys(&mut m, "s q j");
    assert!(shown(&m, 64, 18).contains("no app matches"));
    assert!(keys(&mut m, "enter").is_empty());
    // backspace edits; esc clears, then goes back.
    keys(&mut m, "backspace backspace");
    assert!(shown(&m, 64, 18).starts_with(" start an app  "));
    keys(&mut m, "c esc");
    assert!(shown(&m, 64, 18).contains("confetti"));
    keys(&mut m, "esc");
    assert!(shown(&m, 64, 18).starts_with(" apps of"));
}

#[test]
fn the_form() {
    let mut m = menu();
    keys(&mut m, "s end up up up enter"); // bricks
    let screen = shown(&m, 78, 22);
    assert!(screen.starts_with(" start bricks: options"), "{screen}");
    assert!(screen.contains("▶ start bricks"));
    assert!(
        screen.contains("    --speed           (16)         steps per second"),
        "{screen}"
    );
    assert!(screen.contains("[ ] -r, --rules "), "{screen}");
    // The frame comes last of the options, then the free line.
    let frame = screen.find("-f, --frame       (largest)").unwrap();
    assert!(
        screen.find("--grab").unwrap() < frame && frame < screen.find("more arguments").unwrap()
    );
    // A value is typed after enter; enter ends.
    keys(&mut m, "down enter");
    assert!(shown(&m, 78, 22).contains("type the value  enter done"));
    keys(&mut m, "2 4 left backspace 3 end . 5 enter");
    assert!(
        shown(&m, 78, 22).contains("--speed           34.5 "),
        "{}",
        shown(&m, 78, 22)
    );
    // Flags toggle; the hint is the help of the row.
    keys(&mut m, "down down space");
    let screen = shown(&m, 48, 13);
    assert!(screen.contains("[x] -r, --rules"), "{screen}");
    assert!(
        screen.contains("\n hard, loose, bumper and bomb bricks"),
        "{screen}"
    );
    keys(&mut m, "down enter down enter enter");
    let screen = shown(&m, 48, 13);
    assert!(screen.contains("[x] -b, --bonuses") && screen.contains("[ ] -m, --multiplayer"));
    // The free line, with a quoted word.
    keys(&mut m, "end enter - - s e e d space ' 4 space 2 '");
    // Typing does not leave: `q` is a letter there.
    keys(&mut m, "space q enter");
    let action = keys(&mut m, "home enter");
    // Never configured: `-a, --all` was set.
    let args = [
        "--speed",
        "34.5",
        "--rules",
        "--bonuses",
        "--all",
        "--seed",
        "4 2",
        "q",
    ];
    let start = Action::Start {
        name: "bricks".into(),
        args: s(&args),
    };
    assert_eq!(action, kept("bricks", &args, start));
}

#[test]
fn the_form_goes_back() {
    let mut m = menu();
    keys(&mut m, "s enter");
    assert!(shown(&m, 48, 13).contains("start confetti: options"));
    // esc while typing ends the typing only.
    keys(&mut m, "down enter 7 esc");
    assert!(shown(&m, 48, 13).contains("start confetti: options"));
    assert!(
        shown(&m, 48, 13).contains("--bursts       7"),
        "{}",
        shown(&m, 48, 13)
    );
    // backspace on a value clears it.
    keys(&mut m, "backspace");
    assert!(
        shown(&m, 48, 13).contains("--bursts       (1)"),
        "{}",
        shown(&m, 48, 13)
    );
    keys(&mut m, "esc");
    assert!(shown(&m, 48, 13).starts_with(" start an app"));
    keys(&mut m, "esc");
    assert!(shown(&m, 48, 13).starts_with(" apps of"));
}

#[test]
fn restart_with_other_options() {
    let mut m = menu();
    keys(&mut m, "down e");
    let screen = shown(&m, 64, 18);
    assert!(screen.starts_with(" restart fishes: options"), "{screen}");
    assert!(screen.contains("--count        12 "), "{screen}");
    assert!(screen.contains("[x] -d, --decorate") && screen.contains("[x] -b, --bubbles"));
    assert!(screen.contains("[ ] -r, --random"));
    // One more flag, one less.
    keys(&mut m, "down down down down space down down down space");
    let argv = s(&[
        "/usr/bin/ptyrc-app",
        "fishes",
        "--count",
        "12",
        "--bubbles",
        "--all",
    ]);
    let restart = Action::Restart {
        id: 2,
        argv: Some(argv),
    };
    assert_eq!(
        keys(&mut m, "home enter"),
        kept("fishes", &["--count", "12", "--bubbles", "--all"], restart)
    );
}

#[test]
fn a_form_is_filled_from_any_argv() {
    let mut m = Menu::new("x", 1);
    m.catalog(Some(catalog::sample()));
    let mut app = info(
        1,
        "bricks -rb --speed=20 -f 3 --what ever --seed",
        "running",
    );
    m.apps(vec![app.clone()]);
    keys(&mut m, "e");
    let screen = shown(&m, 78, 22);
    assert!(screen.contains("[x] -r, --rules") && screen.contains("[x] -b, --bonuses"));
    assert!(screen.contains("--speed           20 "), "{screen}");
    assert!(screen.contains("-f, --frame       3 "), "{screen}");
    // Unknown words, and a value flag without its value, are kept as typed.
    assert!(
        screen.contains("more arguments    --what ever --seed"),
        "{screen}"
    );
    let argv = s(&[
        "/usr/bin/ptyrc-app",
        "bricks",
        "--speed",
        "20",
        "--rules",
        "--bonuses",
        "--frame",
        "3",
        "--what",
        "ever",
        "--seed",
    ]);
    // A frame by id is not kept.
    let lasting = [
        "--speed",
        "20",
        "--rules",
        "--bonuses",
        "--what",
        "ever",
        "--seed",
    ];
    let restart = Action::Restart {
        id: 1,
        argv: Some(argv),
    };
    assert_eq!(keys(&mut m, "enter"), kept("bricks", &lasting, restart));
    // Positionals have their line.
    app = info(1, "marks -i ERROR 'two words' --no-hud", "running");
    m.apps(vec![app]);
    keys(&mut m, "e");
    let screen = shown(&m, 78, 22);
    assert!(
        screen.contains("PATTERN           ERROR 'two words'"),
        "{screen}"
    );
    assert!(screen.contains("[x] -i, --ignore-case") && screen.contains("[x] --no-hud"));
    let argv = s(&[
        "/usr/bin/ptyrc-app",
        "marks",
        "--ignore-case",
        "--no-hud",
        "ERROR",
        "two words",
    ]);
    let lasting = ["--ignore-case", "--no-hud", "ERROR", "two words"];
    let restart = Action::Restart {
        id: 1,
        argv: Some(argv),
    };
    assert_eq!(keys(&mut m, "enter"), kept("marks", &lasting, restart));
    // A program that is no app of the catalog: one free line.
    let other = AppInfo::from_json(&json!({
        "id": 2, "kind": "child", "name": "sh", "argv": ["/bin/sh", "-c", "sleep 9"],
        "pid": 9, "state": "running", "since": 1.0, "log": null,
    }))
    .unwrap();
    m.apps(vec![other]);
    keys(&mut m, "e");
    let screen = shown(&m, 64, 18);
    assert!(screen.contains("restart sh: options"), "{screen}");
    assert!(screen.contains("more arguments -c 'sleep 9'"), "{screen}");
    keys(&mut m, "down enter left backspace 1 enter");
    let argv = s(&["/bin/sh", "-c", "sleep 1"]);
    assert_eq!(
        keys(&mut m, "up enter"),
        [Action::Restart {
            id: 2,
            argv: Some(argv)
        }]
    );
}

#[test]
fn the_table_changes_under_the_menu() {
    let mut m = menu();
    keys(&mut m, "down");
    // The selection follows its app.
    m.app(info(1, "bricks -a", "gone"));
    assert!(bar(&m).starts_with("‖ fishes"), "{}", bar(&m));
    m.app(info(9, "snow", "running"));
    assert!(bar(&m).starts_with("‖ fishes"));
    m.app(info(2, "fishes", "running"));
    assert!(bar(&m).starts_with("● fishes   "), "{}", bar(&m));
    // Its screens close when it leaves.
    keys(&mut m, "enter");
    assert!(shown(&m, 48, 13).starts_with(" fishes"));
    m.app(info(2, "fishes", "gone"));
    assert!(shown(&m, 48, 13).starts_with(" apps of"));
    assert!(bar(&m).starts_with("○ clock"), "{}", bar(&m));
    // Nothing left, nothing to start.
    m.apps(Vec::new());
    m.catalog(None);
    let screen = shown(&m, 48, 13);
    assert!(
        screen.contains("# frames…") && !screen.contains("start an app"),
        "{screen}"
    );
    assert_eq!(keys(&mut m, "p x"), []);
    assert_eq!(keys(&mut m, "s"), []);
    assert!(
        shown(&m, 48, 13).contains("ptyrc-app was not found"),
        "{}",
        shown(&m, 48, 13)
    );
    assert_eq!(keys(&mut m, "q"), [Action::Quit]);
}

#[test]
fn random_app() {
    let mut m = menu();
    let mut last = String::new();
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..200 {
        let [Action::Start { name, args }] = &keys(&mut m, "end up enter")[..] else {
            panic!("no start");
        };
        assert_ne!(*name, last);
        assert_ne!(name, "marks");
        let app = catalog::sample().app(name).unwrap().clone();
        assert!(app.random.contains(args));
        last = name.clone();
        seen.insert((name.clone(), args.clone()));
    }
    assert!(
        seen.contains(&("bricks".into(), s(&["-a"]))) && seen.contains(&("bricks".into(), vec![]))
    );
    assert_eq!(
        seen.iter()
            .map(|x| &x.0)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        11
    );
    // An app configured is drawn as last given.
    m.favorites([("bricks".to_string(), s(&["-r"]))].into());
    let mut bricks = std::collections::BTreeSet::new();
    for _ in 0..200 {
        if let [Action::Start { name, args }] = &keys(&mut m, "end up enter")[..]
            && name == "bricks"
        {
            bricks.insert(args.clone());
        }
    }
    assert_eq!(bricks.into_iter().collect::<Vec<_>>(), [s(&["-r"])]);
}

#[test]
fn the_log() {
    let mut m = menu();
    assert_eq!(keys(&mut m, "down down l"), [Action::Log(3)]);
    m.log(3, (1..=40).map(|i| format!("line {i}")).collect());
    let screen = shown(&m, 48, 13);
    // Its end first.
    assert!(screen.starts_with(" log of clock"), "{screen}");
    assert!(
        screen.contains("line 40") && !screen.contains("line 30 "),
        "{screen}"
    );
    assert!(screen.contains("↑↓ scroll  esc back"));
    keys(&mut m, "home");
    assert!(shown(&m, 48, 13).contains("\n line 1 "));
    keys(&mut m, "esc");
    assert!(shown(&m, 48, 13).starts_with(" apps of"));
    m.log(3, Vec::new());
    assert!(shown(&m, 48, 13).contains("nothing was written"));
}

#[test]
fn sessions() {
    let mut m = Menu::sessions(s(&["tmux-main", "4242"]));
    let screen = shown(&m, 48, 13);
    assert!(
        screen.starts_with(" which session?") && screen.contains("\n 4242"),
        "{screen}"
    );
    assert_eq!(keys(&mut m, "down enter"), [Action::Session("4242".into())]);
    assert_eq!(keys(&mut m, "esc"), [Action::Quit]);
}

#[test]
fn notes() {
    let mut m = menu();
    m.note("clock: unknown time zone 'Mars/Olympus'\nmore", true);
    let rows = m.render(48, 13);
    let note = &rows[11];
    assert_eq!(
        text(std::slice::from_ref(note)).trim(),
        "clock: unknown time zone 'Mars/Olympus'"
    );
    assert!(note.iter().any(|r| r.style.fg == Some("red")));
    // In the smallest size it takes a row of the list, for a key.
    let screen = shown(&m, 34, 9);
    assert!(
        screen.contains("clock: unknown time zone 'Mars/…"),
        "{screen}"
    );
}

#[test]
fn typed_lines() {
    assert_eq!(
        words(r#" a  'b c' "d e"f g\ h '' "#),
        s(&["a", "b c", "d ef", "g h", ""])
    );
    let back = s(&["a", "b c", "", "it's"]);
    assert_eq!(words(&line_of(&back)), back);
    let mut line = Line::new("abc");
    for key in [
        "left", "left", "X", "home", "delete", "end", "é", "ctrl+a", "space",
    ] {
        assert!(line.key(key), "{key}");
    }
    assert_eq!(line.string(), " Xbcé");
    assert!(!line.key("f5") && !line.key("up"));
    line.key("ctrl+k");
    assert_eq!(line.string(), " ");
    // The cursor stays in the window.
    let long = Line::new("0123456789");
    assert_eq!(text(&[long.runs(5, true, PLAIN)]), "6789 ");
    assert_eq!(text(&[long.runs(5, false, PLAIN)]), "0123…");
}

#[test]
fn infos() {
    let app = info(1, "bricks -a", "running");
    assert_eq!(
        (app.label().as_str(), app.args()),
        ("bricks", &s(&["-a"])[..])
    );
    // Under an interpreter, as a client is seen.
    let app = client(
        2,
        &[
            "/usr/bin/python3",
            "/home/u/.local/bin/ptyrc-app",
            "snow",
            "--cover",
        ],
    );
    assert_eq!(
        (app.label().as_str(), app.args()),
        ("snow", &s(&["--cover"])[..])
    );
    let app = client(3, &[]);
    assert_eq!(app.label(), "python3");
    assert!(client(4, &["/x/ptyrc", "manage", "--overlay"]).is_manager());
    assert!(!client(4, &["/x/ptyrc", "attach"]).is_manager());
    assert!(AppInfo::from_json(&json!({"id": 1, "state": "new"})).is_none());
}

#[test]
fn frames_and_their_apps() {
    let mut m = menu();
    // The first frame is shown at once.
    assert_eq!(keys(&mut m, "f"), [Action::Highlight(Some(0))]);
    let screen = shown(&m, 64, 18);
    assert!(screen.starts_with(" frames"), "{screen}");
    assert!(screen.contains(" 2 pane 40x23   busy bricks"), "{screen}");
    assert!(screen.contains(" 3 pane 39x11   idle fishes"), "{screen}");
    // Chrome is no frame to draw on.
    assert!(!screen.contains("80x1"), "{screen}");
    assert_eq!(keys(&mut m, "down"), [Action::Highlight(Some(2))]);
    let screen = shown(&m, 64, 18);
    // The map: the pane selected is filled, the others are not.
    assert!(screen.contains("│█████2█████             │"), "{screen}");
    assert!(screen.contains("\n tmux:main:0/vim "), "{screen}");
    // The smallest size has the list alone.
    let screen = shown(&m, 34, 9);
    assert!(
        !screen.contains('█') && screen.contains("esc back"),
        "{screen}"
    );
    assert!(shown(&m, 48, 13).contains("│██2███ ░░░░░░░│"));
    // The whole screen: the border too.
    keys(&mut m, "up");
    let rows = m.render(64, 18);
    let top = rows
        .iter()
        .flatten()
        .find(|r| r.text.starts_with('┌'))
        .unwrap();
    assert_eq!(top.style.fg, Some("yellow"));
    // Its apps, then their actions.
    assert_eq!(keys(&mut m, "down down enter"), []);
    let screen = shown(&m, 64, 18);
    assert!(screen.starts_with(" apps on frame 3"), "{screen}");
    assert!(screen.contains("‖ fishes --decorate"), "{screen}");
    keys(&mut m, "enter");
    assert!(shown(&m, 64, 18).contains("resume"));
    // Done: the main list, the frame no longer shown.
    assert_eq!(keys(&mut m, "enter"), [Action::Resume(2)]);
    assert_eq!(keys(&mut m, "end enter enter"), []);
    assert!(shown(&m, 64, 18).contains("no app draws on this frame"));
    assert_eq!(keys(&mut m, "enter esc"), []);
    assert_eq!(keys(&mut m, "esc"), [Action::Highlight(None)]);
    // The frames change under it.
    keys(&mut m, "f end");
    let mut fewer = frames();
    fewer.truncate(2);
    assert_eq!(m.frames(fewer), Some(Action::Highlight(Some(2))));
    assert!(!shown(&m, 64, 18).contains("39x11"));
}

#[test]
fn move_to_a_frame() {
    let mut m = menu();
    // The picker opens on the frame the app is on.
    assert_eq!(keys(&mut m, "m"), [Action::Highlight(Some(2))]);
    let screen = shown(&m, 64, 18);
    assert!(screen.starts_with(" move bricks to"), "{screen}");
    assert!(screen.contains("restarts there: state lost"), "{screen}");
    assert!(screen.contains("largest  the largest pane"), "{screen}");
    let argv = s(&["/usr/bin/ptyrc-app", "bricks", "-a", "-f", "4"]);
    assert_eq!(
        keys(&mut m, "down down enter"),
        [
            Action::Restart {
                id: 1,
                argv: Some(argv)
            },
            Action::Highlight(None)
        ]
    );
    // A name; by the actions of the row.
    keys(&mut m, "enter down down down enter");
    assert!(shown(&m, 64, 18).starts_with(" move bricks to"));
    let argv = s(&["/usr/bin/ptyrc-app", "bricks", "-a", "-f", "idle"]);
    assert_eq!(
        keys(&mut m, "end enter"),
        [Action::Restart {
            id: 1,
            argv: Some(argv)
        }]
    );
    // An app that moves as it runs keeps its state.
    m.app(info(9, "snow -f 2", "running"));
    keys(&mut m, "down down down m");
    let screen = shown(&m, 64, 18);
    assert!(screen.starts_with(" move snow to"), "{screen}");
    assert!(screen.contains("moves live: state kept"), "{screen}");
    assert_eq!(
        keys(&mut m, "home down down enter"),
        [
            Action::Move {
                id: 9,
                frame: "3".into()
            },
            Action::Highlight(None)
        ]
    );
    // Paused, it does not listen: restarted.
    m.app(info(9, "snow -f 2 --cover", "paused"));
    keys(&mut m, "m");
    assert!(shown(&m, 64, 18).contains("restarts there: state lost"));
    let argv = s(&["/usr/bin/ptyrc-app", "snow", "-f", "0", "--cover"]);
    assert_eq!(
        keys(&mut m, "home enter"),
        [
            Action::Restart {
                id: 9,
                argv: Some(argv)
            },
            Action::Highlight(None)
        ]
    );
    // A client that does not move stays where it is.
    assert_eq!(keys(&mut m, "down m"), []);
    assert!(shown(&m, 78, 22).contains("python3: it does not move, and was not started here"));
}

#[test]
fn the_frame_of_a_form_is_picked() {
    let mut m = menu();
    keys(&mut m, "s end up up up enter end up");
    assert_eq!(keys(&mut m, "enter"), [Action::Highlight(Some(0))]);
    let screen = shown(&m, 64, 18);
    assert!(screen.starts_with(" which frame?") && screen.contains("the frame it starts on"));
    assert_eq!(keys(&mut m, "down down enter"), [Action::Highlight(None)]);
    assert!(shown(&m, 64, 18).contains("-f, --frame       3 "));
    // Opened again, it is on the frame chosen; a name will do.
    assert_eq!(keys(&mut m, "enter"), [Action::Highlight(Some(3))]);
    keys(&mut m, "end up enter");
    let args = ["--all", "--frame", "quietest"];
    let start = Action::Start {
        name: "bricks".into(),
        args: s(&args),
    };
    assert_eq!(keys(&mut m, "home enter"), kept("bricks", &args, start));
}

#[test]
fn a_form_opens_as_last_given() {
    let mut m = menu();
    // Never configured: `-a, --all` set where there is one.
    keys(&mut m, "s b r i enter");
    let screen = shown(&m, 78, 22);
    assert!(screen.contains("[x] -a, --all"), "{screen}");
    assert!(screen.contains("[ ] -r, --rules"), "{screen}");
    // `--front`, for an app made of widgets only.
    keys(&mut m, "esc esc esc s m o n i enter");
    assert!(shown(&m, 78, 22).contains("[x] --front"));
    keys(&mut m, "esc esc esc s s n a k enter");
    let screen = shown(&m, 78, 22);
    assert!(screen.contains("[ ] --front") && screen.contains("[x] -a, --all"));
    keys(&mut m, "esc esc esc s f l o w enter");
    let screen = shown(&m, 78, 22);
    assert!(screen.contains("[ ] -a, --anywhere"), "{screen}");
    // Submitted: the next form of the app is as that one.
    keys(&mut m, "esc esc esc s b r i enter");
    keys(&mut m, "down enter 9 enter end up up up space");
    let args = ["--speed", "9"];
    let start = Action::Start {
        name: "bricks".into(),
        args: s(&args),
    };
    assert_eq!(keys(&mut m, "home enter"), kept("bricks", &args, start));
    keys(&mut m, "s b r i enter");
    let screen = shown(&m, 78, 22);
    assert!(screen.contains("[ ] -a, --all"), "{screen}");
    assert!(screen.contains("--speed           9 "), "{screen}");
    // A restart with other options is remembered too.
    keys(&mut m, "esc esc esc home down e end up up space");
    assert!(matches!(
        &keys(&mut m, "home enter")[..],
        [Action::Remember { name, .. }, Action::Restart { .. }] if name == "fishes"
    ));
    keys(&mut m, "s f i s h enter");
    let screen = shown(&m, 78, 22);
    assert!(screen.contains("[x] -d, --decorate") && screen.contains("--count        12 "));
    // What the front read of the config directory.
    m.favorites([("snake".to_string(), s(&["-x", "--speed=3"]))].into());
    keys(&mut m, "esc esc esc s s n a enter");
    let screen = shown(&m, 78, 22);
    assert!(screen.contains("[x] -x, --enemies") && screen.contains("[ ] -a, --all"));
    assert!(screen.contains("--speed        3 "), "{screen}");
    // A program that is no app of the catalog: nothing kept.
    let mut m = Menu::new("x", 1);
    m.apps(vec![info(1, "bricks -a", "running")]);
    assert!(matches!(
        &keys(&mut m, "e enter")[..],
        [Action::Restart { .. }]
    ));
}

#[test]
fn names_are_glyphs_only() {
    let mut m = Menu::new("s\x1b]0;t\x07", 7);
    let mut argv = vec!["/usr/bin/\x1b]52;c;eA==\x07x\u{202e}".to_string()];
    argv.push("-a\u{9b}2J\r\n".into());
    m.apps(vec![
        AppInfo::from_json(&json!({
            "id": 1, "kind": "child", "name": "x\x1b]8;;u\x1b\\", "argv": argv, "pid": 1,
            "state": "running", "status": null, "runs": 1, "since": 1.0, "conns": [],
            "sprites": 0, "widgets": 0,
        }))
        .unwrap(),
    ]);
    for (w, h) in [(48, 13), (78, 22)] {
        let text = shown(&m, w, h);
        assert!(
            !text
                .chars()
                .any(|c| (c.is_control() && c != '\n') || ptyrc_vt::is_hidden(c)),
            "{text:?}"
        );
    }
}
