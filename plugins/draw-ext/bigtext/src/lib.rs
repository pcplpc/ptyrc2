// SPDX-License-Identifier: GPL-3.0-or-later
//! Big letters from a 3 x 5 pixel font.
//!
//! `scale` 1 packs two pixel rows per cell with half blocks (a glyph is
//! 3 x 3 cells); `scale` n >= 2 uses full blocks, `2(n-1) x (n-1)` cells per
//! pixel.

use ptyrc_core::plugin::{DrawExt, FnDoc};
use ptyrc_core::scene::SpriteSpec;
use serde_json::{Value, json};

pub struct BigText;

/// Five rows of three pixels, `#` set.
fn glyph(c: char) -> [&'static str; 5] {
    match c.to_ascii_uppercase() {
        'A' => [" # ", "# #", "###", "# #", "# #"],
        'B' => ["## ", "# #", "## ", "# #", "## "],
        'C' => [" ##", "#  ", "#  ", "#  ", " ##"],
        'D' => ["## ", "# #", "# #", "# #", "## "],
        'E' => ["###", "#  ", "## ", "#  ", "###"],
        'F' => ["###", "#  ", "## ", "#  ", "#  "],
        'G' => [" ##", "#  ", "# #", "# #", " ##"],
        'H' => ["# #", "# #", "###", "# #", "# #"],
        'I' => ["###", " # ", " # ", " # ", "###"],
        'J' => ["  #", "  #", "  #", "# #", " # "],
        'K' => ["# #", "# #", "## ", "# #", "# #"],
        'L' => ["#  ", "#  ", "#  ", "#  ", "###"],
        'M' => ["# #", "###", "###", "# #", "# #"],
        'N' => ["## ", "# #", "# #", "# #", "# #"],
        'O' => [" # ", "# #", "# #", "# #", " # "],
        'P' => ["## ", "# #", "## ", "#  ", "#  "],
        'Q' => [" # ", "# #", "# #", " # ", "  #"],
        'R' => ["## ", "# #", "## ", "# #", "# #"],
        'S' => [" ##", "#  ", " # ", "  #", "## "],
        'T' => ["###", " # ", " # ", " # ", " # "],
        'U' => ["# #", "# #", "# #", "# #", "###"],
        'V' => ["# #", "# #", "# #", "# #", " # "],
        'W' => ["# #", "# #", "###", "###", "# #"],
        'X' => ["# #", "# #", " # ", "# #", "# #"],
        'Y' => ["# #", "# #", " # ", " # ", " # "],
        'Z' => ["###", "  #", " # ", "#  ", "###"],
        '0' => ["###", "# #", "# #", "# #", "###"],
        '1' => [" # ", "## ", " # ", " # ", "###"],
        '2' => ["###", "  #", "###", "#  ", "###"],
        '3' => ["###", "  #", "###", "  #", "###"],
        '4' => ["# #", "# #", "###", "  #", "  #"],
        '5' => ["###", "#  ", "###", "  #", "###"],
        '6' => ["###", "#  ", "###", "# #", "###"],
        '7' => ["###", "  #", "  #", "  #", "  #"],
        '8' => ["###", "# #", "###", "# #", "###"],
        '9' => ["###", "# #", "###", "  #", "###"],
        ':' => ["   ", " # ", "   ", " # ", "   "],
        '.' => ["   ", "   ", "   ", "   ", " # "],
        ',' => ["   ", "   ", "   ", " # ", "#  "],
        '-' => ["   ", "   ", "###", "   ", "   "],
        '+' => ["   ", " # ", "###", " # ", "   "],
        '!' => [" # ", " # ", " # ", "   ", " # "],
        '?' => ["## ", "  #", " # ", "   ", " # "],
        '/' => ["  #", "  #", " # ", "#  ", "#  "],
        '\'' => [" # ", " # ", "   ", "   ", "   "],
        ' ' => ["   ", "   ", "   ", "   ", "   "],
        _ => ["###", "###", "###", "###", "###"],
    }
}

/// Pixel rows of `text`: five strings of `#` and spaces, one blank column
/// between glyphs.
fn pixels(text: &str) -> [String; 5] {
    let mut rows: [String; 5] = Default::default();
    for (i, c) in text.chars().enumerate() {
        let g = glyph(c);
        for (row, line) in rows.iter_mut().zip(g) {
            if i > 0 {
                row.push(' ');
            }
            row.push_str(line);
        }
    }
    rows
}

/// Rows of cells for `text` at `scale`.
pub fn render(text: &str, scale: usize) -> Vec<String> {
    let px = pixels(text);
    let on = |row: usize, col: usize| px.get(row).is_some_and(|r| r.as_bytes()[col] == b'#');
    let width = px[0].len();
    if scale <= 1 {
        return (0..3)
            .map(|cell| {
                (0..width)
                    .map(|col| match (on(cell * 2, col), on(cell * 2 + 1, col)) {
                        (true, true) => '█',
                        (true, false) => '▀',
                        (false, true) => '▄',
                        (false, false) => ' ',
                    })
                    .collect()
            })
            .collect();
    }
    let s = scale - 1;
    let mut out = Vec::new();
    for row in 0..5 {
        let line: String = (0..width)
            .flat_map(|col| std::iter::repeat_n(if on(row, col) { '█' } else { ' ' }, 2 * s))
            .collect();
        out.extend(std::iter::repeat_n(line, s));
    }
    out
}

impl DrawExt for BigText {
    fn name(&self) -> &'static str {
        "bigtext"
    }

    fn functions(&self) -> Vec<FnDoc> {
        vec![FnDoc {
            name: "text",
            params: "text, scale (1..4), fg, bg",
            doc: "Block letters: A-Z, 0-9 and some punctuation.",
        }]
    }

    fn draw(&self, function: &str, params: &Value) -> Result<SpriteSpec, String> {
        if function != "text" {
            return Err(format!("no function {function}"));
        }
        let text = params
            .get("text")
            .and_then(Value::as_str)
            .ok_or("text: required")?;
        if text.is_empty() || text.chars().count() > 64 || text.contains('\n') {
            return Err("text: one line of 1 to 64 characters".into());
        }
        let scale = params.get("scale").and_then(Value::as_u64).unwrap_or(1);
        if !(1..=4).contains(&scale) {
            return Err("scale: 1 to 4".into());
        }
        let mut style = serde_json::Map::new();
        for key in ["fg", "bg", "bold", "dim"] {
            if let Some(v) = params.get(key) {
                style.insert(key.to_string(), v.clone());
            }
        }
        serde_json::from_value(
            json!({"content": {"rows": render(text, scale as usize), "style": style}}),
        )
        .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_blocks() {
        assert_eq!(render("1", 1), vec!["▄█ ", " █ ", "▀▀▀"]);
        assert_eq!(render("12:0", 1)[0].chars().count(), 15);
    }

    #[test]
    fn full_blocks() {
        let rows = render("L", 2);
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0], "██    ");
        assert_eq!(rows[4], "██████");
        assert_eq!(render("L", 3).len(), 10);
    }

    #[test]
    fn params() {
        assert!(
            BigText
                .draw("text", &json!({"text": "ok", "fg": "red"}))
                .is_ok()
        );
        assert!(BigText.draw("text", &json!({})).is_err());
        assert!(
            BigText
                .draw("text", &json!({"text": "a", "scale": 9}))
                .is_err()
        );
    }
}
