// SPDX-License-Identifier: GPL-3.0-or-later
//! Shapes: line, rect, circle, ellipse.
//!
//! Curves are drawn in braille: a cell is 2 x 4 dots, and dots are square
//! on a usual font. Sizes are in cells; a circle of radius `r` rows is
//! `4r` columns wide.

use ptyrc_core::plugin::{DrawExt, FnDoc};
use ptyrc_core::scene::SpriteSpec;
use serde_json::{Value, json};

pub struct Shapes;

/// Dot canvas of `w x h` cells.
struct Dots {
    w: usize,
    h: usize,
    bits: Vec<u8>,
}

impl Dots {
    fn new(w: usize, h: usize) -> Dots {
        Dots {
            w,
            h,
            bits: vec![0; w * h],
        }
    }

    fn set(&mut self, x: i64, y: i64) {
        if x < 0 || y < 0 || x >= (self.w * 2) as i64 || y >= (self.h * 4) as i64 {
            return;
        }
        let (x, y) = (x as usize, y as usize);
        // Braille dot numbering: columns 1-2-3-7 and 4-5-6-8.
        const BIT: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];
        self.bits[(y / 4) * self.w + x / 2] |= BIT[y % 4][x % 2];
    }

    fn line(&mut self, x0: i64, y0: i64, x1: i64, y1: i64) {
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
        let (mut x, mut y, mut err) = (x0, y0, dx + dy);
        loop {
            self.set(x, y);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Ellipse inscribed in the canvas.
    fn ellipse(&mut self, fill: bool) {
        let (dw, dh) = ((self.w * 2) as f64, (self.h * 4) as f64);
        let (cx, cy, rx, ry) = ((dw - 1.0) / 2.0, (dh - 1.0) / 2.0, dw / 2.0, dh / 2.0);
        for y in 0..dh as i64 {
            for x in 0..dw as i64 {
                let nx = (x as f64 - cx) / rx;
                let ny = (y as f64 - cy) / ry;
                let d = nx * nx + ny * ny;
                // The outline is about one dot thick.
                let edge = 1.0 - 1.6 / rx.min(ry);
                if d <= 1.0 && (fill || d >= edge * edge) {
                    self.set(x, y);
                }
            }
        }
    }

    fn rows(&self) -> Vec<String> {
        self.bits
            .chunks(self.w)
            .map(|row| {
                row.iter()
                    .map(|&b| {
                        if b == 0 {
                            ' '
                        } else {
                            char::from_u32(0x2800 + b as u32).unwrap()
                        }
                    })
                    .collect()
            })
            .collect()
    }
}

/// An integer parameter; `default` when absent. Another type is refused.
fn num(params: &Value, key: &str, default: i64) -> Result<i64, String> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v.as_i64().ok_or(format!("{key}: an integer, not {v}")),
    }
}

fn sprite(rows: Vec<String>, params: &Value) -> Result<SpriteSpec, String> {
    let mut style = serde_json::Map::new();
    for key in ["fg", "bg", "bold", "dim"] {
        if let Some(v) = params.get(key) {
            style.insert(key.to_string(), v.clone());
        }
    }
    serde_json::from_value(json!({"content": {"rows": rows, "style": style}}))
        .map_err(|e| e.to_string())
}

fn size(params: &Value, key: &str, default: i64) -> Result<usize, String> {
    let v = num(params, key, default)?;
    if (1..=500).contains(&v) {
        Ok(v as usize)
    } else {
        Err(format!("{key}: 1 to 500"))
    }
}

impl DrawExt for Shapes {
    fn name(&self) -> &'static str {
        "shapes"
    }

    fn functions(&self) -> Vec<FnDoc> {
        vec![
            FnDoc {
                name: "line",
                params: "x1, y1, x2, y2, fg",
                doc: "Segment between two cells, relative to the sprite.",
            },
            FnDoc {
                name: "rect",
                params: "w, h, border (light|heavy|double|round), fill, fg, bg",
                doc: "Box of w x h cells.",
            },
            FnDoc {
                name: "circle",
                params: "r, fill, fg",
                doc: "Circle of radius r rows.",
            },
            FnDoc {
                name: "ellipse",
                params: "w, h, fill, fg",
                doc: "Ellipse in w x h cells.",
            },
        ]
    }

    fn draw(&self, function: &str, params: &Value) -> Result<SpriteSpec, String> {
        let fill = match params.get("fill") {
            None | Some(Value::Null) => false,
            Some(v) => v.as_bool().ok_or(format!("fill: true or false, not {v}"))?,
        };
        match function {
            "line" => {
                let (x1, y1) = (num(params, "x1", 0)?, num(params, "y1", 0)?);
                let (x2, y2) = (num(params, "x2", 0)?, num(params, "y2", 0)?);
                if [x1, y1, x2, y2].iter().any(|v| !(0..500).contains(v)) {
                    return Err("coordinates: 0 to 499".into());
                }
                let mut dots = Dots::new(x1.max(x2) as usize + 1, y1.max(y2) as usize + 1);
                dots.line(x1 * 2, y1 * 4 + 1, x2 * 2 + 1, y2 * 4 + 2);
                sprite(dots.rows(), params)
            }
            "rect" => {
                let (w, h) = (size(params, "w", 4)?, size(params, "h", 3)?);
                let border = match params.get("border") {
                    None | Some(Value::Null) => "light",
                    Some(v) => v.as_str().ok_or(format!("border: a name, not {v}"))?,
                };
                let g: [char; 6] = match border {
                    "light" => ['┌', '┐', '└', '┘', '─', '│'],
                    "heavy" => ['┏', '┓', '┗', '┛', '━', '┃'],
                    "double" => ['╔', '╗', '╚', '╝', '═', '║'],
                    "round" => ['╭', '╮', '╰', '╯', '─', '│'],
                    other => return Err(format!("border: {other}")),
                };
                if w < 2 || h < 2 {
                    return Err("w, h: at least 2".into());
                }
                let inner = if fill { "█" } else { " " }.repeat(w - 2);
                let mut rows = vec![format!(
                    "{}{}{}",
                    g[0],
                    g[4].to_string().repeat(w - 2),
                    g[1]
                )];
                rows.extend((0..h - 2).map(|_| format!("{}{}{}", g[5], inner, g[5])));
                rows.push(format!(
                    "{}{}{}",
                    g[2],
                    g[4].to_string().repeat(w - 2),
                    g[3]
                ));
                sprite(rows, params)
            }
            "circle" => {
                let r = size(params, "r", 3)?;
                let mut dots = Dots::new(r * 4, r * 2);
                dots.ellipse(fill);
                sprite(dots.rows(), params)
            }
            "ellipse" => {
                let mut dots = Dots::new(size(params, "w", 8)?, size(params, "h", 3)?);
                dots.ellipse(fill);
                sprite(dots.rows(), params)
            }
            other => Err(format!("no function {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(spec: SpriteSpec) -> Vec<String> {
        spec.content.unwrap().rows.unwrap()
    }

    #[test]
    fn rect() {
        let r = rows(
            Shapes
                .draw("rect", &json!({"w": 4, "h": 3, "border": "round"}))
                .unwrap(),
        );
        assert_eq!(r, vec!["╭──╮", "│  │", "╰──╯"]);
        assert!(Shapes.draw("rect", &json!({"w": 1})).is_err());
    }

    #[test]
    fn circle_is_symmetric() {
        let r = rows(Shapes.draw("circle", &json!({"r": 3})).unwrap());
        assert_eq!(r.len(), 6);
        assert!(r.iter().all(|l| l.chars().count() == 12));
        // Hollow: the centre is empty; filled: it is not.
        assert_eq!(r[3].chars().nth(6), Some(' '));
        let f = rows(
            Shapes
                .draw("circle", &json!({"r": 3, "fill": true}))
                .unwrap(),
        );
        assert_eq!(f[3].chars().nth(6), Some('⣿'));
        let dots = |l: &str| l.chars().filter(|c| *c != ' ').count();
        assert_eq!(dots(&r[0]), dots(&r[5]));
    }

    #[test]
    fn line_and_unknown() {
        let r = rows(Shapes.draw("line", &json!({"x2": 5, "y2": 2})).unwrap());
        assert_eq!(r.len(), 3);
        assert_ne!(r[0].chars().next(), Some(' '));
        assert!(Shapes.draw("spiral", &json!({})).is_err());
    }

    #[test]
    fn a_parameter_of_the_wrong_type_is_refused() {
        let err = |f: &str, p: Value| Shapes.draw(f, &p).unwrap_err();
        assert_eq!(err("circle", json!({"r": "x"})), "r: an integer, not \"x\"");
        assert_eq!(err("circle", json!({"r": 2.5})), "r: an integer, not 2.5");
        assert_eq!(err("line", json!({"x2": [1]})), "x2: an integer, not [1]");
        assert_eq!(
            err("rect", json!({"fill": "yes"})),
            "fill: true or false, not \"yes\""
        );
        assert_eq!(err("rect", json!({"border": 3})), "border: a name, not 3");
        assert!(Shapes.draw("circle", &json!({"r": null})).is_ok());
    }
}
