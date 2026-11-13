// SPDX-License-Identifier: GPL-3.0-or-later
//! The menu in the session itself: a modal widget, the keys of the host
//! keyboard grabbed.

use super::menu::{Menu, Row};
use super::ui::{Backend, Flow, Runner, Session};
use crate::Fail;
use ptyrc_proto::{Client, ClientError};
use serde_json::{Value, json};
use std::process::ExitCode;
use std::time::{Duration, Instant};

/// The tag, and the title on the border.
const TAG: &str = "manage";
/// Inner sizes of the variants.
pub const SIZES: [(usize, usize); 4] = [(34, 9), (48, 13), (64, 18), (78, 22)];
/// The times shown are counted again this often.
const BEAT: Duration = Duration::from_secs(1);

/// Rows as the `cells` of a widget content.
pub fn cells(rows: &[Row]) -> Value {
    let piece = |r: &super::menu::Run| {
        let mut cell = json!({"t": r.text});
        let style = r.style;
        if let Some(fg) = style.fg {
            cell["fg"] = json!(fg);
        }
        for (on, name) in [
            (style.bold, "bold"),
            (style.dim, "dim"),
            (style.inverse, "inverse"),
        ] {
            if on {
                cell[name] = json!(true);
            }
        }
        cell
    };
    let row = |row: &Row| Value::Array(row.iter().map(piece).collect());
    json!({"cells": rows.iter().map(row).collect::<Vec<_>>()})
}

/// The widget of the menu, as the driver last told it.
struct Shown {
    id: u64,
    /// Inner size of the variant shown; `None`: hidden.
    size: Option<(usize, usize)>,
}

impl Shown {
    /// Take the info of a widget; false when it is another's.
    fn update(&mut self, info: &Value) -> bool {
        if info["id"].as_u64() != Some(self.id) {
            return false;
        }
        let cell = |i: usize| info["size"][i].as_u64().map(|v| v as usize);
        self.size = cell(0).zip(cell(1));
        true
    }
}

pub fn run(client: Client) -> Result<ExitCode, Fail> {
    let (mut session, catalog) = Session::new(client);
    session.lights = true;
    let name = session.name();
    let client = &mut session.client;
    let variants: Vec<Value> = (SIZES.iter())
        .map(|(w, h)| json!({"tag": TAG, "name": format!("{w}x{h}"), "size": [w, h]}))
        .collect();
    client.call("widget.declare", json!({"widgets": variants}))?;
    let show = json!({
        "tag": TAG, "modal": true, "border": "alert", "prefer": "center",
        "fill": 1.0, "title": "manage",
    });
    let info = client.call("widget.show", show)?;
    let mut shown = Shown {
        id: info["widget"]["id"].as_u64().ok_or("widget.show: no id")?,
        size: None,
    };
    shown.update(&info["widget"]);
    // Every key is the menu's; ctrl+t always leaves.
    client.call("input.grab", json!({}))?;
    client.subscribe(&["apps", "frames", "widgets", "exit"])?;

    let seed = std::process::id() as u64 ^ 0x9e37_79b9_7f4a_7c15;
    let mut menu = Menu::new(&name, seed);
    menu.catalog(catalog);
    let mut runner = Runner::new(menu, session);
    let mut beat = Instant::now();
    let mut drawn: Option<Value> = None;
    let result = (|| -> Result<(), Fail> {
        loop {
            if let Some((w, h)) = shown.size {
                let content = cells(&runner.menu.render(w, h));
                if drawn.as_ref() != Some(&content) {
                    let set = json!({"id": shown.id, "content": content});
                    runner.backend.client.call("widget.set", set)?;
                    drawn = Some(content);
                }
            }
            let now = Instant::now();
            let until = runner
                .deadline()
                .map_or(beat + BEAT, |d| d.min(beat + BEAT));
            let wait = until
                .saturating_duration_since(now)
                .max(Duration::from_millis(1));
            match runner.backend.client.next_event(Some(wait)) {
                Ok(Some(event)) => match event.event.as_str() {
                    "exit" => return Ok(()),
                    "input" if event.data["released"].is_string() => return Ok(()),
                    "input" => {
                        if let Some(key) = event.data["key"].as_str()
                            && runner.key(key, Instant::now()) != Flow::Stay
                        {
                            return Ok(());
                        }
                    }
                    "widgets" => {
                        if shown.update(&event.data) {
                            // Another variant: its content is not set yet.
                            drawn = None;
                            if event.data["state"].as_str() == Some("gone") {
                                return Ok(());
                            }
                        }
                    }
                    topic => runner.event(topic, &event.data),
                },
                Ok(None) => {}
                Err(ClientError::Closed) => return Ok(()),
                Err(e) => return Err(e.into()),
            }
            let now = Instant::now();
            runner.tick(now);
            if now >= beat + BEAT {
                runner.beat();
                beat = now;
            }
        }
    })();
    // The connection takes all with it; said anyway, in order.
    runner.backend.highlight(None);
    let client = &mut runner.backend.client;
    let _ = client.call("input.release", json!({}));
    let _ = client.call("widget.hide", json!({"id": shown.id}));
    result.map(|()| ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manage::menu::tests::menu;

    #[test]
    fn rows_as_cells() {
        let m = menu();
        for (w, h) in SIZES {
            let content = cells(&m.render(w, h));
            let rows = content["cells"].as_array().unwrap();
            assert_eq!(rows.len(), h);
            for row in rows {
                let runs = row.as_array().unwrap();
                let width: usize = (runs.iter())
                    .map(|r| r["t"].as_str().unwrap().chars().count())
                    .sum();
                assert_eq!(width, w);
            }
        }
        let content = cells(&m.render(48, 13));
        let bar = &content["cells"][2][1];
        assert_eq!(bar["inverse"], json!(true));
        assert_eq!(bar["bold"], json!(true));
        let text = content.to_string();
        assert!(text.contains(r#""fg":"yellow""#) && text.contains(r#""dim":true"#));
    }

    #[test]
    fn the_widget_shown() {
        let mut shown = Shown { id: 7, size: None };
        assert!(!shown.update(&json!({"id": 8, "size": [34, 9]})));
        assert!(shown.update(&json!({"id": 7, "state": "placed", "size": [48, 13]})));
        assert_eq!(shown.size, Some((48, 13)));
        assert!(shown.update(&json!({"id": 7, "state": "hidden", "size": null})));
        assert_eq!(shown.size, None);
    }
}
