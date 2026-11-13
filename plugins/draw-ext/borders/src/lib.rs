// SPDX-License-Identifier: GPL-3.0-or-later
//! Borders of widgets: frames as of pictures.
//!
//! - `painting`: a gilt double frame, ticks along its edges.
//! - `baroque`: a bronze double frame, crossed corners, studded edges.
//! - `mat`: a pale mount of half blocks.
//! - `sketch`: dashed, rounded, dim: a frame drawn in pencil.

use ptyrc_core::plugin::{DrawExt, FnDoc};
use ptyrc_core::scene::{ColorSpec, SpriteSpec, StyleSpec};
use ptyrc_core::widgets::{BorderSpec, TitleSpec};
use ptyrc_vt::Color;
use serde_json::Value;

pub struct Borders;

fn colour(index: u8) -> Option<ColorSpec> {
    Some(ColorSpec(Color::Indexed(index)))
}

/// A frame: corners, edges top, bottom, left, right; colour; title style.
fn frame(corners: &str, edges: [&str; 4], style: StyleSpec, title: StyleSpec) -> BorderSpec {
    BorderSpec {
        corners: corners.into(),
        top: edges[0].into(),
        bottom: edges[1].into(),
        left: edges[2].into(),
        right: edges[3].into(),
        style,
        title: TitleSpec {
            style: title,
            ..TitleSpec::default()
        },
        chase: None,
    }
}

/// The presets of the extension.
pub fn presets() -> Vec<(&'static str, BorderSpec)> {
    let gold = StyleSpec {
        fg: colour(178),
        ..StyleSpec::default()
    };
    let bronze = StyleSpec {
        fg: colour(137),
        ..StyleSpec::default()
    };
    let pale = StyleSpec {
        fg: colour(250),
        ..StyleSpec::default()
    };
    let pencil = StyleSpec {
        dim: true,
        ..StyleSpec::default()
    };
    let italic = StyleSpec {
        italic: true,
        ..StyleSpec::default()
    };
    vec![
        (
            "painting",
            frame("╔╗╚╝", ["═╤", "═╧", "║╟", "║╢"], gold, italic),
        ),
        (
            "baroque",
            frame("╬╬╬╬", ["═╦", "═╩", "║╠", "║╣"], bronze, italic),
        ),
        (
            "mat",
            frame(
                "▛▜▙▟",
                ["▀", "▄", "▌", "▐"],
                pale,
                StyleSpec {
                    fg: colour(236),
                    bg: colour(250),
                    ..StyleSpec::default()
                },
            ),
        ),
        (
            "sketch",
            frame("╭╮╰╯", ["┄", "┄", "┆", "┆"], pencil, italic),
        ),
    ]
}

impl DrawExt for Borders {
    fn name(&self) -> &'static str {
        "borders"
    }

    fn functions(&self) -> Vec<FnDoc> {
        Vec::new()
    }

    fn draw(&self, function: &str, _params: &Value) -> Result<SpriteSpec, String> {
        Err(format!("no function {function}"))
    }

    fn borders(&self) -> Vec<(&'static str, BorderSpec)> {
        presets()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ptyrc_core::widgets::compose;

    #[test]
    fn glyphs_of_one_cell() {
        for (name, spec) in presets() {
            spec.check().unwrap_or_else(|e| panic!("{name}: {e}"));
            let all = [
                &spec.corners,
                &spec.top,
                &spec.bottom,
                &spec.left,
                &spec.right,
            ];
            for c in all.iter().flat_map(|s| s.chars()) {
                assert_eq!(ptyrc_vt::char_width(c), 1, "{name}: {c}");
            }
        }
    }

    #[test]
    fn painting() {
        let spec = &presets()[0].1;
        let content = compose(spec, "sun", None, 7, 2);
        let rows: Vec<String> = content
            .cells
            .unwrap()
            .iter()
            .map(|runs| runs.iter().map(|r| r.t.as_str()).collect())
            .collect();
        assert_eq!(rows, ["╔═╤═╤═╤═╗", "║       ║", "╟       ╢", "╚═╧ sun ╝"]);
    }
}
